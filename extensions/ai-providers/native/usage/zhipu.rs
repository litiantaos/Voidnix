//! 智谱 Coding Plan 用量监控：配额窗口（5h / 7d）+ 近 30 天用量曲线。
//! 对齐 tokens-monitor 协议；Authorization = 裸 API Key。

use serde::Serialize;

/// 配额端点与 chat 端点同 host（社区工具共识路由）
const ZHIPU_QUOTA_URL: &str = "https://open.bigmodel.cn/api/monitor/usage/quota/limit";
const ZHIPU_USAGE_URL: &str = "https://bigmodel.cn/api/monitor/usage/model-usage";
/// unit 枚举（对齐官方 web 端）：3=小时窗（5h）、6=周窗（7d）
const ZHIPU_UNIT_5H: i64 = 3;
const ZHIPU_UNIT_WEEKLY: i64 = 6;
const ZHIPU_REFERER: &str = "https://bigmodel.cn/coding-plan/personal/usage";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ZhipuQuotaSlice {
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
        total_calls: 0,
        total_tokens: 0,
        tokens_series: vec![],
        error,
    }
}

/// 配额端点（open.bigmodel.cn，Key 认证路由）：最小请求头。
fn zhipu_quota_headers(key: &str) -> reqwest::header::HeaderMap {
    let mut h = reqwest::header::HeaderMap::new();
    if let Ok(v) = reqwest::header::HeaderValue::from_str(key) {
        h.insert(reqwest::header::AUTHORIZATION, v);
    }
    h.insert(
        reqwest::header::ACCEPT,
        reqwest::header::HeaderValue::from_static("application/json"),
    );
    h
}

/// 用量端点（bigmodel.cn web 监控网关）：叠加浏览器请求面，与 tokens-monitor 一致，降低被网关拒的概率。
fn zhipu_usage_headers(key: &str) -> reqwest::header::HeaderMap {
    let mut h = zhipu_quota_headers(key);
    h.insert(
        reqwest::header::ACCEPT,
        reqwest::header::HeaderValue::from_static("application/json, text/plain, */*"),
    );
    h.insert(
        reqwest::header::REFERER,
        reqwest::header::HeaderValue::from_static(ZHIPU_REFERER),
    );
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
    let (status, text) =
        match super::fetch_text(crate::http::client(), &url, zhipu_usage_headers(key)).await {
            Ok(v) => v,
            Err(e) => {
                log::warn!("[ai-providers] model-usage request failed: {e}");
                return (0, 0, vec![]);
            }
        };
    let body: serde_json::Value = match serde_json::from_str(&text) {
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
        .map(super::json_i64)
        .unwrap_or(0);
    let tokens = total
        .and_then(|t| t.get("totalTokensUsage"))
        .map(super::json_i64)
        .unwrap_or(0);
    // tokensUsage：扁平 number[]（实测 bigmodel 近 30 天按日 30 点）
    let series_val = data
        .get("tokensUsage")
        .or_else(|| data.get("tokenUsage"))
        .or_else(|| total.and_then(|t| t.get("tokensUsage")));
    let series = match series_val {
        Some(serde_json::Value::Array(arr)) => {
            arr.iter().filter_map(super::json_f64).collect::<Vec<_>>()
        }
        // 偶发对象 map：按 key 排序后取 value
        Some(serde_json::Value::Object(map)) => {
            let mut pairs: Vec<_> = map.iter().collect();
            pairs.sort_by(|a, b| a.0.cmp(b.0));
            pairs
                .into_iter()
                .filter_map(|(_, v)| super::json_f64(v))
                .collect()
        }
        _ => vec![],
    };
    log::info!(
        "[ai-providers] model-usage ok calls={calls} tokens={tokens} series_len={}",
        series.len()
    );
    (calls, tokens, series)
}

/// 从响应体提取 (level, limits)：兼容 `{data:{limits,level}}` 信封与 V3 顶层数组两种 shape。
/// V3（2026-07-30 上线的积分制套餐）已观测到无信封裸数组 + `CREDIT_LIMIT` 类型。
fn zhipu_level_and_limits(
    body: &serde_json::Value,
) -> Result<(String, Vec<&serde_json::Value>), String> {
    if let Some(arr) = body.as_array() {
        return Ok(("unknown".into(), arr.iter().collect()));
    }
    let data = body.get("data").ok_or_else(|| "无 data".to_string())?;
    let level = data
        .get("level")
        .and_then(|l| l.as_str())
        .unwrap_or("unknown")
        .to_string();
    let limits = data
        .get("limits")
        .and_then(|l| l.as_array())
        .map(|a| a.iter().collect())
        .unwrap_or_default();
    Ok((level, limits))
}

/// 解析 limits 数组 → (5h 窗, 周窗)；非配额类型（如 `TIME_LIMIT`）跳过。
/// `TOKENS_LIMIT`（V2）与 `CREDIT_LIMIT`（V3 积分制）同构映射到 5h/7d 窗。
fn parse_zhipu_limit_entries(
    limits: &[&serde_json::Value],
) -> (Option<ZhipuQuotaSlice>, Option<ZhipuQuotaSlice>) {
    let mut five_hour = None;
    let mut weekly = None;
    for limit in limits {
        let ty = limit.get("type").and_then(|t| t.as_str()).unwrap_or("");
        let unit = limit.get("unit").and_then(|u| u.as_i64()).unwrap_or(0);
        if ty != "TOKENS_LIMIT" && ty != "CREDIT_LIMIT" {
            continue;
        }
        let slice = ZhipuQuotaSlice {
            percentage: limit
                .get("percentage")
                .and_then(|p| p.as_f64())
                .unwrap_or(0.0),
            next_reset_time: limit
                .get("nextResetTime")
                .and_then(|t| t.as_i64())
                .unwrap_or(0),
        };
        if unit == ZHIPU_UNIT_5H {
            five_hour = Some(slice);
        } else if unit == ZHIPU_UNIT_WEEKLY {
            weekly = Some(slice);
        }
    }
    (five_hour, weekly)
}

/// 拉取智谱 Coding Plan 配额 + 近 30 天用量（Authorization = API Key），两请求并发。
#[tauri::command]
pub async fn ai_providers_zhipu_quota(api_key: String) -> Result<ZhipuQuotaResult, String> {
    let key = api_key.trim();
    if key.is_empty() {
        return Err("API Key 为空".into());
    }

    // join! 同时驱动两个 future（future 惰性，不 join 即串行）
    let (quota, (total_calls, total_tokens, tokens_series)) = tokio::join!(
        super::fetch_text(
            crate::http::client(),
            ZHIPU_QUOTA_URL,
            zhipu_quota_headers(key)
        ),
        fetch_zhipu_usage(key)
    );
    let (status, text) = quota?;
    let body: serde_json::Value =
        serde_json::from_str(text.trim()).map_err(|e| format!("解析响应失败: {e}"))?;

    // HTTP 401 与响应 shape 无关，须先于 V3 裸数组分支（数组 body 无信封 code 字段可判）
    if status.as_u16() == 401 {
        return Ok(empty_quota("unknown", true, Some("API Key 无效".into())));
    }

    // V3 裸数组：无信封字段，直接就是 limits
    if body.is_array() {
        let (level, limits) = zhipu_level_and_limits(&body)?;
        let (five_hour, weekly) = parse_zhipu_limit_entries(&limits);
        return Ok(ZhipuQuotaResult {
            level,
            expired: false,
            five_hour,
            weekly,
            total_calls,
            total_tokens,
            tokens_series,
            error: None,
        });
    }

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

    if code == 401 || code == 1001 {
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

    let (level, limits) = zhipu_level_and_limits(&body)?;
    let (five_hour, weekly) = parse_zhipu_limit_entries(&limits);

    Ok(ZhipuQuotaResult {
        level,
        expired: false,
        five_hour,
        weekly,
        total_calls,
        total_tokens,
        tokens_series,
        error: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zhipu_quota_parses_envelope_with_tokens_limit() {
        let body = serde_json::json!({
            "code": 200, "msg": "操作成功", "success": true,
            "data": {
                "level": "max",
                "limits": [
                    {"type": "TOKENS_LIMIT", "unit": 3, "number": 5, "percentage": 61, "nextResetTime": 1787569633376i64},
                    {"type": "TOKENS_LIMIT", "unit": 6, "number": 1, "percentage": 24, "nextResetTime": 1787623243998i64},
                    {"type": "TIME_LIMIT", "unit": 5, "number": 1, "usage": 4000, "currentValue": 4,
                     "remaining": 3996, "percentage": 1, "nextResetTime": 1787709643997i64}
                ]
            }
        });
        let (level, limits) = zhipu_level_and_limits(&body).unwrap();
        assert_eq!(level, "max");
        assert_eq!(limits.len(), 3);
        let (five_hour, weekly) = parse_zhipu_limit_entries(&limits);
        let fh = five_hour.unwrap();
        assert_eq!(fh.percentage, 61.0);
        assert_eq!(fh.next_reset_time, 1787569633376i64);
        assert_eq!(weekly.unwrap().percentage, 24.0);
    }

    #[test]
    fn zhipu_quota_parses_v3_bare_array_with_credit_limit() {
        // V3（2026-07-30 积分制）已观测 shape：顶层数组、CREDIT_LIMIT、无 level 信封
        let body = serde_json::json!([
            {"type": "CREDIT_LIMIT", "unit": 3, "number": 5, "usage": 28000, "currentValue": 2585,
             "remaining": 25414, "percentage": 9, "nextResetTime": 1786592963348i64},
            {"type": "CREDIT_LIMIT", "unit": 6, "number": 1, "usage": 140000, "currentValue": 58386,
             "remaining": 81613, "percentage": 41, "nextResetTime": 1786692650981i64}
        ]);
        assert!(body.is_array());
        let (level, limits) = zhipu_level_and_limits(&body).unwrap();
        assert_eq!(level, "unknown");
        assert_eq!(limits.len(), 2);
        let (five_hour, weekly) = parse_zhipu_limit_entries(&limits);
        assert_eq!(five_hour.unwrap().percentage, 9.0);
        assert_eq!(weekly.unwrap().percentage, 41.0);
    }

    #[test]
    fn zhipu_quota_envelope_without_limits_yields_empty_slices() {
        // 团队档 type=2 对个人 Key 的实测响应：data 为空对象
        let body = serde_json::json!({"code": 200, "msg": "操作成功", "data": {}, "success": true});
        let (level, limits) = zhipu_level_and_limits(&body).unwrap();
        assert_eq!(level, "unknown");
        assert!(limits.is_empty());
        let (five_hour, weekly) = parse_zhipu_limit_entries(&limits);
        assert!(five_hour.is_none() && weekly.is_none());
    }

    #[test]
    fn zhipu_quota_missing_data_is_error() {
        let body = serde_json::json!({"code": 200, "success": true});
        assert!(zhipu_level_and_limits(&body).is_err());
    }
}
