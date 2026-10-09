//! 请求体的读取与归一:网关侧对客户端请求体的全部纯函数读改(CC 怪癖在网关吸收)。
//! 响应侧错误语义判定/翻译(配额文案分类、上下文超长改写)与请求侧接入身份解析
//! (凭证约定 + UA 兜底)亦居此——吸收怪癖的纯函数族。
//!
//! 读侧:extract_model(路由键)/ session_hash(会话指纹)/ norm_model(模型名归一);
//! 改写侧:normalize_body——剥 [Nm] 后缀、注入 thinking disabled、输出预算下限、
//! GLM effort 翻译、auto mode 分类器识别与扩容。全部无状态,Protocol 词汇来自 server。

use super::guard::contains_bytes;
use super::server::Protocol;
use axum::body::Bytes;
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::Value;

/// 只取 model 字段的最小反序列化目标:零未知字段树分配(长上下文请求体可达数十 MB,
/// 全量建 serde_json::Value 树每请求付出 2-3 倍 body 的分配)
#[derive(Deserialize)]
struct ModelOnly {
    model: String,
}

/// 会话指纹:messages[0](Anthropic/Chat 面)或 input[0](Responses 面)的 hash
/// (RawValue 零拷贝)。会话是 append-only,首条消息全程不变,同一会话的各轮请求得到
/// 同一指纹;分类器等独立请求指纹唯一,绑定后无后续命中,无害
pub(super) fn session_hash(body: &[u8]) -> Option<u64> {
    #[derive(Deserialize)]
    struct Probe<'a> {
        #[serde(default, borrow)]
        messages: Vec<&'a serde_json::value::RawValue>,
        #[serde(default, borrow)]
        input: Vec<&'a serde_json::value::RawValue>,
    }
    let probe: Probe = serde_json::from_slice(body).ok()?;
    let first = probe.messages.first().or(probe.input.first())?;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    std::hash::Hasher::write(&mut h, first.get().as_bytes());
    Some(std::hash::Hasher::finish(&h))
}

/// 从请求体 JSON 提取 model 字段(三种协议请求同名字段)
pub(super) fn extract_model(body: &[u8]) -> Option<String> {
    let parsed: ModelOnly = serde_json::from_slice(body).ok()?;
    let trimmed = parsed.model.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// 模型名归一:剥 `[Nm]` 长上下文后缀(N = 1/2/…,CC 私有语法;CC 发送前已剥,中枢侧可能带后缀存储,双向归一比对)
pub(super) fn norm_model(m: &str) -> &str {
    let t = m.trim();
    let Some(body) = t.strip_suffix(']') else {
        return t;
    };
    let Some(cut) = body.rfind('[') else { return t };
    let Some(digits) = body[cut + 1..].strip_suffix('m') else {
        return t;
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return t;
    }
    t[..cut].trim()
}

// ─── 请求体归一 ───────────────────────────────────────────

/// 请求体归一(唯一的请求体改写,基础三件 + GLM/分类器两支内联注释):
/// 1. 剥 model 的 `[Nm]` 客户端后缀(全协议面)——CC 主对话发送前自剥 + 发 beta 头,
///    但其分类器等旁路请求原样带后缀,上游不认识该语法必报「模型不存在」;网关路由
///    匹配时已归一,转发时同样归一,客户端怪癖在网关侧吸收
/// 2. Anthropic 面注入 `thinking: {"type": "disabled"}`(请求未提 thinking 且非 claude 原生模型):
///    原生 Anthropic 语义即「无该字段 = 不思考」,但兼容端点默认思考(智谱/DeepSeek)——CC 分类器/
///    标题等旁路请求不带 thinking,输出预算被思考耗尽产出空 text。DeepSeek 认此参数彻底关思考
///    (实测 blocks 仅 text);智谱忽略它,由下述预算下限兜底(参考 magpie 的 thinkingOffUnlessAsked)
/// 3. Anthropic 面输出预算下限:对未显式声明 thinking 且 max_tokens 低于 256 的请求提升预算——
///    智谱忽略 disabled 依然思考且计入 max_tokens,小预算请求仍会被耗尽,下限保证 text 有出口
///    (模型答完即停,不产生额外消耗;flash 档分类任务实测思考约 150 token,256 留有余量)
///
/// 改写需全量建 serde_json::Value 树:GLM 主对话(effort 翻译命中)每请求付出 2-3 倍
/// body 的分配尖峰并占用 tokio worker([1m] 长上下文可达数十 MB)——已知可接受成本,
/// 相对网络传输是小头;未来优化方向 = 被改键(model/thinking/output_config/max_tokens)
/// 全在顶层,可做字节级 splice 免建树
const ANTHROPIC_MIN_OUTPUT_TOKENS: u64 = 256;

/// 输出预算探测(部分反序列化,未知字段流式跳过零建树;大请求体的线性扫描成本远低于网络传输)
#[derive(Deserialize)]
struct OutputPrefs<'a> {
    #[serde(default)]
    max_tokens: Option<u64>,
    /// 解析 thinking:存在性判定「是否注入 disabled」,budget 供 GLM effort 翻译
    #[serde(default)]
    thinking: Option<ThinkingSpec>,
    /// 仅探测非空性:分类器请求不带工具定义(与 magpie 判据一致),
    /// 据此排除「消息文本里恰好出现 <transcript>/<block> 字样」的主对话误判
    #[serde(default, borrow)]
    tools: Vec<&'a serde_json::value::RawValue>,
}

#[derive(Deserialize)]
struct ThinkingSpec {
    /// 显式声明的 thinking 类型:「disabled」供分类器形态判据消费,其余类型值不消费
    #[serde(rename = "type")]
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

/// CC auto mode 安全分类器请求识别,双判据取并集(单判据都有漂移面):
/// - **形态(主,跨 CC 版本稳定)**:无工具定义 + 显式 thinking disabled + 输出预算 ≤ 128
///   ——分类器请求天然三者俱全,不依赖 prompt 内容;CC 改版换标签时仍被捕获
/// - **标签(辅,magpie automode.go 判据)**:请求体含 `<transcript>` + `<block>`/`<severity>`
///   ——CC 只改形态字段(预算/thinking 写法)时仍被捕获
///
/// 分类器用会话主模型、主动带 thinking disabled、max_tokens 仅 64——无视 disabled 的
/// 模型(智谱)思考耗尽预算产出空答案,CC 读到无 verdict 即拦截动作。
/// tools 非空直接否决:主对话必带工具定义,防「消息文本里恰好出现特征 token」误判
fn is_auto_mode_classifier(protocol: Protocol, prefs: &OutputPrefs, body: &[u8]) -> bool {
    if protocol != Protocol::Anthropic || !prefs.tools.is_empty() {
        return false;
    }
    let shape = prefs
        .thinking
        .as_ref()
        .is_some_and(|t| t.kind == "disabled")
        && prefs.max_tokens.is_some_and(|m| m <= 128);
    shape || tag_match(body)
}

/// 标签判据的字面特征(transcript 块 + verdict 标签同时出现即足够特异)
fn tag_match(body: &[u8]) -> bool {
    contains_bytes(body, b"<transcript>")
        && (contains_bytes(body, b"<block>") || contains_bytes(body, b"<severity>"))
}

/// 分类器预算扩容:给无视 thinking disabled 的模型腾出思考余量 + 答案空间(magpie classifierRoom)
const CLASSIFIER_ROOM: u64 = 2048;

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

pub(super) fn normalize_body(protocol: Protocol, model: &str, body: Bytes) -> Bytes {
    let strip_suffix = model != norm_model(model);
    // claude 原生模型按原样发送(原生端点本就「不问不思考」,无需注入)
    let native_claude = model.to_lowercase().contains("claude-");
    let prefs: Option<OutputPrefs> = serde_json::from_slice(&body).ok();
    let no_thinking = prefs.as_ref().is_some_and(|p| p.thinking.is_none());
    let needs_disable = protocol == Protocol::Anthropic && !native_claude && no_thinking;
    // CC 分类器请求自带 thinking disabled(被智谱无视),预算提升不受「显式自管」豁免约束
    let classifier = prefs
        .as_ref()
        .is_some_and(|p| is_auto_mode_classifier(protocol, p, &body));
    let needs_boost = protocol == Protocol::Anthropic
        && (classifier
            || (no_thinking
                && prefs.as_ref().is_some_and(|p| {
                    p.max_tokens
                        .is_some_and(|m| m < ANTHROPIC_MIN_OUTPUT_TOKENS)
                })));
    // GLM:thinking 带预算时翻译 output_config.effort(智谱忽略 budget,不翻则强度形同虚设);
    // 分类器请求(GLM)无预算时压到最低档,少思考快出 verdict(magpie fitAutoModeClassifier)
    let glm = glm_effort_model(norm_model(model));
    let effort = match (&prefs, protocol) {
        (Some(p), Protocol::Anthropic) if glm => {
            if classifier {
                Some(
                    p.thinking
                        .as_ref()
                        .and_then(|t| t.budget_tokens)
                        .map_or("low", |b| match budget_to_effort(b) {
                            "high" => "high",
                            "medium" => "medium",
                            _ => "low",
                        }),
                )
            } else {
                p.thinking
                    .as_ref()
                    .and_then(|t| t.budget_tokens)
                    .map(budget_to_effort)
            }
        }
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
            let target = if classifier {
                m + CLASSIFIER_ROOM
            } else {
                ANTHROPIC_MIN_OUTPUT_TOKENS
            };
            if m < target {
                root["max_tokens"] = Value::from(target);
            }
        }
    }
    serde_json::to_vec(&root).map_or_else(|_| body, Bytes::from)
}

/// 接入身份解析(双判据,凭证优先):
/// ① 「占位 Key 即身份」——客户端凭证是任意占位值,按约定填 `voidnix-<工具名>`
///    (slug 限小写字母/数字/连字符,如 voidnix-claude-code)即可被识别,零登记零配置;
///    slug 直接转显示名(claude-code → Claude Code)
/// ② User-Agent 特征兜底——自带登录态的 CLI(如 grok)对自定义端点发自家 session
///    token 而非配置的 api_key,Key 约定对其失效;UA 携带稳定工具特征
///    (grok-pager / claude-cli / codex_cli)是唯一可达信号,小写子串匹配
/// 双判据都未命中返回 None,事件按协议面兜底
pub(super) fn consumer_from_key(headers: &HeaderMap) -> Option<String> {
    let key = headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .or_else(|| {
            headers
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("Bearer "))
        });
    if let Some(key) = key {
        if let Some(slug) = key.strip_prefix("voidnix-") {
            if !slug.is_empty()
                && slug
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            {
                return Some(
                    slug.split('-')
                        .map(|w| {
                            let mut c = w.chars();
                            match c.next() {
                                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                                None => String::new(),
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(" "),
                );
            }
        }
    }
    let ua = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())?
        .to_lowercase();
    // 特征表保守:仅收录 UA 稳定含工具名的已知 CLI,可按需扩充
    const UA_MARKERS: &[&str] = &["grok", "claude-cli", "codex"];
    UA_MARKERS
        .iter()
        .find(|m| ua.contains(*m))
        .map(|m| match *m {
            "claude-cli" => "Claude Code".to_string(),
            "codex" => "Codex".to_string(),
            _ => "Grok".to_string(),
        })
}

// ─── 错误语义判定/翻译(响应侧纯函数) ─────────────────────

/// 配额耗尽型错误文案子串(大小写不敏感):速率限制的 60s 冷却对配额耗尽是必然失败
/// 的空转——日/周配额窗口内重试无意义,长冷却避免每分钟白付一次往返。词表保守,
/// 仅高置信度配额语义(计费/配额/余额/周期限额);误伤速率限制的代价是多冷 29 分钟
pub(super) const QUOTA_MARKERS: &[&str] = &[
    "quota",
    "billing",
    "balance",
    "arrears",
    "usage limit",
    "daily limit",
    "monthly",
    "weekly",
    "额度",
    "配额",
    "欠费",
    "余额",
    "今日",
    // 智谱 1310 实测文案「您已达到每周/每月使用上限,您的限额将在 … 重置」——中文周/月
    // 上限词与英文 monthly/weekly 对齐,漏判会按速率限制 60s 冷却每分钟空转一次
    "每周",
    "每月",
];

/// 轮换失败(429/5xx 等)的响应体是否配额耗尽型
pub(super) fn quota_exhausted(body: &[u8]) -> bool {
    let text = String::from_utf8_lossy(body).to_lowercase();
    QUOTA_MARKERS.iter().any(|m| text.contains(m))
}

/// 配额耗尽冷却:日/周窗口尺度下 30min 重探一次(每次代价仅一个 429 往返),
/// 不做 resets_at 提取(厂商格式无标准,固定值鲁棒)
pub(super) const QUOTA_COOLDOWN: std::time::Duration = std::time::Duration::from_secs(30 * 60);

/// 上下文超长的厂商错误文案子串(大小写不敏感):命中即翻译为各协议标准错误,客户端
/// (CC)识别标准语义触发自动 compact——原文案(尤其中文厂商)客户端识别不了,直接
/// 报错停会话。词表保守:仅上下文语义,不含易与周期限额混淆的「token limit」族
/// (Anthropic 月度配额文案含该词,误译会触发无意义 compact)
pub(super) const TOO_LONG_MARKERS: &[&str] = &[
    "context_length_exceeded",
    "context length",
    "context window",
    "maximum context",
    "prompt is too long",
    "input is too long",
    "too many input tokens",
    "exceeds the maximum",
    "上下文",
];

/// 上下文超长错误改写:按客户端协议族生成标准错误载荷(Anthropic 面 "prompt is
/// too long" / OpenAI 面 code=context_length_exceeded),原文摘要保留在 message 供
/// 排障。纯函数只产 (status, JSON 载荷),Response 构造回归 server 的响应构造节
pub(super) fn too_long_rewrite(protocol: Protocol, body: &[u8]) -> Option<(StatusCode, Value)> {
    let text = String::from_utf8_lossy(body);
    let lower = text.to_lowercase();
    if !TOO_LONG_MARKERS.iter().any(|m| lower.contains(m)) {
        return None;
    }
    let snippet: String = text.chars().take(200).collect();
    let message = format!("prompt is too long: {snippet}");
    match protocol {
        Protocol::Anthropic => Some((
            StatusCode::BAD_REQUEST,
            serde_json::json!({
                "type": "error",
                "error": { "type": "invalid_request_error", "message": message },
            }),
        )),
        _ => Some((
            StatusCode::BAD_REQUEST,
            serde_json::json!({
                "error": { "message": message, "type": "invalid_request_error", "code": "context_length_exceeded" },
            }),
        )),
    }
}

// ─── 单元测试 ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn norm_model_strips_context_suffix() {
        assert_eq!(norm_model("glm-5.3[1m]"), "glm-5.3");
        assert_eq!(norm_model("glm-5.3[2m]"), "glm-5.3");
        assert_eq!(norm_model("glm-5.3"), "glm-5.3");
        assert_eq!(norm_model(" glm-5.3 [1m] "), "glm-5.3");
        // 非 `[数字m]` 形态不剥(空数字/非数字/后缀不在结尾)
        assert_eq!(norm_model("a[m]"), "a[m]");
        assert_eq!(norm_model("a[x1m]"), "a[x1m]");
        assert_eq!(norm_model("a[1m]b"), "a[1m]b");
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
    fn session_hash_covers_responses_input_array() {
        // Responses 面请求体是 input 数组(无 messages):同样取首条做指纹,
        // 此前返回 None 致该面会话完全无亲和
        let base = r#"{"model":"gpt-x","input":[{"role":"user","content":"task A"},REST]}"#;
        let a = base.replace("REST", r#"{"role":"assistant","content":"ok"}"#);
        let b = base.replace("REST", r#"{"role":"user","content":"more"}"#);
        assert_eq!(session_hash(a.as_bytes()), session_hash(b.as_bytes()));
        let c = base.replace("task A", "task B");
        assert_ne!(session_hash(a.as_bytes()), session_hash(c.as_bytes()));
        // messages 优先于 input(两字段同在时取 messages 首条)
        let m = r#"{"model":"m","messages":[{"content":"via messages"}],"input":[{"content":"via input"}]}"#;
        let only_input = r#"{"model":"m","input":[{"content":"via input"}]}"#;
        assert_ne!(
            session_hash(m.as_bytes()),
            session_hash(only_input.as_bytes())
        );
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
    fn normalize_body_gives_auto_mode_classifier_room() {
        // 形态判据(主):显式 disabled + 64 预算 + 无 tools,即使标签特征漂移(假想 CC
        // 新版 prompt 标签)也捕获——预算扩容 + GLM 压最低 effort
        let body = br#"{"model":"glm-5.3","max_tokens":64,"thinking":{"type":"disabled"},"system":"Answer <verdict>yes</verdict>","messages":[{"role":"user","content":"<dialog>tool call</dialog>"}]}"#;
        let out = normalize_body(Protocol::Anthropic, "glm-5.3", Bytes::from_static(body));
        let v: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["max_tokens"], 64 + CLASSIFIER_ROOM);
        assert_eq!(v["output_config"]["effort"], "low");
        // 标签判据(辅):形态字段漂移(如预算改大/thinking 写法变化)但特征标签仍在,
        // 且无 tools——仍按分类器处理
        let body = br#"{"model":"glm-5.3","max_tokens":64,"thinking":{"type":"adaptive"},"system":"Answer <block>yes</block>","messages":[{"role":"user","content":"<transcript>tool call</transcript>"}]}"#;
        let out = normalize_body(Protocol::Anthropic, "glm-5.3", Bytes::from_static(body));
        let v: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["max_tokens"], 64 + CLASSIFIER_ROOM);
        assert_eq!(v["output_config"]["effort"], "low");
        // 主对话消息里出现特征 token 但带 tools 定义:不误判,effort 不压、预算不扩容
        let body = br#"{"model":"glm-5.3","max_tokens":32000,"thinking":{"type":"adaptive"},"tools":[{"type":"text_editor"}],"system":"analyze <block> tags","messages":[{"role":"user","content":"<transcript> example"}]}"#;
        let out = normalize_body(Protocol::Anthropic, "glm-5.3", Bytes::from_static(body));
        let v: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["max_tokens"], 32000);
        assert_eq!(v.get("output_config"), None);
        // disabled + 无 tools 但预算充裕(512 > 128):形态不符且无标签,不动
        let body = br#"{"model":"glm-5.3","max_tokens":512,"thinking":{"type":"disabled"},"system":"normal","messages":[{"role":"user","content":"hi"}]}"#;
        let out = normalize_body(Protocol::Anthropic, "glm-5.3", Bytes::from_static(body));
        let v: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["max_tokens"], 512);
        assert_eq!(v.get("output_config"), None);
        // disabled + 64 预算但带 tools(带工具的显式关思考请求):不是分类器,不动
        let body = br#"{"model":"glm-5.3","max_tokens":64,"thinking":{"type":"disabled"},"tools":[{"type":"bash"}],"system":"normal","messages":[{"role":"user","content":"hi"}]}"#;
        let out = normalize_body(Protocol::Anthropic, "glm-5.3", Bytes::from_static(body));
        let v: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["max_tokens"], 64);
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
    fn quota_exhausted_matches_provider_wording() {
        // OpenAI 配额 / DeepSeek 余额 / 智谱今日额度 / Anthropic 周期限额
        assert!(quota_exhausted(
            br#"{"error":{"message":"You exceeded your current quota, please check your plan and billing details."}}"#
        ));
        // 智谱 1310 周期上限实测文案(网关日志捕获)
        assert!(quota_exhausted(
            r#"{"error":{"message":"[1310][您已达到每周/每月使用上限，您的限额将在 2026-10-07 10:56:46 重置。]"}}"#.as_bytes()
        ));
        assert!(quota_exhausted(
            br#"{"error":{"message":"Insufficient Balance"}}"#
        ));
        assert!(quota_exhausted(
            r#"{"error":{"message":"今日额度已用完,请明日再试"}}"#.as_bytes()
        ));
        assert!(quota_exhausted(
            b"This organization has hit its monthly token limit"
        ));
        // 速率限制/内部错误/空体不是配额型(60s 常规冷却)
        assert!(!quota_exhausted(
            b"Number of request tokens has exceeded your per-minute rate limit"
        ));
        assert!(!quota_exhausted(b"internal server error"));
        assert!(!quota_exhausted(b""));
    }

    #[test]
    fn too_long_rewrite_translates_provider_wording() {
        // OpenAI 英文超长 → Anthropic 面标准 "prompt is too long"(CC 识别触发 compact)
        let (status, v) = too_long_rewrite(
            Protocol::Anthropic,
            br#"{"error":{"message":"This model's maximum context length is 16385 tokens. However, your messages resulted in 20500 tokens"}}"#,
        )
        .expect("英文超长文案应翻译");
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(v["type"], "error");
        assert_eq!(v["error"]["type"], "invalid_request_error");
        assert!(v["error"]["message"]
            .as_str()
            .unwrap()
            .starts_with("prompt is too long:"));
        // 中文厂商文案同样命中
        assert!(too_long_rewrite(
            Protocol::Anthropic,
            r#"{"error":{"message":"输入上下文长度超过模型上限,请缩减对话"}}"#.as_bytes()
        )
        .is_some());
        // Chat 面:OpenAI 族 code 语义
        let (status, v) = too_long_rewrite(
            Protocol::Chat,
            b"Your request exceeds the maximum number of tokens allowed",
        )
        .unwrap();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(v["error"]["code"], "context_length_exceeded");
        // Anthropic 周期配额文案不误译(误译会触发无意义 compact 丢上下文)
        assert!(too_long_rewrite(
            Protocol::Anthropic,
            b"This organization has hit its monthly token limit"
        )
        .is_none());
        // 普通参数错误原样透传
        assert!(too_long_rewrite(Protocol::Anthropic, b"Invalid model name").is_none());
        assert!(too_long_rewrite(
            Protocol::Chat,
            br#"{"error":{"message":"Invalid max_tokens"}}"#
        )
        .is_none());
    }
}
