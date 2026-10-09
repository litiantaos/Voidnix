//! AI 网关服务器:Anthropic Messages / OpenAI Responses / OpenAI Chat Completions 三协议直通反向代理。
//!
//! 路由 = 请求体 `model` 字段 → 提供商;每提供商多 Key 轮换(429/401/403/503/529 换下一把重发,
//! lastGood 粘性优先)。请求体整体缓冲以支持换 Key 重放,响应(SSE 流式)透传不落盘。
//! 本模块持有词汇(Protocol、路由表)与 Gateway 状态机、轮换策略、HTTP 骨架与 proxy
//! 主流程;请求体读改与错误语义翻译纯函数在 normalize,2xx 流守护与尾部哨兵在 guard,
//! 快照持久化与排障日志在 state。

use super::guard::{GuardCommit, GuardCtx, GuardStream};
use super::normalize::{
    consumer_from_key, extract_model, norm_model, normalize_body, quota_exhausted, session_hash,
    too_long_rewrite, QUOTA_COOLDOWN,
};
use super::state::{persist_state, read_state, PersistedState};
use axum::body::{Body, Bytes};
use axum::extract::{Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::{get, post};
use axum::Router;
use futures_util::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex, RwLock};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

/// release 8788(固定端口,存量 CC 配置零改动);
/// dev 8789(dev 与 release 常驻并存,不互抢端口)
pub const PORT: u16 = if cfg!(debug_assertions) { 8789 } else { 8788 };

/// 网关端口对:cc_settings 跨实例互斥/归属判定消费(单一源,勿另处字面量);
/// 与 TS `GATEWAY_PORT` 双端手动同步
pub const GATEWAY_PORTS: &[u16] = &[8788, 8789];

/// 端口所属构建名(互斥拒绝报错指引用)
pub fn port_side(port: u16) -> &'static str {
    if port == 8789 {
        "dev"
    } else {
        "release"
    }
}

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GatewayKey {
    pub label: String,
    pub api_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
    /// 已治理上游:端点本身是另一网关(级联末跳)。归一/错误翻译/配额判定的知识
    /// (真实上游模型、key 池)只在末跳,本跳让位降级为哑管道。serde default 兜
    /// 旧 gateway-state.json 快照(无该键反序列化失败会致冷启动空路由)。
    #[serde(default)]
    pub governed: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayStatus {
    pub running: bool,
    pub port: u16,
    pub route_count: usize,
    pub bind_error: Option<String>,
    /// 使用事件流(最新在前;内存态,重启清零,不落盘)
    pub usage: Vec<super::usage::UsageEvent>,
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
    pub(super) log_dir: RwLock<Option<PathBuf>>,
    /// 日志追加串行锁:proxy 多并发,防交错
    pub(super) log_lock: Mutex<()>,
    /// 使用事件流(内存,重启清零;见 usage 模块)
    pub(super) usage: super::usage::UsageTable,
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
        usage: super::usage::UsageTable::default(),
    })
});

/// 会话亲和保持时长:上游 prompt cache 冷却约 5 分钟(Anthropic/OpenAI 最短),
/// 24h 覆盖一个长工作会话的生命周期,过期条目在写入时惰性清理
const AFFINITY_KEEP: std::time::Duration = std::time::Duration::from_secs(24 * 3600);

/// 轮换失败 Key 的默认冷却
const KEY_COOLDOWN: std::time::Duration = std::time::Duration::from_secs(60);

/// 最后一 Key 原地重试的次数上限(瞬时 429/5xx 在网关内换气重打,CC 无感;
/// 超限回放让客户端自行决策)
const INPLACE_RETRIES: u32 = 2;

/// 原地重试的等待决策:仅瞬时压力类(429/5xx 且非配额耗尽)值得原地重打——配额型
/// 窗口内重试必然失败。等待 = 上游 Retry-After(超 8s 的长等待放弃重试,回放让
/// 客户端决策)或 1s 倍增;round 为已重试轮次(0 起),倍增后超 8s 同样放弃
fn inplace_retry_wait(
    status: u16,
    quota: bool,
    retry_after: Option<std::time::Duration>,
    round: u32,
) -> Option<std::time::Duration> {
    if quota || !(status == 429 || status >= 500) {
        return None;
    }
    let wait = match retry_after {
        Some(d) if d.as_secs() > 8 => return None,
        Some(d) => d * 2u32.pow(round),
        None => std::time::Duration::from_secs(1u64 << round),
    };
    (wait.as_secs() <= 8).then_some(wait)
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
        // 换表才清按 (provider, idx) 记录的运行时状态:Key 增删/重排后同一 idx 指向
        // 不同 Key,冷却/亲和/粘性继续沿用会错位对象。同表重推(改开关/别名等无关
        // 配置触发的 sync)不清——亲和清空 = 活跃会话丢 Key 粘性,上游 prompt cache
        // 按 Key 隔离,换 Key 即前缀 cache 作废全价重算,代价远超一次成功请求。
        // usage 刻意不在清表之列:统计是历史事实,无 idx 错位语义
        if **self.routes.read().unwrap() != routes {
            self.cooldowns.lock().unwrap().clear();
            self.affinity.lock().unwrap().clear();
            self.last_good.lock().unwrap().clear();
        }
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
            usage: self.usage.snapshot(),
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

    /// 供 guard 的 GuardCommit 回写(lastGood 粘性)
    pub(super) fn mark_good(&self, provider_id: &str, idx: Option<usize>) {
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

    /// 会话应答成功:绑定亲和并顺带清理过期条目(低频写入,全扫成本可忽略)。
    /// 供 guard 的 GuardCommit 回写
    pub(super) fn bind_session(&self, session: u64, provider_id: &str, idx: usize) {
        let mut aff = self.affinity.lock().unwrap();
        aff.retain(|_, (_, _, at)| at.elapsed() < AFFINITY_KEEP);
        aff.insert(
            session,
            (provider_id.to_string(), idx, std::time::Instant::now()),
        );
    }

    /// 测试专用:脱离全局单例构造(不 bind 端口、不落盘)
    #[cfg(test)]
    pub(super) fn bare() -> Gateway {
        Gateway {
            routes: RwLock::new(Arc::new(Vec::new())),
            last_good: Mutex::new(HashMap::new()),
            cooldowns: Mutex::new(HashMap::new()),
            affinity: Mutex::new(HashMap::new()),
            task: Mutex::new(None),
            bind_error: RwLock::new(None),
            lifecycle: tokio::sync::Mutex::new(()),
            log_dir: RwLock::new(None),
            log_lock: Mutex::new(()),
            usage: super::usage::UsageTable::default(),
        }
    }

    /// 测试专用:会话是否已有亲和绑定(guard 断言用)
    #[cfg(test)]
    pub(super) fn affinity_has(&self, session: u64) -> bool {
        self.affinity.lock().unwrap().contains_key(&session)
    }

    /// 测试专用:lastGood 粘性断言用
    #[cfg(test)]
    pub(super) fn last_good_of(&self, provider_id: &str) -> Option<usize> {
        self.last_good.lock().unwrap().get(provider_id).copied()
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

/// 三协议线协议族词汇(normalize 归一判定复用)
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Protocol {
    Anthropic,
    Responses,
    Chat,
}

impl Protocol {
    /// 线协议面名(使用事件的消费者归并键,前端据此区分 Claude Code 与其它工具)
    fn wire(self) -> &'static str {
        match self {
            Protocol::Anthropic => "anthropic",
            Protocol::Responses => "responses",
            Protocol::Chat => "chat",
        }
    }

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

pub(super) async fn proxy(g: Arc<Gateway>, protocol: Protocol, req: Request) -> Response {
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
        g.log_gateway(
            "route",
            &format!("{} {} 缺少 model 字段", parts.method, parts.uri.path()),
        );
        return error_response(
            StatusCode::BAD_REQUEST,
            "invalid_request_error",
            "请求缺少 model 字段,网关无法路由",
            protocol,
        );
    };

    let session = session_hash(&body);

    let routes = g.routes_snapshot();
    let Some(route) = find_route(&routes, &model) else {
        g.log_gateway(
            "route",
            &format!(
                "{} {} · 未知模型 {model} · 可用: {}",
                parts.method,
                parts.uri.path(),
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
        g.log_gateway(
            "route",
            &format!(
                "{} {} · {} 未声明 {:?} 协议端点",
                parts.method,
                parts.uri.path(),
                route.name,
                protocol
            ),
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

    // 请求体归一:剥 [Nm] 后缀 + Anthropic 面注入 thinking disabled + GLM effort 翻译
    // + 输出预算下限。路由命中后才执行——未知模型/缺端点的请求直接拒绝,不为其
    // 支付归一成本(GLM 面含全量建树)。已治理上游让位:转发体字节级原样([Nm] 后缀
    // 留体,find_route 双向 norm_model 不影响路由匹配,末跳自剥幂等)
    let body = if route.governed {
        body
    } else {
        normalize_body(protocol, &model, body)
    };

    let path_and_query = parts
        .uri
        .path_and_query()
        .map(|pq| pq.as_str().to_string())
        .unwrap_or_default();
    let url = join_upstream_url(protocol, base, &path_and_query);
    let headers = forward_headers(&parts.headers, body.len());

    let order = g.key_order(route, session);
    let mut last_error: Option<(u16, Bytes)> = None;
    // 游标式循环:最后一 Key 的瞬时失败原地重打(不前进),轮换语义不变
    let mut i = 0usize;
    let mut retry_round: u32 = 0;
    while i < order.len() {
        let idx = order[i];
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
                g.log_gateway(
                    "net",
                    &format!("{model} · {} · {}: {e}", route.name, key.label),
                );
                i += 1;
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
                        .map(std::time::Duration::from_secs);
                    let cached = read_capped(r, MAX_ERROR_BODY).await.ok();
                    if let Some(buf) = &cached {
                        last_error = Some((status, buf.clone()));
                    }
                    // 配额耗尽型(计费/配额/余额文案)长冷却:速率限制的 60s 对配额耗尽
                    // 是必然失败的空转,日/周窗口内重试无意义。已治理上游让位:末跳 429
                    // 带权威 Retry-After(月度配额精确到月初),文本分类对其错误体可能
                    // 误判,恒 false 退化为 max(Retry-After, 60s) 且保住原地重试
                    let quota = !route.governed && cached.as_deref().is_some_and(quota_exhausted);
                    let cooldown = retry_after
                        .map_or(if quota { QUOTA_COOLDOWN } else { KEY_COOLDOWN }, |d| {
                            d.max(if quota { QUOTA_COOLDOWN } else { KEY_COOLDOWN })
                        });
                    let snippet = cached
                        .as_ref()
                        .map(|b| {
                            String::from_utf8_lossy(b)
                                .chars()
                                .take(160)
                                .collect::<String>()
                        })
                        .unwrap_or_default();
                    g.log_gateway(
                        "upstream",
                        &format!(
                            "{model} · {} · {}: {status}{quota_tag} {snippet}",
                            route.name,
                            key.label,
                            quota_tag = if quota { " quota" } else { "" }
                        ),
                    );
                    // 无下家可换时的原地重试(冷却前:重试成功则 Key 无罪,不进冷却)
                    if i + 1 == order.len() && retry_round < INPLACE_RETRIES {
                        if let Some(wait) =
                            inplace_retry_wait(status, quota, retry_after, retry_round)
                        {
                            g.log_gateway(
                                "retry",
                                &format!(
                                    "{model} · {} · {}: {status} 原地重试(第 {} 轮,等 {}s)",
                                    route.name,
                                    key.label,
                                    retry_round + 1,
                                    wait.as_secs()
                                ),
                            );
                            tokio::time::sleep(wait).await;
                            retry_round += 1;
                            continue;
                        }
                    }
                    log::warn!(
                        "[ai-gateway] {} · {}: {status},换下一把 Key",
                        route.name,
                        key.label
                    );
                    g.mark_good(&route.provider_id, None);
                    g.mark_cool(
                        &route.provider_id,
                        idx,
                        std::time::Instant::now() + cooldown,
                    );
                    i += 1;
                    continue;
                }
                if status < 400 {
                    // 2xx:使用事件即时记(响应头到达 = 该模型已被使用;归一裸名合并
                    // [Nm] 后缀变体,见 usage 模块),粘性/亲和的提交移交流守护——
                    // 收到响应头 ≠ 应答成功,流中途断开(哨兵判异常/传输错误)不提交,
                    // 客户端重试走其它 Key
                    g.record_usage(
                        norm_model(&model),
                        &route.name,
                        protocol.wire(),
                        consumer_from_key(&parts.headers),
                    );
                    let commit = GuardCommit {
                        g: Arc::clone(&g),
                        provider_id: route.provider_id.clone(),
                        idx,
                        session,
                    };
                    let guard = GuardCtx {
                        commit,
                        sentinel: (protocol == Protocol::Anthropic)
                            .then(|| (model.clone(), route.name.clone())),
                    };
                    return stream_response(r, Some(guard)).await;
                }
                // 非轮换错误(如模型名 400)回传客户端:Key 本身没坏不轮换,但必须落
                // 日志——CC 报 unavailable 而网关日志为空的盲区即此处。错误体很小,
                // 即时提交粘性/亲和(4xx 结构化错误证明 Key 存活)
                g.mark_good(&route.provider_id, Some(idx));
                if let Some(s) = session {
                    g.bind_session(s, &route.provider_id, idx);
                }
                let content_type = r
                    .headers()
                    .get(axum::http::header::CONTENT_TYPE)
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("application/json")
                    .to_string();
                let buf = read_capped(r, MAX_ERROR_BODY).await.unwrap_or_default();
                g.log_gateway(
                    "errpass",
                    &format!("{model} · {}: {status} 透传(不轮换)", route.name),
                );
                // 上下文超长语义翻译:命中厂商超长文案(尤其中文)改写为协议标准错误,
                // CC 识别后自动 compact;原文案 CC 认不了,直接报错停会话。已治理上
                // 游让位:末跳已译为协议标准错误,本跳透传即正确(词表重复命中是噪声)
                if !route.governed {
                    if let Some((status, payload)) = too_long_rewrite(protocol, &buf) {
                        g.log_gateway(
                            "toolong",
                            &format!("{model} · {}: 已翻译超长错误", route.name),
                        );
                        return json_response(status, payload);
                    }
                }
                return Response::builder()
                    .status(StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY))
                    .header(axum::http::header::CONTENT_TYPE, content_type)
                    .body(Body::from(buf))
                    .unwrap_or_else(|_| empty_error());
            }
        }
    }

    if let Some((status, buf)) = last_error {
        // 回放最后一个上游错误(429 等),客户端能看到真实原因
        g.log_gateway("replay", &format!("{model} · {} 回放 {status}", route.name));
        return Response::builder()
            .status(StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY))
            .header(axum::http::header::CONTENT_TYPE, "application/json")
            .body(Body::from(buf))
            .unwrap_or_else(|_| empty_error());
    }
    g.log_gateway(
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
/// guard = Some 的 2xx 流经 GuardStream 守护(流完整才提交粘性/亲和,哨兵判异常注入
/// 错误事件);None(错误透传)裸 pipe
async fn stream_response(upstream: reqwest::Response, guard: Option<GuardCtx>) -> Response {
    let status = upstream.status();
    // 上游响应头原样透传(content-type 区分 JSON/SSE,request-id 等客户端依赖;
    // 此前遗漏透传致全部头丢失——SSE 客户端宽松解析无感,非流式 JSON 客户端解析失败)
    let mut builder = Response::builder().status(status);
    for (name, value) in upstream.headers() {
        if !is_hop_by_hop(name.as_str()) {
            builder = builder.header(name, value);
        }
    }
    let is_sse = upstream
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.contains("text/event-stream"));
    let stream = upstream.bytes_stream();
    let stream: Box<dyn Stream<Item = std::io::Result<Bytes>> + Send + Unpin> = match guard {
        Some(ctx) => {
            let meta = if is_sse { ctx.sentinel } else { None };
            Box::new(GuardStream::new(stream, ctx.commit, meta))
        }
        None => {
            Box::new(stream.map(|chunk| chunk.map_err(|e| std::io::Error::other(e.to_string()))))
        }
    };
    builder
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
        // append 而非 insert:多值头(如逐行发送的多条 anthropic-beta)在迭代中逐值
        // 产出,insert 会整体替换只剩最后一个值
        out.append(name.clone(), value.clone());
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

/// 路由匹配:模型名经 norm_model 双向归一后比对([Nm] 后缀容差,归一在 normalize)
fn find_route<'a>(routes: &'a [GatewayRoute], model: &str) -> Option<&'a GatewayRoute> {
    let target = norm_model(model);
    routes
        .iter()
        .find(|r| r.models.iter().any(|m| norm_model(m) == target))
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
    json_response(status, payload)
}

/// JSON 载荷 → Response 的统一构造(超长翻译等已产载荷的路径复用)
fn json_response(status: StatusCode, payload: serde_json::Value) -> Response {
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

// pub(super):state/usage 模块的测试复用本模块工厂与 mock 上游基建
#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use serde_json::Value;

    pub(in crate::extensions::ai_gateway) fn route(
        id: &str,
        anthropic: &str,
        models: &[&str],
    ) -> GatewayRoute {
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
            governed: false,
        }
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
    fn forward_headers_preserves_multi_value_headers() {
        // 客户端逐行发送多条 anthropic-beta(context-1m 等能力开关)必须全量到达上游,
        // 迭代逐值产出 + insert 整体替换会只剩最后一个值
        let mut src = HeaderMap::new();
        src.append("anthropic-beta", "context-1m-2025-08-07".parse().unwrap());
        src.append("anthropic-beta", "oauth-2025-04-20".parse().unwrap());
        let out = forward_headers(&src, 1);
        let betas: Vec<&str> = out
            .get_all("anthropic-beta")
            .iter()
            .map(|v| v.to_str().unwrap())
            .collect();
        assert_eq!(betas, vec!["context-1m-2025-08-07", "oauth-2025-04-20"]);
    }

    #[test]
    fn key_order_prefers_last_good() {
        let g = Gateway::bare();
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
        let g = Gateway::bare();
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
    fn inplace_retry_wait_decides_transient_pressure_only() {
        use std::time::Duration;
        // 瞬时 429/5xx:1s 起倍增
        assert_eq!(
            inplace_retry_wait(429, false, None, 0),
            Some(Duration::from_secs(1))
        );
        assert_eq!(
            inplace_retry_wait(503, false, None, 1),
            Some(Duration::from_secs(2))
        );
        assert_eq!(
            inplace_retry_wait(429, false, None, 2),
            Some(Duration::from_secs(4))
        );
        // Retry-After 优先;倍增后超 8s 放弃
        assert_eq!(
            inplace_retry_wait(429, false, Some(Duration::from_secs(5)), 0),
            Some(Duration::from_secs(5))
        );
        assert_eq!(
            inplace_retry_wait(429, false, Some(Duration::from_secs(5)), 1),
            None
        );
        // Retry-After 本身超 8s:长等待不值得占住,回放让客户端决策
        assert_eq!(
            inplace_retry_wait(429, false, Some(Duration::from_secs(30)), 0),
            None
        );
        // 配额耗尽型(窗口内必然失败)与鉴权类失败(换 Key 才有意义)不原地重试
        assert_eq!(inplace_retry_wait(429, true, None, 0), None);
        assert_eq!(inplace_retry_wait(401, false, None, 0), None);
        assert_eq!(inplace_retry_wait(403, false, None, 0), None);
    }

    #[test]
    fn rotatable_covers_auth_quota_timeout_and_5xx() {
        for s in [401u16, 402, 403, 404, 408, 429, 500, 502, 503, 504, 529] {
            assert!(rotatable(s), "{s} 应可轮换");
        }
        // 请求级错误不轮换(400 参数错误)
        assert!(!rotatable(400));
    }

    /// 同表重推(改开关/别名等无关 sync)保留冷却/亲和/粘性;换表才清(idx 错位防线)。
    /// enabled=false 不触端口 bind,测试可与其它用例并存
    #[tokio::test]
    async fn update_preserves_runtime_state_when_routes_unchanged() {
        let dir = std::env::temp_dir().join(format!("voidnix-gw-keep-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let g = Gateway::bare();
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
            ],
            ..route("zhipu", "", &[])
        };
        g.update(false, vec![multi.clone()], &dir).await;
        g.bind_session(42, "zhipu", 1);
        g.update(false, vec![multi.clone()], &dir).await;
        assert_eq!(
            g.key_order(&multi, Some(42)),
            vec![1, 0],
            "同表重推不清亲和"
        );
        // 换表(头部插一把 Key,idx 语义变化)即清:亲和失效回配置序;usage 是历史
        // 事实无 idx 语义,换表同样保留(与冷却/亲和的清形成对照)
        g.record_usage("glm-5.3", "zhipu", "anthropic", None);
        let mut changed = multi.clone();
        changed.keys.insert(
            0,
            GatewayKey {
                label: "x".into(),
                api_key: "9".into(),
            },
        );
        g.update(false, vec![changed], &dir).await;
        assert_eq!(g.key_order(&multi, Some(42)), vec![0, 1], "换表清亲和");
        assert_eq!(g.usage_snapshot().len(), 1, "换表保留使用统计");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ─── 已治理上游(governed):级联第一跳让位 ─────────────────────

    /// 临时上游 mock(127.0.0.1:0 临时端口):逐次记录到达的原始请求体,恒按脚本
    /// 应答。fallback 接任意路径(测试不必关心 join_upstream_url 的路径整形),
    /// tests 直调 proxy() 端到端,网关侧无需 socket
    pub(in crate::extensions::ai_gateway) async fn spawn_upstream(
        status: u16,
        body: &'static [u8],
        headers: Vec<(&'static str, String)>,
        seen: Arc<Mutex<Vec<Bytes>>>,
    ) -> String {
        let app = Router::new().fallback(post(move |req: Request| {
            let seen = Arc::clone(&seen);
            async move {
                let bytes = axum::body::to_bytes(req.into_body(), 1024 * 1024)
                    .await
                    .unwrap();
                seen.lock().unwrap().push(bytes);
                let mut resp = Response::builder().status(status);
                for (k, v) in &headers {
                    resp = resp.header(*k, v);
                }
                resp.body(Body::from(body)).unwrap()
            }
        }));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        format!("http://{addr}")
    }

    /// 裸 Gateway 直写路由表(proxy 的最小装配,不经 update 的持久化副作用)
    pub(in crate::extensions::ai_gateway) fn gateway_with(
        routes: Vec<GatewayRoute>,
    ) -> Arc<Gateway> {
        let g = Arc::new(Gateway::bare());
        *g.routes.write().unwrap() = Arc::new(routes);
        g
    }

    pub(in crate::extensions::ai_gateway) fn anthropic_post(body: &str) -> Request {
        Request::builder()
            .method("POST")
            .uri("/v1/messages")
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    }

    #[tokio::test]
    async fn governed_forwards_body_bytes_verbatim() {
        let raw = r#"{"model":"glm-5.3[1m]","max_tokens":64,"messages":[{"role":"user","content":"hi"}]}"#;
        // 已治理:转发体字节级原样([1m] 留体、无 thinking 注入)
        let seen = Arc::new(Mutex::new(Vec::new()));
        let base = spawn_upstream(200, b"{\"ok\":true}", Vec::new(), Arc::clone(&seen)).await;
        let g = gateway_with(vec![GatewayRoute {
            anthropic_url: base,
            governed: true,
            ..route("gov", "", &["glm-5.3"])
        }]);
        let resp = proxy(g, Protocol::Anthropic, anthropic_post(raw)).await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(&*seen.lock().unwrap()[0], raw.as_bytes());
        // 对照组(直连):同请求被归一——后缀剥除 + thinking disabled 注入,
        // 证明差异来自门控而非 mock 形态
        seen.lock().unwrap().clear();
        let base = spawn_upstream(200, b"{\"ok\":true}", Vec::new(), Arc::clone(&seen)).await;
        let g = gateway_with(vec![GatewayRoute {
            anthropic_url: base,
            ..route("plain", "", &["glm-5.3"])
        }]);
        let resp = proxy(g, Protocol::Anthropic, anthropic_post(raw)).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let got: Value = serde_json::from_slice(&seen.lock().unwrap()[0]).unwrap();
        assert_eq!(got["model"], "glm-5.3");
        assert_eq!(got["thinking"]["type"], "disabled");
    }

    #[tokio::test]
    async fn governed_treats_quota_wording_as_rate_limit() {
        let quota_body: &'static [u8] =
            r#"{"error":{"message":"已达到今日额度上限,今日额度已用完"}}"#.as_bytes();
        // 已治理:配额文案不触发 30min 长冷却,退化为常规 60s(末跳 429 的
        // Retry-After 才是权威);配额分支不再禁掉原地重试——单 Key 初打 + 2 轮
        // 全打完才回放(Retry-After: 0 → 等待 0s,测试零真实 sleep)
        let seen = Arc::new(Mutex::new(Vec::new()));
        let base = spawn_upstream(
            429,
            quota_body,
            vec![("retry-after", "0".to_string())],
            Arc::clone(&seen),
        )
        .await;
        let g = gateway_with(vec![GatewayRoute {
            anthropic_url: base,
            governed: true,
            ..route("gov", "", &["glm-5.3"])
        }]);
        let resp = proxy(
            Arc::clone(&g),
            Protocol::Anthropic,
            anthropic_post(r#"{"model":"glm-5.3","messages":[{"role":"user","content":"hi"}]}"#),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
        let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
            .await
            .unwrap();
        assert_eq!(&*body, quota_body, "回放末跳原始 429 体");
        assert_eq!(seen.lock().unwrap().len(), 3, "原地重试保住(初打+2 轮)");
        let cooled = g
            .cooldowns
            .lock()
            .unwrap()
            .get(&("gov".to_string(), 0))
            .copied()
            .expect("重试耗尽后落常规冷却");
        let remain = cooled
            .saturating_duration_since(std::time::Instant::now())
            .as_secs();
        assert!(
            remain >= 55 && remain <= 65,
            "60s 常规冷却,非 30min(remain={remain})"
        );
        // 对照组(直连):同文案判配额型——原地重试被禁(单次)+ 30min 长冷却
        let seen = Arc::new(Mutex::new(Vec::new()));
        let base = spawn_upstream(
            429,
            quota_body,
            vec![("retry-after", "0".to_string())],
            Arc::clone(&seen),
        )
        .await;
        let g = gateway_with(vec![GatewayRoute {
            anthropic_url: base,
            ..route("plain", "", &["glm-5.3"])
        }]);
        let resp = proxy(
            Arc::clone(&g),
            Protocol::Anthropic,
            anthropic_post(r#"{"model":"glm-5.3","messages":[{"role":"user","content":"hi"}]}"#),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(seen.lock().unwrap().len(), 1, "配额型不做原地重试");
        let cooled = g
            .cooldowns
            .lock()
            .unwrap()
            .get(&("plain".to_string(), 0))
            .copied()
            .expect("配额型落冷却");
        let remain = cooled
            .saturating_duration_since(std::time::Instant::now())
            .as_secs();
        assert!(remain >= 1700, "30min 配额长冷却(remain={remain})");
    }

    #[tokio::test]
    async fn governed_passes_too_long_error_verbatim() {
        let long_body: &'static [u8] =
            r#"{"error":{"code":"1210","message":"输入上下文长度超过模型上限,请缩减对话后重试"}}"#
                .as_bytes();
        // 已治理:末跳已译为协议标准错误,本跳原样透传(不重复翻译)
        let seen = Arc::new(Mutex::new(Vec::new()));
        let base = spawn_upstream(400, long_body, Vec::new(), Arc::clone(&seen)).await;
        let g = gateway_with(vec![GatewayRoute {
            anthropic_url: base,
            governed: true,
            ..route("gov", "", &["glm-5.3"])
        }]);
        let resp = proxy(
            g,
            Protocol::Anthropic,
            anthropic_post(r#"{"model":"glm-5.3","messages":[{"role":"user","content":"x"}]}"#),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
            .await
            .unwrap();
        assert_eq!(&*body, long_body, "超长文案原样透传");
        // 对照组(直连):同文案被翻译为协议标准错误
        let base = spawn_upstream(400, long_body, Vec::new(), seen).await;
        let g = gateway_with(vec![GatewayRoute {
            anthropic_url: base,
            ..route("plain", "", &["glm-5.3"])
        }]);
        let resp = proxy(
            g,
            Protocol::Anthropic,
            anthropic_post(r#"{"model":"glm-5.3","messages":[{"role":"user","content":"x"}]}"#),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
            .await
            .unwrap();
        let v: Value = serde_json::from_slice(&body).unwrap();
        assert!(v["error"]["message"]
            .as_str()
            .unwrap()
            .starts_with("prompt is too long:"));
    }

    /// 切换 governed = 行为变化,整表不等 → 清冷却/亲和/粘性(一次性亲和代价,
    /// 上游 prompt cache 按 Key 隔离即重建一次,语义正确)
    #[tokio::test]
    async fn toggling_governed_resets_runtime_state() {
        let dir = std::env::temp_dir().join(format!("voidnix-gw-toggle-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let g = Gateway::bare();
        let plain = route("z", "", &["m"]);
        let mut gov = plain.clone();
        gov.governed = true;
        g.update(false, vec![plain], &dir).await;
        g.bind_session(7, "z", 0);
        assert!(g.affinity_has(7));
        g.update(false, vec![gov], &dir).await;
        assert!(!g.affinity_has(7), "切换 governed 清运行时状态");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
