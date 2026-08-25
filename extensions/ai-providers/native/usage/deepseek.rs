//! DeepSeek 账户余额监控：`GET {origin}/user/balance`（Bearer API Key）。

use serde::Serialize;

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
    let mut headers = reqwest::header::HeaderMap::new();
    if let Ok(v) = reqwest::header::HeaderValue::from_str(&format!("Bearer {key}")) {
        headers.insert(reqwest::header::AUTHORIZATION, v);
    }
    headers.insert(
        reqwest::header::ACCEPT,
        reqwest::header::HeaderValue::from_static("application/json"),
    );

    let (status, text) = super::fetch_text(crate::http::client(), &url, headers).await?;
    let body: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("解析响应失败: {e}"))?;

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
                            .and_then(super::json_f64)
                            .map(|f| format!("{f}"))
                    })
                    .unwrap_or_else(|| "0".into()),
                granted_balance: item
                    .get("granted_balance")
                    .and_then(|c| c.as_str())
                    .map(str::to_string)
                    .or_else(|| {
                        item.get("granted_balance")
                            .and_then(super::json_f64)
                            .map(|f| format!("{f}"))
                    })
                    .unwrap_or_else(|| "0".into()),
                topped_up_balance: item
                    .get("topped_up_balance")
                    .and_then(|c| c.as_str())
                    .map(str::to_string)
                    .or_else(|| {
                        item.get("topped_up_balance")
                            .and_then(super::json_f64)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn balance_url_derives_origin() {
        assert_eq!(
            deepseek_balance_url("https://api.deepseek.com/v1"),
            "https://api.deepseek.com/user/balance"
        );
        assert_eq!(
            deepseek_balance_url("https://proxy.example.com:8443/openai/v1?q=1"),
            "https://proxy.example.com:8443/user/balance"
        );
        assert_eq!(
            deepseek_balance_url("  "),
            "https://api.deepseek.com/user/balance"
        );
    }
}
