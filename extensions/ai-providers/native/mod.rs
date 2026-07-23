//! AI 提供商扩展：写 `~/.config/voidnix[/dev]/ai.env` + 读 env 快照（供 App 内回退）。
//! 无代理、无热路径。env 文件按构建分流到 voidnix/ 或 voidnix.dev/；shell 全局投影仅 release
//! 注入——外部工具固定变量名（`ZHIPU_*` 等）无法 dev/prod 并存，debug 只写文件供 App 内回退与手动 source。

use crate::runtime::registry::Extension;
use serde::Serialize;
use std::path::PathBuf;
use tauri::AppHandle;

/// dev 构建用 `.dev` 后缀目录与 scope，release 用基础值。
/// 与 `src-tauri/tauri.conf.json`（`com.litiantao.voidnix`）/ `tauri.dev.conf.json`（`.dev`）的 bundle id 隔离一致。
const DEV_SUFFIX: &str = if cfg!(debug_assertions) { ".dev" } else { "" };

/// 导出目录：release `~/.config/voidnix` / debug `~/.config/voidnix.dev`
fn export_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or_else(|| "无法解析 home 目录".to_string())?;
    Ok(home.join(".config").join(format!("voidnix{DEV_SUFFIX}")))
}

fn env_file_path() -> Result<PathBuf, String> {
    Ok(export_dir()?.join("ai.env"))
}

fn atomic_write(path: &std::path::Path, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {e}"))?;
    }
    let tmp = path.with_extension(format!(
        "{}.tmp",
        path.extension().and_then(|e| e.to_str()).unwrap_or("tmp")
    ));
    std::fs::write(&tmp, content).map_err(|e| format!("写入临时文件失败: {e}"))?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("替换目标文件失败: {e}")
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// shell rc scope（marker `# voidnix ai-providers`；见 runtime/shell_rc）。
/// 仅 release 注入 source 钩子；debug 用此 scope 摘除历史 dev 块自愈。
const SHELL_SCOPE: &str = if cfg!(debug_assertions) {
    "ai-providers-dev"
} else {
    "ai-providers"
};

/// source 钩子 body：指向 release 的 `ai.env`（仅 release 注入）。
fn shell_hook_body() -> String {
    r#"[ -f "$HOME/.config/voidnix/ai.env" ] && source "$HOME/.config/voidnix/ai.env""#.to_string()
}

/// 迁移：摘除旧版 `>>> voidnix-ai >>>` 成对 marker（走 atomic_write_rc 留 bak）。
fn migrate_legacy_pairs(rc_path: &std::path::Path) -> Result<(), String> {
    if !rc_path.exists() {
        return Ok(());
    }
    let existing = std::fs::read_to_string(rc_path)
        .map_err(|e| format!("读取 {} 失败: {e}", rc_path.display()))?;
    if existing.contains("# >>> voidnix-ai >>>") {
        let cleaned = crate::runtime::shell_rc::filter_legacy_pair_markers(&existing, "ai");
        if cleaned != existing {
            crate::runtime::shell_rc::atomic_write_rc(rc_path, &cleaned)?;
        }
    }
    Ok(())
}

/// 维护 shell rc 钩子（统一 shell_rc 约定）。
/// release：幂等写入 source 块；debug：摘除历史 dev 块自愈——外部工具固定变量名无法 dev/prod
/// 并存，shell 全局投影只保留 prod，debug 凭证仅写 `voidnix.dev/ai.env` 供 App 内回退与手动 source。
fn ensure_shell_hook(rc_path: &std::path::Path) -> Result<bool, String> {
    migrate_legacy_pairs(rc_path)?;
    if cfg!(debug_assertions) {
        crate::runtime::shell_rc::remove_block(rc_path, SHELL_SCOPE)
    } else {
        crate::runtime::shell_rc::upsert_block(rc_path, SHELL_SCOPE, &shell_hook_body())
    }
}

fn ensure_user_shell_hooks() {
    let Some(home) = dirs::home_dir() else {
        return;
    };
    // zsh 主用；login 也挂 zprofile 一份（部分终端只读它）
    for name in [".zshrc", ".zprofile"] {
        let path = home.join(name);
        match ensure_shell_hook(&path) {
            Ok(true) => log::info!("[ai-providers] installed shell hook → {}", path.display()),
            Ok(false) => {}
            Err(e) => log::warn!("[ai-providers] shell hook {}: {e}", path.display()),
        }
    }
}

/// 写入 `ai.env`，并幂等安装 shell source 钩子。返回 env 文件绝对路径。
#[tauri::command]
pub fn ai_providers_export(env_text: String) -> Result<String, String> {
    let path = env_file_path()?;
    atomic_write(&path, &env_text)?;
    ensure_user_shell_hooks();
    Ok(path.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn ai_providers_export_dir() -> Result<String, String> {
    Ok(export_dir()?.to_string_lossy().into_owned())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiEnvSnapshot {
    pub api_key: String,
    pub endpoint: String,
    pub model: String,
    /// process | file | empty
    pub source: String,
}

/// 从 shell 风格 export 行解析 KEY=VALUE（支持单引号/双引号/无引号）。
fn parse_export_line(line: &str) -> Option<(String, String)> {
    let s = line.trim();
    let s = s.strip_prefix("export ").unwrap_or(s).trim();
    if s.is_empty() || s.starts_with('#') {
        return None;
    }
    let (k, v) = s.split_once('=')?;
    let key = k.trim();
    if key.is_empty() {
        return None;
    }
    let mut val = v.trim().to_string();
    if (val.starts_with('\'') && val.ends_with('\''))
        || (val.starts_with('"') && val.ends_with('"'))
    {
        val = val[1..val.len().saturating_sub(1)].to_string();
        // 还原 shell 单引号转义 `'\\''` → `'`
        val = val.replace("'\\''", "'");
    }
    Some((key.to_string(), val))
}

fn read_env_file_map(path: &std::path::Path) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    let Ok(text) = std::fs::read_to_string(path) else {
        return map;
    };
    for line in text.lines() {
        if let Some((k, v)) = parse_export_line(line) {
            map.insert(k, v);
        }
    }
    map
}

fn pick(map: &std::collections::HashMap<String, String>, keys: &[&str]) -> String {
    for k in keys {
        if let Some(v) = map.get(*k) {
            let t = v.trim();
            if !t.is_empty() {
                return t.to_string();
            }
        }
    }
    String::new()
}

fn from_process() -> (String, String, String) {
    let api_key = std::env::var("OPENAI_API_KEY")
        .or_else(|_| std::env::var("VOIDNIX_AI_API_KEY"))
        .unwrap_or_default()
        .trim()
        .to_string();
    let endpoint = std::env::var("OPENAI_BASE_URL")
        .or_else(|_| std::env::var("VOIDNIX_AI_BASE_URL"))
        .unwrap_or_default()
        .trim()
        .to_string();
    let model = std::env::var("OPENAI_MODEL")
        .or_else(|_| std::env::var("VOIDNIX_AI_MODEL"))
        .unwrap_or_default()
        .trim()
        .to_string();
    (api_key, endpoint, model)
}

/// 读 OpenAI 兼容凭证：进程环境优先，否则 `~/.config/voidnix[/dev]/ai.env`。
/// Dock 启动时进程往往无 shell env，文件回退保证 App 内可用。
#[tauri::command]
pub fn ai_providers_env_snapshot() -> AiEnvSnapshot {
    let (mut api_key, mut endpoint, mut model) = from_process();
    let mut source = if !api_key.is_empty() || !endpoint.is_empty() {
        "process"
    } else {
        "empty"
    };

    if api_key.is_empty() || endpoint.is_empty() {
        if let Ok(path) = env_file_path() {
            let map = read_env_file_map(&path);
            if api_key.is_empty() {
                api_key = pick(&map, &["OPENAI_API_KEY", "VOIDNIX_AI_API_KEY"]);
            }
            if endpoint.is_empty() {
                endpoint = pick(&map, &["OPENAI_BASE_URL", "VOIDNIX_AI_BASE_URL"]);
            }
            if model.is_empty() {
                model = pick(&map, &["OPENAI_MODEL", "VOIDNIX_AI_MODEL"]);
            }
            if (!api_key.is_empty() || !endpoint.is_empty()) && source == "empty" {
                source = "file";
            }
        }
    }

    if api_key.is_empty() && endpoint.is_empty() {
        source = "empty";
    }

    AiEnvSnapshot {
        api_key,
        endpoint,
        model,
        source: source.into(),
    }
}

// ── 智谱 Coding Plan 监控（对齐 tokens-monitor：quota + 30d usage）──

const ZHIPU_QUOTA_URL: &str = "https://bigmodel.cn/api/monitor/usage/quota/limit";
const ZHIPU_USAGE_URL: &str = "https://bigmodel.cn/api/monitor/usage/model-usage";
const ZHIPU_UNIT_5H: i64 = 3;
const ZHIPU_UNIT_WEEKLY: i64 = 6;
const ZHIPU_UNIT_MCP: i64 = 5;
const ZHIPU_REFERER: &str = "https://bigmodel.cn/coding-plan/personal/usage";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ZhipuQuotaSlice {
    pub percentage: f64,
    pub next_reset_time: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ZhipuMcpSlice {
    pub usage: f64,
    pub total: f64,
    pub remaining: f64,
    pub percentage: f64,
    pub next_reset_time: i64,
}

/// 列表项展示：5h / 7d 配额 + 30d 曲线与总 calls/tokens。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ZhipuQuotaResult {
    pub level: String,
    pub expired: bool,
    pub five_hour: Option<ZhipuQuotaSlice>,
    /// 周配额（Coding Plan 周期，UI 标 7d）
    pub weekly: Option<ZhipuQuotaSlice>,
    pub mcp: Option<ZhipuMcpSlice>,
    /// 近 30 天总调用
    pub total_calls: i64,
    /// 近 30 天总 tokens
    pub total_tokens: i64,
    /// 近 30 天用量序列（供 sparkline；API 返回 tokensUsage）
    pub tokens_series: Vec<f64>,
    pub error: Option<String>,
}

fn empty_quota(level: &str, expired: bool, error: Option<String>) -> ZhipuQuotaResult {
    ZhipuQuotaResult {
        level: level.into(),
        expired,
        five_hour: None,
        weekly: None,
        mcp: None,
        total_calls: 0,
        total_tokens: 0,
        tokens_series: vec![],
        error,
    }
}

fn zhipu_headers(key: &str) -> reqwest::header::HeaderMap {
    let mut h = reqwest::header::HeaderMap::new();
    if let Ok(v) = reqwest::header::HeaderValue::from_str(key) {
        h.insert(reqwest::header::AUTHORIZATION, v);
    }
    h.insert(
        reqwest::header::ACCEPT,
        reqwest::header::HeaderValue::from_static("application/json, text/plain, */*"),
    );
    h.insert(
        reqwest::header::REFERER,
        reqwest::header::HeaderValue::from_static(ZHIPU_REFERER),
    );
    // 与浏览器 / tokens-monitor 请求面一致，降低被网关拒的概率
    h.insert(
        reqwest::header::USER_AGENT,
        reqwest::header::HeaderValue::from_static(
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
        ),
    );
    h.insert(
        reqwest::header::ORIGIN,
        reqwest::header::HeaderValue::from_static("https://bigmodel.cn"),
    );
    h
}

/// JSON 数字宽松转 i64（兼容整数 / 浮点 / 数字字符串）。
fn json_i64(v: &serde_json::Value) -> i64 {
    if let Some(i) = v.as_i64() {
        return i;
    }
    if let Some(u) = v.as_u64() {
        return u.min(i64::MAX as u64) as i64;
    }
    if let Some(f) = v.as_f64() {
        return f as i64;
    }
    if let Some(s) = v.as_str() {
        if let Ok(i) = s.parse::<i64>() {
            return i;
        }
        if let Ok(f) = s.parse::<f64>() {
            return f as i64;
        }
    }
    0
}

fn json_f64(v: &serde_json::Value) -> Option<f64> {
    if let Some(f) = v.as_f64() {
        return Some(f);
    }
    if let Some(i) = v.as_i64() {
        return Some(i as f64);
    }
    if let Some(u) = v.as_u64() {
        return Some(u as f64);
    }
    if let Some(s) = v.as_str() {
        return s.parse().ok();
    }
    None
}

/// 本地墙钟格式 `YYYY-MM-DD HH:MM:SS`（对齐 tokens-monitor）。
fn fmt_local(secs: i64) -> String {
    let t = secs as libc::time_t;
    let mut tm = unsafe { std::mem::zeroed::<libc::tm>() };
    let ptr = unsafe { libc::localtime_r(&t, &mut tm) };
    if ptr.is_null() {
        return "1970-01-01 00:00:00".into();
    }
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min,
        tm.tm_sec
    )
}

/// 本地日历：今天 00:00 往前 `days_ago` 天 00:00（与 tokens-monitor setHours(0)+setDate(-29) 一致）。
fn local_midnight_days_ago(days_ago: i32) -> i64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let t = now as libc::time_t;
    let mut tm = unsafe { std::mem::zeroed::<libc::tm>() };
    if unsafe { libc::localtime_r(&t, &mut tm) }.is_null() {
        return now - i64::from(days_ago) * 86400;
    }
    tm.tm_hour = 0;
    tm.tm_min = 0;
    tm.tm_sec = 0;
    tm.tm_mday -= days_ago;
    tm.tm_isdst = -1;
    let ts = unsafe { libc::mktime(&mut tm) };
    if ts < 0 {
        return now - i64::from(days_ago) * 86400;
    }
    ts as i64
}

fn usage_time_range() -> (String, String) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    // tokens-monitor：start = 29 天前本地 0 点，end = now
    (fmt_local(local_midnight_days_ago(29)), fmt_local(now))
}

/// 解析 model-usage 响应；失败时 log 原因（debug 构建可见）。
async fn fetch_zhipu_usage(key: &str) -> (i64, i64, Vec<f64>) {
    let (start, end) = usage_time_range();
    let url = format!(
        "{ZHIPU_USAGE_URL}?startTime={}&endTime={}",
        urlencoding::encode(&start),
        urlencoding::encode(&end)
    );
    let res = match crate::http::client()
        .get(&url)
        .headers(zhipu_headers(key))
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            log::warn!("[ai-providers] model-usage request failed: {e}");
            return (0, 0, vec![]);
        }
    };
    let status = res.status();
    let body: serde_json::Value = match res.json().await {
        Ok(v) => v,
        Err(e) => {
            log::warn!("[ai-providers] model-usage json failed ({status}): {e}");
            return (0, 0, vec![]);
        }
    };
    let success = body
        .get("success")
        .and_then(|s| s.as_bool())
        .unwrap_or(false);
    if !success {
        let msg = body.get("msg").and_then(|m| m.as_str()).unwrap_or("");
        log::warn!("[ai-providers] model-usage success=false status={status} msg={msg}");
        return (0, 0, vec![]);
    }
    let data = match body.get("data") {
        Some(d) if !d.is_null() => d,
        _ => {
            log::warn!("[ai-providers] model-usage no data");
            return (0, 0, vec![]);
        }
    };
    // totalUsage 可能缺省；数字可能是 float
    let total = data.get("totalUsage");
    let calls = total
        .and_then(|t| t.get("totalModelCallCount"))
        .map(json_i64)
        .unwrap_or(0);
    let tokens = total
        .and_then(|t| t.get("totalTokensUsage"))
        .map(json_i64)
        .unwrap_or(0);
    // tokensUsage：扁平 number[]（实测 bigmodel 近 30 天按日 30 点）
    let series_val = data
        .get("tokensUsage")
        .or_else(|| data.get("tokenUsage"))
        .or_else(|| total.and_then(|t| t.get("tokensUsage")));
    let series = match series_val {
        Some(serde_json::Value::Array(arr)) => arr.iter().filter_map(json_f64).collect::<Vec<_>>(),
        // 偶发对象 map：按 key 排序后取 value
        Some(serde_json::Value::Object(map)) => {
            let mut pairs: Vec<_> = map.iter().collect();
            pairs.sort_by(|a, b| a.0.cmp(b.0));
            pairs.into_iter().filter_map(|(_, v)| json_f64(v)).collect()
        }
        _ => vec![],
    };
    log::info!(
        "[ai-providers] model-usage ok calls={calls} tokens={tokens} series_len={}",
        series.len()
    );
    (calls, tokens, series)
}

/// 拉取智谱 Coding Plan 配额 + 近 30 天用量（Authorization = API Key）。
#[tauri::command]
pub async fn ai_providers_zhipu_quota(api_key: String) -> Result<ZhipuQuotaResult, String> {
    let key = api_key.trim();
    if key.is_empty() {
        return Err("API Key 为空".into());
    }

    let client = crate::http::client();
    let headers = zhipu_headers(key);

    let quota_fut = client.get(ZHIPU_QUOTA_URL).headers(headers.clone()).send();
    let usage_fut = fetch_zhipu_usage(key);

    let (quota_res, (total_calls, total_tokens, tokens_series)) =
        tokio::join!(quota_fut, usage_fut);

    let res = quota_res.map_err(|e| format!("请求失败: {e}"))?;
    let status = res.status();
    let body: serde_json::Value = res.json().await.map_err(|e| format!("解析响应失败: {e}"))?;

    let code = body.get("code").and_then(|c| c.as_i64()).unwrap_or(-1);
    let success = body
        .get("success")
        .and_then(|s| s.as_bool())
        .unwrap_or(false);
    let msg = body
        .get("msg")
        .and_then(|m| m.as_str())
        .unwrap_or("")
        .to_string();

    if code == 1001 || status.as_u16() == 401 {
        return Ok(empty_quota("unknown", true, Some("API Key 无效".into())));
    }
    if !success {
        return Ok(empty_quota(
            "unknown",
            false,
            Some(if msg.is_empty() {
                "请求失败".into()
            } else {
                msg
            }),
        ));
    }

    let data = body.get("data").ok_or_else(|| "无 data".to_string())?;
    let level = data
        .get("level")
        .and_then(|l| l.as_str())
        .unwrap_or("unknown")
        .to_string();
    let mut five_hour = None;
    let mut weekly = None;
    let mut mcp = None;

    if let Some(limits) = data.get("limits").and_then(|l| l.as_array()) {
        for limit in limits {
            let ty = limit.get("type").and_then(|t| t.as_str()).unwrap_or("");
            let unit = limit.get("unit").and_then(|u| u.as_i64()).unwrap_or(0);
            let percentage = limit
                .get("percentage")
                .and_then(|p| p.as_f64())
                .unwrap_or(0.0);
            let next_reset = limit
                .get("nextResetTime")
                .and_then(|t| t.as_i64())
                .unwrap_or(0);

            if ty == "TOKENS_LIMIT" {
                let slice = ZhipuQuotaSlice {
                    percentage,
                    next_reset_time: next_reset,
                };
                if unit == ZHIPU_UNIT_5H {
                    five_hour = Some(slice);
                } else if unit == ZHIPU_UNIT_WEEKLY {
                    weekly = Some(slice);
                }
            } else if ty == "TIME_LIMIT" && unit == ZHIPU_UNIT_MCP {
                mcp = Some(ZhipuMcpSlice {
                    usage: limit
                        .get("currentValue")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0),
                    total: limit.get("usage").and_then(|v| v.as_f64()).unwrap_or(0.0),
                    remaining: limit
                        .get("remaining")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0),
                    percentage,
                    next_reset_time: next_reset,
                });
            }
        }
    }

    Ok(ZhipuQuotaResult {
        level,
        expired: false,
        five_hour,
        weekly,
        mcp,
        total_calls,
        total_tokens,
        tokens_series,
        error: None,
    })
}

// ── DeepSeek 账户余额（GET /user/balance，Bearer API Key）──

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeepseekBalanceInfo {
    pub currency: String,
    pub total_balance: String,
    pub granted_balance: String,
    pub topped_up_balance: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeepseekBalanceResult {
    pub is_available: bool,
    pub balance_infos: Vec<DeepseekBalanceInfo>,
    pub error: Option<String>,
}

/// 从 OpenAI 兼容 endpoint 推出 balance URL：`{scheme}://{host[:port]}/user/balance`。
fn deepseek_balance_url(endpoint: &str) -> String {
    let ep = endpoint.trim().trim_end_matches('/');
    if ep.is_empty() {
        return "https://api.deepseek.com/user/balance".into();
    }
    // 去掉 path，只留 origin（无 url crate 依赖）
    let without_query = ep.split(['?', '#']).next().unwrap_or(ep);
    if let Some(scheme_sep) = without_query.find("://") {
        let rest = &without_query[scheme_sep + 3..];
        let hostport = rest.split('/').next().unwrap_or(rest);
        if !hostport.is_empty() {
            let scheme = &without_query[..scheme_sep];
            return format!("{scheme}://{hostport}/user/balance");
        }
    }
    "https://api.deepseek.com/user/balance".into()
}

/// 拉取 DeepSeek 账户余额。`endpoint` 用提供商配置的 API URL 推导 host。
#[tauri::command]
pub async fn ai_providers_deepseek_balance(
    api_key: String,
    endpoint: String,
) -> Result<DeepseekBalanceResult, String> {
    let key = api_key.trim();
    if key.is_empty() {
        return Err("API Key 为空".into());
    }
    let url = deepseek_balance_url(&endpoint);
    // 首跳 SSRF 门禁（与 http_get / LLM 路径一致；endpoint 用户可控）
    crate::http::validate_url(&url)?;
    let auth = format!("Bearer {key}");

    let res = crate::http::client()
        .get(&url)
        .header(reqwest::header::AUTHORIZATION, auth)
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|e| format!("请求失败: {e}"))?;

    let status = res.status();
    let body: serde_json::Value = res.json().await.map_err(|e| format!("解析响应失败: {e}"))?;

    if status.as_u16() == 401 || status.as_u16() == 403 {
        return Ok(DeepseekBalanceResult {
            is_available: false,
            balance_infos: vec![],
            error: Some("API Key 无效".into()),
        });
    }
    if !status.is_success() {
        let msg = body
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .or_else(|| body.get("message").and_then(|m| m.as_str()))
            .unwrap_or("请求失败");
        return Ok(DeepseekBalanceResult {
            is_available: false,
            balance_infos: vec![],
            error: Some(format!("{status}: {msg}")),
        });
    }

    let is_available = body
        .get("is_available")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let mut balance_infos = Vec::new();
    if let Some(arr) = body.get("balance_infos").and_then(|v| v.as_array()) {
        for item in arr {
            balance_infos.push(DeepseekBalanceInfo {
                currency: item
                    .get("currency")
                    .and_then(|c| c.as_str())
                    .unwrap_or("")
                    .to_string(),
                total_balance: item
                    .get("total_balance")
                    .and_then(|c| c.as_str())
                    .map(str::to_string)
                    .or_else(|| {
                        item.get("total_balance")
                            .and_then(json_f64)
                            .map(|f| format!("{f}"))
                    })
                    .unwrap_or_else(|| "0".into()),
                granted_balance: item
                    .get("granted_balance")
                    .and_then(|c| c.as_str())
                    .map(str::to_string)
                    .or_else(|| {
                        item.get("granted_balance")
                            .and_then(json_f64)
                            .map(|f| format!("{f}"))
                    })
                    .unwrap_or_else(|| "0".into()),
                topped_up_balance: item
                    .get("topped_up_balance")
                    .and_then(|c| c.as_str())
                    .map(str::to_string)
                    .or_else(|| {
                        item.get("topped_up_balance")
                            .and_then(json_f64)
                            .map(|f| format!("{f}"))
                    })
                    .unwrap_or_else(|| "0".into()),
            });
        }
    }

    Ok(DeepseekBalanceResult {
        is_available,
        balance_infos,
        error: None,
    })
}

pub struct AiProvidersExtension;

#[async_trait::async_trait]
impl Extension for AiProvidersExtension {
    fn id(&self) -> &'static str {
        "ai-providers"
    }

    async fn setup(&self, _app: &AppHandle) -> tauri::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_export_variants() {
        assert_eq!(
            parse_export_line("export OPENAI_API_KEY='sk-test'"),
            Some(("OPENAI_API_KEY".into(), "sk-test".into()))
        );
        assert_eq!(
            parse_export_line(r#"OPENAI_BASE_URL="https://x.com/v1""#),
            Some(("OPENAI_BASE_URL".into(), "https://x.com/v1".into()))
        );
        assert_eq!(
            parse_export_line("export FOO=bar"),
            Some(("FOO".into(), "bar".into()))
        );
        assert_eq!(parse_export_line("# comment"), None);
    }

    #[test]
    fn shell_hook_dev_never_injects_but_migrates_and_self_heals() {
        let dir = std::env::temp_dir().join(format!("voidnix-ai-hook-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let rc = dir.join(".zshrc");
        std::fs::write(
            &rc,
            "# existing\n\n# >>> voidnix-ai >>>\nold\n# <<< voidnix-ai <<<\n",
        )
        .unwrap();
        // debug 构建：迁移旧 pair marker，不注入 source 块（无 dev 块可摘 → false）
        assert!(!ensure_shell_hook(&rc).unwrap());
        let text = std::fs::read_to_string(&rc).unwrap();
        assert!(!text.contains("voidnix-ai"));
        assert!(!text.contains("voidnix ai-providers"));
        assert!(!text.contains("ai.env"));
        assert!(text.contains("# existing"));

        // 历史残留 dev 块自愈摘除
        std::fs::write(
            &rc,
            "# existing\n\n# voidnix ai-providers-dev\n[ -f \"$HOME/.config/voidnix.dev/ai.env\" ] && source \"$HOME/.config/voidnix.dev/ai.env\"\n",
        )
        .unwrap();
        assert!(ensure_shell_hook(&rc).unwrap());
        let text = std::fs::read_to_string(&rc).unwrap();
        assert!(!text.contains("voidnix ai-providers-dev"));
        assert!(!text.contains("ai.env"));
        assert!(text.contains("# existing"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
