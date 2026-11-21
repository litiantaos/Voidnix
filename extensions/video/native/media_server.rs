//! 剪辑预览的本地媒体服务：127.0.0.1 随机端口 + 进程内 token + 单区间 HTTP Range。
//!
//! 为什么不用 Tauri asset protocol：该自定义协议在 macOS/WKWebView 上的媒体 Range 有
//! 单次 1MB 上限与花屏/请求风暴等已知问题（tauri#4133 / #7355 / #10426），且 scheme
//! responder 一次返回完整 body 无法流式。本地 HTTP 走 WebKit 标准媒体管线（与 file://
//! 同级，seek 行为即 Safari 行为），是 Tauri 维护者推荐的媒体播放路径。
//!
//! 安全边界：仅绑 127.0.0.1、端口随机、token 由进程内随机 salt 派生（文件路径永不出现在
//! URL）；只读打开且先过 path_guard；剪辑窗口关闭即停服并清空路由。

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use axum::body::Body;
use axum::extract::{Path as HttpPath, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use serde::Serialize;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

/// 流式读块的粒度（与 HTTP 常见媒体分块同量级，避免整文件读入内存）。
const READ_CHUNK: u64 = 256 * 1024;

#[derive(Default)]
struct Routes {
    /// token → 文件路径
    by_token: HashMap<String, PathBuf>,
    /// 文件路径 → token（同一文件重复登记复用 token）
    by_path: HashMap<PathBuf, String>,
    /// 进程内随机 salt：token 不可跨进程预测
    salt: [u8; 32],
}

struct MediaServer {
    origin: String,
    shutdown: CancellationToken,
    task: tauri::async_runtime::JoinHandle<()>,
}

#[derive(Default)]
struct MediaRuntime {
    shared: Option<Arc<Mutex<Routes>>>,
    server: Option<MediaServer>,
}

fn runtime() -> &'static Mutex<MediaRuntime> {
    static RUNTIME: OnceLock<Mutex<MediaRuntime>> = OnceLock::new();
    RUNTIME.get_or_init(|| Mutex::new(MediaRuntime::default()))
}

/// 前端拿到的可播放引用。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaItem {
    pub path: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaOpen {
    pub origin: String,
    pub items: Vec<MediaItem>,
}

/// 读 32 字节系统随机源；失败退化为时间 + pid（token 仍不可跨进程复用）。
fn random_salt() -> [u8; 32] {
    let mut salt = [0u8; 32];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        if std::io::Read::read_exact(&mut f, &mut salt).is_ok() {
            return salt;
        }
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id() as u128;
    let mixed = nanos.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ pid;
    for (i, b) in salt.iter_mut().enumerate() {
        *b = (mixed >> ((i % 16) * 8)) as u8 ^ (i as u8);
    }
    salt
}

fn token_for(salt: &[u8; 32], path: &Path) -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    salt.hash(&mut h);
    path.hash(&mut h);
    format!("{:016x}", h.finish())
}

fn mime_for(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("mp4") | Some("m4v") => "video/mp4",
        Some("mov") | Some("qt") => "video/quicktime",
        Some("webm") => "video/webm",
        Some("mkv") => "video/x-matroska",
        Some("m4a") => "audio/mp4",
        Some("mp3") => "audio/mpeg",
        Some("wav") => "audio/wav",
        _ => "application/octet-stream",
    }
}

/// 解析单区间 `bytes=`（浏览器媒体请求只发单区间）。返回 None = 语法无效。
fn parse_range(raw: &str, len: u64) -> Option<Option<(u64, u64)>> {
    let spec = raw.trim().strip_prefix("bytes=")?;
    if spec.contains(',') {
        // 多区间不支持：按无效处理，回 416 会让浏览器重试整段请求
        return None;
    }
    let (a, b) = spec.split_once('-')?;
    let (start, end) = if a.is_empty() {
        // suffix：最后 N 字节
        let n: u64 = b.trim().parse().ok()?;
        if n == 0 || len == 0 {
            return None;
        }
        (len.saturating_sub(n), len - 1)
    } else {
        let s: u64 = a.trim().parse().ok()?;
        let e = if b.trim().is_empty() {
            len.saturating_sub(1)
        } else {
            b.trim().parse().ok()?
        };
        (s, e.min(len.saturating_sub(1)))
    };
    if len == 0 || start > end || start >= len {
        return None;
    }
    Some(Some((start, end)))
}

async fn stream_file(path: &Path, start: u64, length: u64) -> std::io::Result<Body> {
    let mut file = tokio::fs::File::open(path).await?;
    file.seek(std::io::SeekFrom::Start(start)).await?;
    let stream = futures_util::stream::unfold((file, length), |(mut f, remain)| async move {
        if remain == 0 {
            return None;
        }
        let want = remain.min(READ_CHUNK);
        let mut buf = vec![0u8; want as usize];
        match f.read(&mut buf).await {
            Ok(0) => None,
            Ok(n) => {
                buf.truncate(n);
                Some((
                    Ok::<_, std::io::Error>(axum::body::Bytes::from(buf)),
                    (f, remain - n as u64),
                ))
            }
            Err(e) => Some((Err(e), (f, 0))),
        }
    });
    Ok(Body::from_stream(stream))
}

async fn handle_media(
    State(shared): State<Arc<Mutex<Routes>>>,
    HttpPath(token): HttpPath<String>,
    headers: HeaderMap,
) -> Response {
    let Some(path) = ({
        let routes = shared.lock().ok();
        routes.and_then(|r| r.by_token.get(&token).cloned())
    }) else {
        return (StatusCode::NOT_FOUND, "media not found").into_response();
    };
    let Ok(meta) = tokio::fs::metadata(&path).await else {
        return (StatusCode::NOT_FOUND, "file not found").into_response();
    };
    let len = meta.len();
    let content_type = mime_for(&path);
    let range_header = headers
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);

    let mut builder = Response::builder().header(header::CONTENT_TYPE, content_type);
    builder = builder
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(
            header::ACCESS_CONTROL_EXPOSE_HEADERS,
            "Content-Range, Content-Length, Accept-Ranges",
        );

    let Some(range_header) = range_header else {
        // 无 Range（少数播放器/预探测）：整段流式返回
        return match stream_file(&path, 0, len).await {
            Ok(body) => builder
                .header(header::CONTENT_LENGTH, len.to_string())
                .body(body)
                .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response()),
            Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        };
    };

    let Some(Some((start, end))) = parse_range(&range_header, len) else {
        return builder
            .status(StatusCode::RANGE_NOT_SATISFIABLE)
            .header(header::CONTENT_RANGE, format!("bytes */{len}"))
            .body(Body::empty())
            .unwrap_or_else(|_| StatusCode::RANGE_NOT_SATISFIABLE.into_response());
    };
    let length = end - start + 1;
    match stream_file(&path, start, length).await {
        Ok(body) => builder
            .status(StatusCode::PARTIAL_CONTENT)
            .header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{len}"))
            .header(header::CONTENT_LENGTH, length.to_string())
            .body(body)
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response()),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

/// 启动（幂等）并登记媒体文件，返回可播放 URL。
///
/// 首次调用绑 127.0.0.1:0 并 spawn axum server；之后复用同一端口与路由表。
pub async fn open(paths: Vec<String>) -> Result<MediaOpen, String> {
    let shared = ensure_server().await?;
    let mut items = Vec::with_capacity(paths.len());
    {
        let mut routes = shared.lock().map_err(|_| "媒体路由锁失效")?;
        for raw in paths {
            let p = PathBuf::from(&raw);
            if !crate::platform::path_guard::validate(&p) {
                return Err(format!("路径不安全或不存在：{raw}"));
            }
            let token = match routes.by_path.get(&p) {
                Some(t) => t.clone(),
                None => {
                    let t = token_for(&routes.salt, &p);
                    routes.by_path.insert(p.clone(), t.clone());
                    routes.by_token.insert(t.clone(), p.clone());
                    t
                }
            };
            let origin = {
                let rt = runtime().lock().map_err(|_| "媒体运行时锁失效")?;
                rt.server
                    .as_ref()
                    .map(|s| s.origin.clone())
                    .ok_or("媒体服务未启动")?
            };
            items.push(MediaItem {
                path: raw,
                url: format!("{origin}/m/{token}"),
            });
        }
    }
    let origin = {
        let rt = runtime().lock().map_err(|_| "媒体运行时锁失效")?;
        rt.server
            .as_ref()
            .map(|s| s.origin.clone())
            .ok_or("媒体服务未启动")?
    };
    Ok(MediaOpen { origin, items })
}

async fn ensure_server() -> Result<Arc<Mutex<Routes>>, String> {
    {
        let rt = runtime().lock().map_err(|_| "媒体运行时锁失效")?;
        if let (Some(shared), Some(_)) = (&rt.shared, &rt.server) {
            return Ok(shared.clone());
        }
    }
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("媒体服务监听失败：{e}"))?;
    let addr = listener
        .local_addr()
        .map_err(|e| format!("媒体服务地址读取失败：{e}"))?;
    let shared = Arc::new(Mutex::new(Routes {
        salt: random_salt(),
        ..Routes::default()
    }));
    let app = Router::new()
        .route("/m/{token}", get(handle_media))
        .with_state(shared.clone());
    let shutdown = CancellationToken::new();
    let token = shutdown.clone();
    let task = tauri::async_runtime::spawn(async move {
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(async move { token.cancelled().await })
            .await;
    });

    let mut rt = runtime().lock().map_err(|_| "媒体运行时锁失效")?;
    // 双检：并发首启时保留先到者（后到者放弃自己的 server）
    if let (Some(existing), Some(_)) = (&rt.shared, &rt.server) {
        shutdown.cancel();
        task.abort();
        return Ok(existing.clone());
    }
    rt.shared = Some(shared.clone());
    rt.server = Some(MediaServer {
        origin: format!("http://127.0.0.1:{}", addr.port()),
        shutdown,
        task,
    });
    Ok(shared)
}

/// 停止媒体服务并清空路由（剪辑窗口关闭时调用）。
pub fn stop() {
    let Ok(mut rt) = runtime().lock() else {
        return;
    };
    if let Some(server) = rt.server.take() {
        server.shutdown.cancel();
        server.task.abort();
    }
    rt.shared = None;
}

/// 登记并返回播放 URL；供剪辑窗口前端在打开后调用。
#[tauri::command]
pub async fn video_media_open(paths: Vec<String>) -> Result<MediaOpen, String> {
    open(paths).await
}

/// 停止媒体服务。
#[tauri::command]
pub async fn video_media_close() -> Result<(), String> {
    stop();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_prefix_and_suffix() {
        assert_eq!(parse_range("bytes=0-99", 1000), Some(Some((0, 99))));
        assert_eq!(parse_range("bytes=100-", 1000), Some(Some((100, 999))));
        assert_eq!(parse_range("bytes=-50", 1000), Some(Some((950, 999))));
        // 结尾 clamp
        assert_eq!(parse_range("bytes=900-5000", 1000), Some(Some((900, 999))));
    }

    #[test]
    fn range_invalid() {
        assert_eq!(parse_range("bytes=1000-", 1000), None);
        assert_eq!(parse_range("bytes=0-0,2-3", 1000), None);
        assert_eq!(parse_range("bytes=abc-def", 1000), None);
        assert_eq!(parse_range("seconds=0-1", 1000), None);
        assert_eq!(parse_range("bytes=-0", 1000), None);
    }

    #[test]
    fn token_is_stable_per_path() {
        let salt = [7u8; 32];
        let a = token_for(&salt, Path::new("/tmp/a.mp4"));
        let b = token_for(&salt, Path::new("/tmp/a.mp4"));
        let c = token_for(&salt, Path::new("/tmp/b.mp4"));
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a.len(), 16);
    }

    #[test]
    fn mime_mapping() {
        assert_eq!(mime_for(Path::new("/x/a.MP4")), "video/mp4");
        assert_eq!(mime_for(Path::new("/x/a.mov")), "video/quicktime");
        assert_eq!(
            mime_for(Path::new("/x/a.unknown")),
            "application/octet-stream"
        );
    }
}
