//! AI 网关服务器:Anthropic Messages / OpenAI Responses / OpenAI Chat Completions 三协议直通反向代理。
//!
//! 路由 = 请求体 `model` 字段 → 提供商;每提供商多 Key 轮换(429/401/403/503/529 换下一把重发,
//! lastGood 粘性优先)。请求体整体缓冲以支持换 Key 重放,响应(SSE 流式)透传不落盘。

use axum::body::{Body, Bytes};
use axum::extract::{Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::{get, post};
use axum::Router;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex, RwLock};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

/// release 8788(固定端口,存量 CC 配置零改动);
/// dev 8789(dev 与 release 常驻并存,不互抢端口)
pub const PORT: u16 = if cfg!(debug_assertions) { 8789 } else { 8788 };

/// 请求体缓冲上限:CC 长上下文请求可达数十 MB,128MB 兜底异常超大请求
const MAX_BODY_BYTES: usize = 128 * 1024 * 1024;

/// 轮换触发的上游状态码:容量满(429/503/529)、鉴权失效(401/403)——换 Key 常可恢复。
/// 529 为 Anthropic 生态过载语义,智谱同款。
/// 轮换判定(参考 magpie fallback 集):401-408(鉴权/欠费/模型权限/超时)与 429 可换 Key 重发,
/// 5xx 同理;全部 Key 失败后回放最后错误,请求级错误(如 400 参数)不会被误伤——不在此集
fn rotatable(status: u16) -> bool {
    matches!(status, 401..=408 | 429) || status >= 500
}

/// 上游错误体缓冲上限:轮换失败后回放最后一个错误响应给客户端(错误体很小,64KB 足够)
const MAX_ERROR_BODY: usize = 64 * 1024;

// ─── 数据结构(与前端 logic.ts 同构,camelCase)──────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayKey {
    pub label: String,
    pub api_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayRoute {
    pub provider_id: String,
    pub name: String,
    /// Anthropic Messages 线协议端点(空 = 该提供商不支持 /v1/messages 直通)
    pub anthropic_url: String,
    /// OpenAI Responses 线协议端点(空 = 该提供商不支持 /v1/responses 直通)
    pub responses_url: String,
    /// OpenAI Chat Completions 线协议端点(空 = 该提供商不支持 /v1/chat/completions 直通)
    pub chat_url: String,
    pub models: Vec<String>,
    pub keys: Vec<GatewayKey>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayStatus {
    pub running: bool,
    pub port: u16,
    pub route_count: usize,
    pub bind_error: Option<String>,
}

/// 持久化快照(extensions/ai-gateway/gateway-state.json):app 重启后前端就绪前
/// 由 Rust 侧直接拉起服务器,消除冷启动窗口内 CC 断连。
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedState {
    enabled: bool,
    routes: Vec<GatewayRoute>,
}

fn state_file(dir: &Path) -> PathBuf {
    dir.join("gateway-state.json")
}

fn persist_state(dir: &Path, state: &PersistedState) -> Result<(), String> {
    let path = state_file(dir);
    let text = serde_json::to_string_pretty(state).map_err(|e| e.to_string())?;
    crate::runtime::storage::atomic_write(&path, &(text + "\n"))
}

fn read_state(dir: &Path) -> Result<PersistedState, String> {
    let text = std::fs::read_to_string(state_file(dir)).map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

// ─── 全局单例 ─────────────────────────────────────────────

pub struct Gateway {
    /// 路由表快照:换表时整体替换 Arc,读侧 clone 一个 Arc 零深拷贝(表含全部 Key 字符串)
    routes: RwLock<Arc<Vec<GatewayRoute>>>,
    /// 提供商 → 上次成功 Key 下标;粘性优先复用,避免每次都撞已满的 Key
    last_good: Mutex<HashMap<String, usize>>,
    /// 轮换失败的 Key 冷却表 (provider, idx) → 解禁时刻;冷却中的 Key 排队尾,
    /// 避免每个请求都为已知失败的 Key 白付一次往返(429 对齐上游 Retry-After)
    cooldowns: Mutex<HashMap<(String, usize), std::time::Instant>>,
    /// 会话亲和:会话指纹 → (提供商, Key 下标, 最近应答);上游 prompt cache 按 Key 隔离,
    /// 同一对话换 Key = 整个前缀 cache 作废全价重算,长会话代价远超粘性收益
    affinity: Mutex<HashMap<u64, (String, usize, std::time::Instant)>>,
    task: Mutex<Option<JoinHandle<()>>>,
    bind_error: RwLock<Option<String>>,
    /// 启停串行锁:bind 是 async,与并发的 sync 调用竞态会双 bind 撞自己端口
    lifecycle: tokio::sync::Mutex<()>,
    /// 排障日志目录(扩展数据目录,update/restore 时注入);None = 冷启动窗口,日志丢弃
    log_dir: RwLock<Option<PathBuf>>,
    /// 日志追加串行锁:proxy 多并发,防交错
    log_lock: Mutex<()>,
}

pub static GATEWAY: LazyLock<Arc<Gateway>> = LazyLock::new(|| {
    Arc::new(Gateway {
        routes: RwLock::new(Arc::new(Vec::new())),
        last_good: Mutex::new(HashMap::new()),
        cooldowns: Mutex::new(HashMap::new()),
        affinity: Mutex::new(HashMap::new()),
        task: Mutex::new(None),
        bind_error: RwLock::new(None),
        lifecycle: tokio::sync::Mutex::new(()),
        log_dir: RwLock::new(None),
        log_lock: Mutex::new(()),
    })
});

/// 会话亲和保持时长:上游 prompt cache 冷却约 5 分钟(Anthropic/OpenAI 最短),
/// 24h 覆盖一个长工作会话的生命周期,过期条目在写入时惰性清理
const AFFINITY_KEEP: std::time::Duration = std::time::Duration::from_secs(24 * 3600);

/// 轮换失败 Key 的默认冷却
const KEY_COOLDOWN: std::time::Duration = std::time::Duration::from_secs(60);

/// 排障日志:数据目录 gateway.log,行式追加(epoch 毫秒 + 类别 + 详情,`date -r 秒` 可转)。
/// 只记异常路径(路由失败/上游错误/网络错误/Key 耗尽),成功请求零记录;
/// 不含 Key 明文与请求体;超 512KB 整文件重置(自旋转)。
fn log_gateway(kind: &str, detail: &str) {
    let Some(dir) = GATEWAY.log_dir.read().unwrap().clone() else {
        return;
    };
    let _guard = GATEWAY.log_lock.lock().unwrap();
    let path = dir.join("gateway.log");
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let line = format!("[{ms}] {kind} {detail}\n");
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        if f.metadata().map(|m| m.len()).unwrap_or(0) > 512 * 1024 {
            let _ = std::fs::write(&path, &line);
        } else {
            let _ = f.write_all(line.as_bytes());
        }
    }
}

impl Gateway {
    /// 前端单一同步入口:换路由表 + 持久化 + 按需启停服务器。
    pub async fn update(&self, enabled: bool, routes: Vec<GatewayRoute>, dir: &Path) {
        let dir = dir.to_path_buf();
        *self.log_dir.write().unwrap() = Some(dir.clone());
        let persisted = PersistedState {
            enabled,
            routes: routes.clone(),
        };
        *self.routes.write().unwrap() = Arc::new(routes);
        if let Err(e) = tokio::task::spawn_blocking(move || persist_state(&dir, &persisted))
            .await
            .unwrap_or_else(|e| Err(e.to_string()))
        {
            log::warn!("[ai-gateway] 状态持久化失败: {e}");
        }
        if enabled {
            self.ensure_server().await;
        } else {
            self.stop_server();
        }
    }

    /// Rust 侧冷启动恢复:读快照,enabled 则直接拉起(路由表先就位,前端就绪后再刷新)。
    pub async fn restore_from(&self, dir: &Path) {
        let Ok(state) = read_state(dir) else {
            return;
        };
        *self.log_dir.write().unwrap() = Some(dir.to_path_buf());
        *self.routes.write().unwrap() = Arc::new(state.routes);
        if state.enabled {
            self.ensure_server().await;
        }
    }

    async fn ensure_server(&self) {
        // 串行化检查 + bind:并发 sync 在此排队,后来者见 task 已立即返回(并清粘滞错误)
        let _guard = self.lifecycle.lock().await;
        if self.task.lock().unwrap().is_some() {
            *self.bind_error.write().unwrap() = None;
            return;
        }
        match TcpListener::bind(("127.0.0.1", PORT)).await {
            Ok(listener) => {
                *self.bind_error.write().unwrap() = None;
                let handle = tokio::spawn(serve_until_exit(listener));
                *self.task.lock().unwrap() = Some(handle);
                log::info!("[ai-gateway] 网关已启动: 127.0.0.1:{PORT}");
            }
            Err(e) => {
                *self.bind_error.write().unwrap() =
                    Some(format!("端口 {PORT} 绑定失败: {e}(被其它进程占用)"));
            }
        }
    }

    fn stop_server(&self) {
        if let Some(handle) = self.task.lock().unwrap().take() {
            handle.abort();
            log::info!("[ai-gateway] 网关已停止");
        }
        *self.bind_error.write().unwrap() = None;
    }

    /// 读侧快照:clone Arc 零深拷贝
    fn routes_snapshot(&self) -> Arc<Vec<GatewayRoute>> {
        Arc::clone(&self.routes.read().unwrap())
    }

    pub fn status(&self) -> GatewayStatus {
        GatewayStatus {
            running: self.task.lock().unwrap().is_some(),
            port: PORT,
            route_count: self.routes_snapshot().len(),
            bind_error: self.bind_error.read().unwrap().clone(),
        }
    }

    /// Key 尝试顺序:会话亲和(粘住该会话上次应答的 Key,保上游 prompt cache) >
    /// lastGood 粘性 > 配置序;冷却中的 Key 整体排到队尾(段内保持相对顺序)
    fn key_order(&self, route: &GatewayRoute, session: Option<u64>) -> Vec<usize> {
        let mut order: Vec<usize> = (0..route.keys.len()).collect();
        let sticky = session
            .and_then(|s| {
                let aff = self.affinity.lock().unwrap();
                aff.get(&s)
                    .filter(|(pid, idx, at)| {
                        pid == &route.provider_id
                            && *idx < route.keys.len()
                            && at.elapsed() < AFFINITY_KEEP
                    })
                    .map(|(_, idx, _)| *idx)
            })
            .or_else(|| {
                self.last_good
                    .lock()
                    .unwrap()
                    .get(&route.provider_id)
                    .copied()
                    .filter(|&lg| lg < route.keys.len())
            });
        if let Some(s) = sticky {
            order.retain(|&i| i != s);
            order.insert(0, s);
        }
        // 冷却段排尾:非冷却段(粘性序) + 冷却段(同相对序),全部冷却时退化为原序
        let now = std::time::Instant::now();
        let cooling: std::collections::HashSet<usize> = self
            .cooldowns
            .lock()
            .unwrap()
            .iter()
            .filter(|((pid, _), until)| pid == &route.provider_id && **until > now)
            .map(|((_, idx), _)| *idx)
            .collect();
        if !cooling.is_empty() && cooling.len() < order.len() {
            let (hot, cold): (Vec<_>, Vec<_>) =
                order.into_iter().partition(|i| !cooling.contains(i));
            order = [hot, cold].concat();
        }
        order
    }

    fn mark_good(&self, provider_id: &str, idx: Option<usize>) {
        let mut map = self.last_good.lock().unwrap();
        match idx {
            Some(i) => {
                map.insert(provider_id.to_string(), i);
            }
            None => {
                map.remove(provider_id);
            }
        }
    }

    /// 轮换失败的 Key 进冷却(429 对齐上游 Retry-After,上限默认值)
    fn mark_cool(&self, provider_id: &str, idx: usize, until: std::time::Instant) {
        self.cooldowns
            .lock()
            .unwrap()
            .insert((provider_id.to_string(), idx), until);
    }

    /// 会话应答成功:绑定亲和并顺带清理过期条目(低频写入,全扫成本可忽略)
    fn bind_session(&self, session: u64, provider_id: &str, idx: usize) {
        let mut aff = self.affinity.lock().unwrap();
        aff.retain(|_, (_, _, at)| at.elapsed() < AFFINITY_KEEP);
        aff.insert(
            session,
            (provider_id.to_string(), idx, std::time::Instant::now()),
        );
    }
}

// ─── 服务器 ───────────────────────────────────────────────

async fn serve_until_exit(listener: TcpListener) {
    let app = Router::new()
        .route("/v1/messages", post(proxy_anthropic))
        // CC 上下文核算调用同协议透传(请求体含 model,路由逻辑一致)
        .route("/v1/messages/count_tokens", post(proxy_anthropic))
        .route("/v1/responses", post(proxy_responses))
        .route("/responses", post(proxy_responses))
        .route("/v1/chat/completions", post(proxy_chat))
        .route("/chat/completions", post(proxy_chat))
        // OpenAI 系工具常先列模型再请求;CC 的 discovery 不用此端点(modelPicker 注入)
        .route("/v1/models", get(list_models))
        .route("/health", get(health))
        .with_state(Arc::clone(&*GATEWAY));
    if let Err(e) = axum::serve(listener, app).await {
        *GATEWAY.bind_error.write().unwrap() = Some(format!("服务器异常退出: {e}"));
        *GATEWAY.task.lock().unwrap() = None;
    }
}

async fn health(State(g): State<Arc<Gateway>>) -> Response {
    let routes = g.routes_snapshot();
    let body = format!(
        "ok (providers: {}, models: {})",
        routes.len(),
        routes.iter().map(|r| r.models.len()).sum::<usize>()
    );
    plain_response(StatusCode::OK, body)
}

/// GET /v1/models:OpenAI 形状模型清单(全部可路由模型,跨协议并集)
async fn list_models(State(g): State<Arc<Gateway>>) -> Response {
    let routes = g.routes_snapshot();
    let data: Vec<serde_json::Value> = routes
        .iter()
        .flat_map(|r| r.models.iter().map(|m| norm_model(m)))
        .map(|id| {
            serde_json::json!({ "id": id, "object": "model", "owned_by": r_owned(routes.as_slice(), id) })
        })
        .collect();
    let payload = serde_json::json!({ "object": "list", "data": data });
    Response::builder()
        .status(StatusCode::OK)
        .header(axum::http::header::CONTENT_TYPE, "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap_or_else(|_| empty_error())
}

/// 模型 → 提供商名(/v1/models 的 owned_by)
fn r_owned<'a>(routes: &'a [GatewayRoute], model: &str) -> &'a str {
    routes
        .iter()
        .find(|r| r.models.iter().any(|m| norm_model(m) == model))
        .map(|r| r.name.as_str())
        .unwrap_or("")
}

async fn proxy_anthropic(State(g): State<Arc<Gateway>>, req: Request) -> Response {
    proxy(g, Protocol::Anthropic, req).await
}

async fn proxy_responses(State(g): State<Arc<Gateway>>, req: Request) -> Response {
    proxy(g, Protocol::Responses, req).await
}

async fn proxy_chat(State(g): State<Arc<Gateway>>, req: Request) -> Response {
    proxy(g, Protocol::Chat, req).await
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Protocol {
    Anthropic,
    Responses,
    Chat,
}

impl Protocol {
    fn upstream_base(self, route: &GatewayRoute) -> &str {
        match self {
            Protocol::Anthropic => &route.anthropic_url,
            Protocol::Responses => &route.responses_url,
            Protocol::Chat => &route.chat_url,
        }
    }

    /// 上游鉴权头族:Anthropic 端点双头(x-api-key + Bearer,智谱/DeepSeek 兼容端点实测接受);
    /// OpenAI 族只注 Bearer(x-api-key 对 OpenAI 语义是无效头,严格网关可能拒绝)
    fn dual_auth(self) -> bool {
        matches!(self, Protocol::Anthropic)
    }

    fn error_kind(self) -> ErrorShape {
        match self {
            Protocol::Anthropic => ErrorShape::Anthropic,
            _ => ErrorShape::OpenAi,
        }
    }
}

/// 上游 URL 拼接:剥客户端 /v1 前缀(OpenAI 族端点自带 /v1 惯例)+ 端点尾缀防重
/// (表单示例是完整端点如 `.../v1/responses`,基目录拼 `/responses` 会双路径)
fn join_upstream_url(protocol: Protocol, base: &str, path_and_query: &str) -> String {
    let stripped = match protocol {
        // Anthropic 端点惯例不带 /v1,客户端路径原样;但端点若以 /v1 结尾则剥掉防 /v1/v1
        Protocol::Anthropic => path_and_query,
        _ => path_and_query.strip_prefix("/v1").unwrap_or(path_and_query),
    };
    let mut base = base.trim_end_matches('/');
    match protocol {
        Protocol::Anthropic => {
            if base.ends_with("/v1") {
                base = &base[..base.len() - 3];
            }
        }
        Protocol::Responses => {
            if base.ends_with("/responses") {
                base = &base[..base.len() - "/responses".len()];
            }
        }
        Protocol::Chat => {
            if let Some(stripped_base) = base.strip_suffix("/chat/completions") {
                base = stripped_base;
            }
        }
    }
    format!("{base}{stripped}")
}

async fn proxy(g: Arc<Gateway>, protocol: Protocol, req: Request) -> Response {
    let (parts, body_raw) = req.into_parts();
    // 请求体整体缓冲:换 Key 重放需要完整 body(SSE 是响应侧流式,不受影响)
    let body = match axum::body::to_bytes(body_raw, MAX_BODY_BYTES).await {
        Ok(b) => b,
        Err(e) => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "invalid_request_error",
                &format!("请求体读取失败: {e}"),
                protocol,
            )
        }
    };

    let Some(model) = extract_model(&body) else {
        log_gateway("route", "请求缺少 model 字段");
        return error_response(
            StatusCode::BAD_REQUEST,
            "invalid_request_error",
            "请求缺少 model 字段,网关无法路由",
            protocol,
        );
    };

    // 请求体归一:剥 [1m] 后缀 + Anthropic 面注入 thinking disabled + GLM effort 翻译 + 输出预算下限
    let session = session_hash(&body);
    let body = normalize_body(protocol, &model, body);

    let routes = g.routes_snapshot();
    let Some(route) = find_route(&routes, &model) else {
        log_gateway(
            "route",
            &format!(
                "未知模型 {model} · 可用: {}",
                available_models(&routes, protocol)
            ),
        );
        return error_response(
            StatusCode::NOT_FOUND,
            "invalid_request_error",
            &format!(
                "未知模型 {model},网关可用: {}",
                available_models(&routes, protocol)
            ),
            protocol,
        );
    };

    let base = protocol.upstream_base(route);
    if base.trim().is_empty() {
        log_gateway(
            "route",
            &format!("{} 未声明 {:?} 协议端点", route.name, protocol),
        );
        return error_response(
            StatusCode::NOT_FOUND,
            "invalid_request_error",
            &format!(
                "提供商 {} 未声明该协议端点(在 AI 提供商中补全 URL)",
                route.name
            ),
            protocol,
        );
    }

    let path_and_query = parts
        .uri
        .path_and_query()
        .map(|pq| pq.as_str().to_string())
        .unwrap_or_default();
    let url = join_upstream_url(protocol, base, &path_and_query);
    let headers = forward_headers(&parts.headers, body.len());

    let order = g.key_order(route, session);
    let mut last_error: Option<(u16, Bytes)> = None;
    for idx in order {
        let key = &route.keys[idx];
        let mut req = crate::http::stream_client()
            .post(&url)
            .headers(headers.clone())
            .bearer_auth(&key.api_key);
        if protocol.dual_auth() {
            req = req.header("x-api-key", &key.api_key);
        }
        let resp = req.body(body.clone()).send().await;
        match resp {
            Err(e) => {
                log::warn!("[ai-gateway] {} · {}: 网络错误 {e}", route.name, key.label);
                log_gateway(
                    "net",
                    &format!("{model} · {} · {}: {e}", route.name, key.label),
                );
                continue;
            }
            Ok(r) => {
                let status = r.status().as_u16();
                if rotatable(status) {
                    // 缓存错误响应兜底(全部 Key 失败时回放给客户端)
                    let retry_after = r
                        .headers()
                        .get(axum::http::header::RETRY_AFTER)
                        .and_then(|v| v.to_str().ok())
                        .and_then(|s| s.trim().parse::<u64>().ok())
                        .map_or(KEY_COOLDOWN, |s| {
                            std::time::Duration::from_secs(s).max(KEY_COOLDOWN)
                        });
                    let cached = read_capped(r, MAX_ERROR_BODY).await.ok();
                    if let Some(buf) = &cached {
                        last_error = Some((status, buf.clone()));
                    }
                    let snippet = cached
                        .as_ref()
                        .map(|b| {
                            String::from_utf8_lossy(b)
                                .chars()
                                .take(160)
                                .collect::<String>()
                        })
                        .unwrap_or_default();
                    log_gateway(
                        "upstream",
                        &format!(
                            "{model} · {} · {}: {status} {snippet}",
                            route.name, key.label
                        ),
                    );
                    log::warn!(
                        "[ai-gateway] {} · {}: {status},换下一把 Key",
                        route.name,
                        key.label
                    );
                    g.mark_good(&route.provider_id, None);
                    g.mark_cool(
                        &route.provider_id,
                        idx,
                        std::time::Instant::now() + retry_after,
                    );
                    continue;
                }
                g.mark_good(&route.provider_id, Some(idx));
                if let Some(s) = session {
                    g.bind_session(s, &route.provider_id, idx);
                }
                if status >= 400 {
                    // 非轮换错误(如模型名 400)直接透传客户端:Key 本身没坏不轮换,
                    // 但必须落日志——CC 报 unavailable 而网关日志为空的盲区即此处
                    log_gateway(
                        "errpass",
                        &format!("{model} · {}: {status} 透传(不轮换)", route.name),
                    );
                }
                return stream_response(r).await;
            }
        }
    }

    if let Some((status, buf)) = last_error {
        // 回放最后一个上游错误(429 等),客户端能看到真实原因
        log_gateway("replay", &format!("{model} · {} 回放 {status}", route.name));
        return Response::builder()
            .status(StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY))
            .header(axum::http::header::CONTENT_TYPE, "application/json")
            .body(Body::from(buf))
            .unwrap_or_else(|_| empty_error());
    }
    log_gateway(
        "exhaust",
        &format!("{model} · {} 的全部 Key 均不可用(网络错误)", route.name),
    );
    error_response(
        StatusCode::BAD_GATEWAY,
        "api_error",
        &format!(
            "提供商 {} 的全部 Key 均不可用(网络错误或额度耗尽)",
            route.name
        ),
        protocol,
    )
}

/// 透传上游响应:状态 + 头(剥跳-by-hop)原样,SSE 字节流直 pipe。
async fn stream_response(upstream: reqwest::Response) -> Response {
    let status = upstream.status();
    let mut headers = HeaderMap::new();
    for (name, value) in upstream.headers() {
        if !is_hop_by_hop(name.as_str()) {
            headers.insert(name, value.clone());
        }
    }
    let stream = upstream
        .bytes_stream()
        .map(|chunk| chunk.map_err(|e| std::io::Error::other(e.to_string())));
    Response::builder()
        .status(status)
        // hyper 对无 content-length 的流式体自动 chunked,与上游传输语义一致
        .body(Body::from_stream(stream))
        .unwrap_or_else(|_| empty_error())
}

/// 读取响应体并截断到上限(轮换失败的错误体缓存用;正常路径不走这里)
async fn read_capped(resp: reqwest::Response, cap: usize) -> Result<Bytes, ()> {
    let mut buf: Vec<u8> = Vec::new();
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| ())?;
        if buf.len() + chunk.len() > cap {
            buf.extend_from_slice(&chunk[..cap - buf.len()]);
            break;
        }
        buf.extend_from_slice(&chunk);
    }
    Ok(buf.into())
}

/// 构造转发头:透传客户端头,剥 host/鉴权/逐跳与长度相关头(长度按缓冲后的 body 重算)
fn forward_headers(src: &HeaderMap, body_len: usize) -> HeaderMap {
    let mut out = HeaderMap::new();
    for (name, value) in src {
        let n = name.as_str();
        if is_hop_by_hop(n)
            || n == "host"
            || n == "authorization"
            || n == "x-api-key"
            || n == "content-length"
        {
            continue;
        }
        out.insert(name.clone(), value.clone());
    }
    if body_len > 0 {
        if let Ok(v) = axum::http::HeaderValue::from_str(&body_len.to_string()) {
            out.insert(axum::http::header::CONTENT_LENGTH, v);
        }
    }
    out
}

fn is_hop_by_hop(name: &str) -> bool {
    matches!(
        name,
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
    )
}

/// 只取 model 字段的最小反序列化目标:零未知字段树分配(长上下文请求体可达数十 MB,
/// 全量建 serde_json::Value 树每请求付出 2-3 倍 body 的分配)
#[derive(Deserialize)]
struct ModelOnly {
    model: String,
}

/// 会话指纹:messages[0] 的 hash(RawValue 零拷贝)。会话是 append-only,首条消息全程不变,
/// 同一会话的各轮请求得到同一指纹;分类器等独立请求指纹唯一,绑定后无后续命中,无害
fn session_hash(body: &[u8]) -> Option<u64> {
    #[derive(Deserialize)]
    struct Probe<'a> {
        #[serde(default, borrow)]
        messages: Vec<&'a serde_json::value::RawValue>,
    }
    let probe: Probe = serde_json::from_slice(body).ok()?;
    let first = probe.messages.first()?;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    std::hash::Hasher::write(&mut h, first.get().as_bytes());
    Some(std::hash::Hasher::finish(&h))
}

/// 从请求体 JSON 提取 model 字段(三种协议请求同名字段)
fn extract_model(body: &[u8]) -> Option<String> {
    let parsed: ModelOnly = serde_json::from_slice(body).ok()?;
    let trimmed = parsed.model.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// 模型名归一:剥 `[1m]` 长上下文后缀(CC 发送前已剥,中枢侧可能带后缀存储,双向归一比对)
fn norm_model(m: &str) -> &str {
    let t = m.trim();
    t.strip_suffix("[1m]").unwrap_or(t).trim()
}

fn find_route<'a>(routes: &'a [GatewayRoute], model: &str) -> Option<&'a GatewayRoute> {
    let target = norm_model(model);
    routes
        .iter()
        .find(|r| r.models.iter().any(|m| norm_model(m) == target))
}

/// 请求体归一(唯一的请求体改写,三件事):
/// 1. 剥 model 的 `[1m]` 客户端后缀(全协议面)——CC 主对话发送前自剥 + 发 beta 头,
///    但其分类器等旁路请求原样带后缀,上游不认识该语法必报「模型不存在」;网关路由
///    匹配时已归一,转发时同样归一,客户端怪癖在网关侧吸收
/// 2. Anthropic 面注入 `thinking: {"type": "disabled"}`(请求未提 thinking 且非 claude 原生模型):
///    原生 Anthropic 语义即「无该字段 = 不思考」,但兼容端点默认思考(智谱/DeepSeek)——CC 分类器/
///    标题等旁路请求不带 thinking,输出预算被思考耗尽产出空 text。DeepSeek 认此参数彻底关思考
///    (实测 blocks 仅 text);智谱忽略它,由下述预算下限兜底(参考 magpie 的 thinkingOffUnlessAsked)
/// 3. Anthropic 面输出预算下限:对未显式声明 thinking 且 max_tokens 低于 256 的请求提升预算——
///    智谱忽略 disabled 依然思考且计入 max_tokens,小预算请求仍会被耗尽,下限保证 text 有出口
///    (模型答完即停,不产生额外消耗;flash 档分类任务实测思考约 150 token,256 留有余量)
const ANTHROPIC_MIN_OUTPUT_TOKENS: u64 = 256;

/// 输出预算探测(部分反序列化,未知字段流式跳过零建树;大请求体的线性扫描成本远低于网络传输)
#[derive(Deserialize)]
struct OutputPrefs {
    #[serde(default)]
    max_tokens: Option<u64>,
    /// 解析 thinking:存在性判定「是否注入 disabled」,budget 供 GLM effort 翻译
    #[serde(default)]
    thinking: Option<ThinkingSpec>,
}

#[derive(Deserialize)]
struct ThinkingSpec {
    /// 显式声明的 thinking 类型(存在性即「客户端自管」,类型值本身不消费)
    #[serde(rename = "type")]
    #[expect(dead_code)]
    kind: String,
    #[serde(default)]
    budget_tokens: Option<u64>,
}

/// GLM 5.2/5.3:思考强度不认 thinking.budget_tokens(实测 50 与 8000 无控制力),
/// 走 output_config.effort(实测 low/high 思考量差 3.5 倍),参考 magpie effortInOutputConfig
fn glm_effort_model(model: &str) -> bool {
    let m = model.to_lowercase();
    m.contains("glm-5.2") || m.contains("glm-5.3")
}

/// thinking budget(CC 的 EFFORT_LEVEL 翻译产物)→ GLM effort 档
fn budget_to_effort(budget: u64) -> &'static str {
    if budget >= 10_000 {
        "high"
    } else if budget >= 4_000 {
        "medium"
    } else {
        "low"
    }
}

fn normalize_body(protocol: Protocol, model: &str, body: Bytes) -> Bytes {
    let strip_suffix = model != norm_model(model);
    // claude 原生模型按原样发送(原生端点本就「不问不思考」,无需注入)
    let native_claude = model.to_lowercase().contains("claude-");
    let prefs = serde_json::from_slice::<OutputPrefs>(&body).ok();
    let no_thinking = prefs.as_ref().is_some_and(|p| p.thinking.is_none());
    let needs_disable = protocol == Protocol::Anthropic && !native_claude && no_thinking;
    let needs_boost = protocol == Protocol::Anthropic
        && no_thinking
        && prefs.as_ref().is_some_and(|p| {
            p.max_tokens
                .is_some_and(|m| m < ANTHROPIC_MIN_OUTPUT_TOKENS)
        });
    // GLM:thinking 带预算时翻译 output_config.effort(智谱忽略 budget,不翻则强度形同虚设)
    let effort = match (&prefs, protocol) {
        (Some(p), Protocol::Anthropic) if glm_effort_model(norm_model(model)) => p
            .thinking
            .as_ref()
            .and_then(|t| t.budget_tokens)
            .map(budget_to_effort),
        _ => None,
    };
    if !strip_suffix && !needs_disable && !needs_boost && effort.is_none() {
        return body;
    }
    let Ok(mut root) = serde_json::from_slice::<Value>(&body) else {
        return body;
    };
    if strip_suffix {
        root["model"] = Value::String(norm_model(model).to_string());
    }
    if needs_disable {
        root["thinking"] = serde_json::json!({ "type": "disabled" });
    }
    if let Some(e) = effort {
        let oc = root
            .get("output_config")
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default();
        let mut oc = oc;
        oc.insert("effort".to_string(), Value::String(e.to_string()));
        root["output_config"] = Value::Object(oc);
    }
    if needs_boost {
        if let Some(m) = root.get("max_tokens").and_then(|v| v.as_u64()) {
            if m < ANTHROPIC_MIN_OUTPUT_TOKENS {
                root["max_tokens"] = Value::from(ANTHROPIC_MIN_OUTPUT_TOKENS);
            }
        }
    }
    serde_json::to_vec(&root).map_or_else(|_| body, Bytes::from)
}

fn available_models(routes: &[GatewayRoute], protocol: Protocol) -> String {
    let mut names: Vec<&str> = Vec::new();
    for r in routes {
        if protocol.upstream_base(r).trim().is_empty() {
            continue;
        }
        names.extend(r.models.iter().map(|m| norm_model(m)));
    }
    if names.is_empty() {
        "（无——请在 AI 提供商中声明对应端点）".to_string()
    } else {
        names.join(", ")
    }
}

// ─── 响应构造 ─────────────────────────────────────────────

/// 错误体形状:按客户端协议族分形,否则 OpenAI 工具按自家约定解析 error.message 得 undefined
enum ErrorShape {
    Anthropic,
    OpenAi,
}

fn error_response(status: StatusCode, kind: &str, message: &str, protocol: Protocol) -> Response {
    let payload = match protocol.error_kind() {
        ErrorShape::Anthropic => serde_json::json!({
            "type": "error",
            "error": { "type": kind, "message": message },
        }),
        ErrorShape::OpenAi => serde_json::json!({
            "error": { "type": kind, "message": message },
        }),
    };
    Response::builder()
        .status(status)
        .header(axum::http::header::CONTENT_TYPE, "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap_or_else(|_| empty_error())
}

fn plain_response(status: StatusCode, text: String) -> Response {
    Response::builder()
        .status(status)
        .header(
            axum::http::header::CONTENT_TYPE,
            "text/plain; charset=utf-8",
        )
        .body(Body::from(text))
        .unwrap_or_else(|_| empty_error())
}

/// Response::builder 失败的兜底(理论不可达:合法 status + 合法 body)
fn empty_error() -> Response {
    let mut r = Response::new(Body::empty());
    *r.status_mut() = StatusCode::BAD_GATEWAY;
    r
}

// ─── 单元测试 ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn route(id: &str, anthropic: &str, models: &[&str]) -> GatewayRoute {
        GatewayRoute {
            provider_id: id.to_string(),
            name: id.to_string(),
            anthropic_url: anthropic.to_string(),
            responses_url: String::new(),
            chat_url: String::new(),
            models: models.iter().map(|s| s.to_string()).collect(),
            keys: vec![GatewayKey {
                label: "k1".into(),
                api_key: "sk-1".into(),
            }],
        }
    }

    #[test]
    fn norm_model_strips_1m_suffix() {
        assert_eq!(norm_model("glm-5.3[1m]"), "glm-5.3");
        assert_eq!(norm_model("glm-5.3"), "glm-5.3");
        assert_eq!(norm_model(" glm-5.3 [1m] "), "glm-5.3");
    }

    #[test]
    fn normalize_body_strips_1m_suffix_on_all_protocols() {
        // 带 [1m] 后缀(含空格形态)剥成裸名;chat 面同样剥(上游不认识客户端语法),
        // 且 chat 面不注入 thinking(协议不同,OpenAI 系工具自管 reasoning)
        let body = br#"{"model":"glm-5.3 [1m]","max_tokens":32000,"messages":[]}"#;
        let out = normalize_body(Protocol::Chat, "glm-5.3 [1m]", Bytes::from_static(body));
        let v: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["model"], "glm-5.3");
        assert_eq!(v.get("thinking"), None);
    }

    #[test]
    fn normalize_body_injects_thinking_disabled_when_unasked() {
        // Anthropic 面未提 thinking:注入 disabled(恢复原生「无字段 = 不思考」语义,
        // DeepSeek 认此参数彻底关思考),预算充足时不提升
        let body = br#"{"model":"glm-5.3","max_tokens":32000,"messages":[]}"#;
        let out = normalize_body(Protocol::Anthropic, "glm-5.3", Bytes::from_static(body));
        let v: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["thinking"]["type"], "disabled");
        assert_eq!(v["max_tokens"], 32000);
        // claude 原生模型不注入(原生端点本就「不问不思考」)
        let body = br#"{"model":"claude-opus-5-5","max_tokens":32000,"messages":[]}"#;
        assert_eq!(
            normalize_body(
                Protocol::Anthropic,
                "claude-opus-5-5",
                Bytes::from_static(body)
            ),
            Bytes::from_static(body)
        );
    }

    #[test]
    fn normalize_body_lifts_small_max_tokens_without_thinking() {
        // 未提 thinking 的小预算:注入 disabled + 提升预算可叠加
        let body = br#"{"model":"glm-5.3-flash","max_tokens":8,"messages":[]}"#;
        let out = normalize_body(
            Protocol::Anthropic,
            "glm-5.3-flash",
            Bytes::from_static(body),
        );
        let v: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["max_tokens"], 256);
        assert_eq!(v["thinking"]["type"], "disabled");
        // 三件事叠加:后缀剥除 + disabled + 预算提升
        let body = br#"{"model":"glm-5.3[1m]","max_tokens":8,"messages":[]}"#;
        let out = normalize_body(Protocol::Anthropic, "glm-5.3[1m]", Bytes::from_static(body));
        let v: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["model"], "glm-5.3");
        assert_eq!(v["max_tokens"], 256);
        assert_eq!(v["thinking"]["type"], "disabled");
    }

    #[test]
    fn normalize_body_respects_explicit_thinking_and_floor() {
        // 显式 thinking(预算自管):不注入 disabled 不提升预算;GLM 模型翻译 output_config.effort
        let body =
            br#"{"model":"glm-5.3","max_tokens":8,"thinking":{"type":"enabled","budget_tokens":1024},"messages":[]}"#;
        let out = normalize_body(Protocol::Anthropic, "glm-5.3", Bytes::from_static(body));
        let v: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["max_tokens"], 8);
        assert_eq!(v["thinking"]["budget_tokens"], 1024);
        assert_eq!(v["output_config"]["effort"], "low");
        // 无 max_tokens(无预算可提升,但 disabled 仍注入)/ 非 JSON(整体放行)
        let body = br#"{"model":"glm-5.3","messages":[]}"#;
        let out = normalize_body(Protocol::Anthropic, "glm-5.3", Bytes::from_static(body));
        let v: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["thinking"]["type"], "disabled");
        let body = br#"not json"#;
        assert_eq!(
            normalize_body(Protocol::Anthropic, "glm-5.3", Bytes::from_static(body)),
            Bytes::from_static(body)
        );
    }

    #[test]
    fn find_route_matches_with_1m_tolerance() {
        let routes = vec![route(
            "zhipu",
            "https://x.cn/api/anthropic",
            &["glm-5.3[1m]"],
        )];
        assert!(find_route(&routes, "glm-5.3").is_some());
        assert!(find_route(&routes, "glm-5.3[1m]").is_some());
        assert!(find_route(&routes, "glm-5.3-flash").is_none());
    }

    #[test]
    fn upstream_base_selects_per_protocol() {
        let r = GatewayRoute {
            anthropic_url: "https://a".into(),
            responses_url: "https://r".into(),
            chat_url: "https://c".into(),
            ..route("x", "", &[])
        };
        assert_eq!(Protocol::Anthropic.upstream_base(&r), "https://a");
        assert_eq!(Protocol::Responses.upstream_base(&r), "https://r");
        assert_eq!(Protocol::Chat.upstream_base(&r), "https://c");
    }

    #[test]
    fn upstream_path_strips_v1_for_openai_conventions() {
        // Chat/Responses 端点自带 /v1:客户端 /v1/chat/completions → 端点 + /chat/completions
        assert_eq!(
            join_upstream_url(
                Protocol::Chat,
                "https://api.example.com/v1",
                "/v1/chat/completions?x=1"
            ),
            "https://api.example.com/v1/chat/completions?x=1"
        );
        // 客户端路径不带 /v1(base_url 已含)同样正确
        assert_eq!(
            join_upstream_url(
                Protocol::Chat,
                "https://api.example.com/v1",
                "/chat/completions"
            ),
            "https://api.example.com/v1/chat/completions"
        );
    }

    #[test]
    fn join_upstream_url_dedups_endpoint_suffixes() {
        // 表单示例是完整端点:基目录拼接会双路径,防重剥掉端点尾部
        assert_eq!(
            join_upstream_url(
                Protocol::Responses,
                "https://api.openai.com/v1/responses",
                "/v1/responses"
            ),
            "https://api.openai.com/v1/responses"
        );
        assert_eq!(
            join_upstream_url(
                Protocol::Chat,
                "https://x.com/v1/chat/completions",
                "/v1/chat/completions"
            ),
            "https://x.com/v1/chat/completions"
        );
        // Anthropic 端点带 /v1 的自然写法:剥掉防 /v1/v1/messages
        assert_eq!(
            join_upstream_url(
                Protocol::Anthropic,
                "https://api.anthropic.com/v1",
                "/v1/messages"
            ),
            "https://api.anthropic.com/v1/messages"
        );
        // 常规基目录不受影响
        assert_eq!(
            join_upstream_url(
                Protocol::Anthropic,
                "https://open.bigmodel.cn/api/anthropic",
                "/v1/messages"
            ),
            "https://open.bigmodel.cn/api/anthropic/v1/messages"
        );
    }

    #[test]
    fn extract_model_parses_json_body() {
        let body = br#"{"model":"glm-5.3","messages":[]}"#;
        assert_eq!(extract_model(body).as_deref(), Some("glm-5.3"));
        assert_eq!(extract_model(br#"{"messages":[]}"#), None);
        assert_eq!(extract_model(b"not json"), None);
        assert_eq!(extract_model(br#"{"model":""}"#), None);
    }

    #[test]
    fn forward_headers_strips_auth_host_length() {
        let mut src = HeaderMap::new();
        src.insert("host", "127.0.0.1:8788".parse().unwrap());
        src.insert("authorization", "Bearer old".parse().unwrap());
        src.insert("x-api-key", "old".parse().unwrap());
        src.insert("content-type", "application/json".parse().unwrap());
        src.insert("anthropic-version", "2023-06-01".parse().unwrap());
        let out = forward_headers(&src, 42);
        assert!(out.get("host").is_none());
        assert!(out.get("authorization").is_none());
        assert!(out.get("x-api-key").is_none());
        assert_eq!(out.get("content-type").unwrap(), "application/json");
        assert_eq!(out.get("anthropic-version").unwrap(), "2023-06-01");
        assert_eq!(out.get("content-length").unwrap(), "42");
    }

    #[test]
    fn key_order_prefers_last_good() {
        let g = Gateway {
            routes: RwLock::new(Arc::new(Vec::new())),
            last_good: Mutex::new(HashMap::new()),
            cooldowns: Mutex::new(HashMap::new()),
            affinity: Mutex::new(HashMap::new()),
            task: Mutex::new(None),
            bind_error: RwLock::new(None),
            lifecycle: tokio::sync::Mutex::new(()),
            log_dir: RwLock::new(None),
            log_lock: Mutex::new(()),
        };
        let r = route("zhipu", "", &[]);
        let order = g.key_order(&r, None);
        assert_eq!(order, vec![0]);

        // 多 Key:粘性置首
        let multi = GatewayRoute {
            keys: vec![
                GatewayKey {
                    label: "a".into(),
                    api_key: "1".into(),
                },
                GatewayKey {
                    label: "b".into(),
                    api_key: "2".into(),
                },
                GatewayKey {
                    label: "c".into(),
                    api_key: "3".into(),
                },
            ],
            ..route("zhipu2", "", &[])
        };
        g.mark_good("zhipu2", Some(2));
        assert_eq!(g.key_order(&multi, None), vec![2, 0, 1]);
        g.mark_good("zhipu2", None);
        assert_eq!(g.key_order(&multi, None), vec![0, 1, 2]);

        // lastGood 越界(删 Key 后)回退配置序
        g.mark_good("zhipu2", Some(9));
        assert_eq!(g.key_order(&multi, None), vec![0, 1, 2]);
    }

    #[test]
    fn key_order_affinity_and_cooldown() {
        let g = Gateway {
            routes: RwLock::new(Arc::new(Vec::new())),
            last_good: Mutex::new(HashMap::new()),
            cooldowns: Mutex::new(HashMap::new()),
            affinity: Mutex::new(HashMap::new()),
            task: Mutex::new(None),
            bind_error: RwLock::new(None),
            lifecycle: tokio::sync::Mutex::new(()),
            log_dir: RwLock::new(None),
            log_lock: Mutex::new(()),
        };
        let multi = GatewayRoute {
            keys: vec![
                GatewayKey {
                    label: "a".into(),
                    api_key: "1".into(),
                },
                GatewayKey {
                    label: "b".into(),
                    api_key: "2".into(),
                },
                GatewayKey {
                    label: "c".into(),
                    api_key: "3".into(),
                },
            ],
            ..route("zhipu", "", &[])
        };
        // 会话亲和优先于 lastGood:会话绑 Key 1(置首),其余按配置序
        g.bind_session(42, "zhipu", 1);
        g.mark_good("zhipu", Some(2));
        assert_eq!(g.key_order(&multi, Some(42)), vec![1, 0, 2]);
        // 非本提供商的亲和不影响
        assert_eq!(g.key_order(&multi, Some(7)), vec![2, 0, 1]);
        // 冷却的 Key 排队尾(粘性 Key 1 冷却 → 让位给下一位)
        g.mark_cool("zhipu", 1, std::time::Instant::now() + KEY_COOLDOWN);
        let order = g.key_order(&multi, Some(42));
        assert_eq!(order.last(), Some(&1));
        // 全部冷却退化为粘性序(仍可服务)
        for i in 0..3 {
            g.mark_cool("zhipu", i, std::time::Instant::now() + KEY_COOLDOWN);
        }
        assert_eq!(g.key_order(&multi, None).len(), 3);
    }

    #[test]
    fn session_hash_is_stable_per_first_message() {
        let base = r#"{"model":"glm-5.3","messages":[{"role":"user","content":"task A"},REST]}"#;
        // 同首条 + 不同后续(append-only 会话的下一轮) → 同指纹
        let a = base.replace("REST", r#"{"role":"assistant","content":"ok"}"#);
        let b = base.replace("REST", r#"{"role":"user","content":"more"}"#);
        assert_eq!(session_hash(a.as_bytes()), session_hash(b.as_bytes()),);
        // 不同首条 → 不同指纹;空 messages → None
        let c = base.replace("task A", "task B");
        assert_ne!(session_hash(a.as_bytes()), session_hash(c.as_bytes()));
        assert_eq!(session_hash(br#"{"model":"m","messages":[]}"#), None);
    }

    #[test]
    fn rotatable_covers_auth_quota_timeout_and_5xx() {
        for s in [401u16, 402, 403, 404, 408, 429, 500, 502, 503, 504, 529] {
            assert!(rotatable(s), "{s} 应可轮换");
        }
        // 请求级错误不轮换(400 参数错误)
        assert!(!rotatable(400));
    }

    #[test]
    fn normalize_body_translates_budget_to_glm_effort() {
        // GLM 模型 + thinking budget → output_config.effort(智谱不认 budget)
        let body = br#"{"model":"glm-5.3","max_tokens":32000,"thinking":{"type":"enabled","budget_tokens":16000},"messages":[]}"#;
        let out = normalize_body(Protocol::Anthropic, "glm-5.3", Bytes::from_static(body));
        let v: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["output_config"]["effort"], "high");
        // 小预算 → low
        let body = br#"{"model":"glm-5.3[1m]","max_tokens":32000,"thinking":{"type":"enabled","budget_tokens":2000},"messages":[]}"#;
        let out = normalize_body(Protocol::Anthropic, "glm-5.3[1m]", Bytes::from_static(body));
        let v: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["output_config"]["effort"], "low");
        assert_eq!(v["model"], "glm-5.3");
        // 非 GLM 模型不翻译
        let body = br#"{"model":"deepseek-v4","max_tokens":32000,"thinking":{"type":"enabled","budget_tokens":16000},"messages":[]}"#;
        let out = normalize_body(Protocol::Anthropic, "deepseek-v4", Bytes::from_static(body));
        let v: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v.get("output_config"), None);
    }

    #[test]
    fn persisted_state_roundtrip() {
        let dir = std::env::temp_dir().join(format!("voidnix-gw-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let state = PersistedState {
            enabled: true,
            routes: vec![route("zhipu", "https://x.cn/api/anthropic", &["glm-5.3"])],
        };
        persist_state(&dir, &state).unwrap();
        let back = read_state(&dir).unwrap();
        assert!(back.enabled);
        assert_eq!(back.routes.len(), 1);
        assert_eq!(back.routes[0].models, vec!["glm-5.3"]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
