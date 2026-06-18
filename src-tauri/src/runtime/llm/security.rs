use crate::runtime::llm::types::LlmMessage;

/// SSE 缓冲上限（1 MiB），防止无界 buffer 增长
pub const MAX_SSE_BUFFER: usize = 1_048_576;
/// 单条消息内容上限（32 KiB）
pub const MAX_MESSAGE_CONTENT_LEN: usize = 32_768;
/// 历史消息条数硬上限
pub const MAX_CONVERSATION_MESSAGES: usize = 100;

// ── SSRF 防护 ──────────────────────────────────────────

/// 手动解析 URL 提取 scheme + host，不依赖 url crate
pub fn parse_scheme_host(raw: &str) -> Option<(&str, &str)> {
    let s = raw.trim();
    let scheme_end = s.find("://")?;
    let scheme = &s[..scheme_end];
    let rest = &s[scheme_end + 3..];
    let host_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let host_port = &rest[..host_end];
    let host = host_port.split(':').next()?;
    if scheme.is_empty() || host.is_empty() {
        return None;
    }
    Some((scheme, host))
}

const BLOCKED_HOST_PREFIXES: &[&str] = &[
    "127.", "10.", "192.168.",
    "172.16.", "172.17.", "172.18.", "172.19.", "172.20.", "172.21.", "172.22.",
    "172.23.", "172.24.", "172.25.", "172.26.", "172.27.", "172.28.", "172.29.",
    "172.30.", "172.31.", "169.254.", "0.",
];

const BLOCKED_HOST_EXACT: &[&str] = &[
    "0.0.0.0",
    "metadata.google.internal",
    "metadata.tencentyun.com",
];

/// 验证 endpoint 安全性，返回 (scheme, safe_endpoint)
pub fn validate_endpoint(endpoint: &str) -> Result<(String, String), String> {
    let trimmed = endpoint.trim();
    let (scheme, host) = parse_scheme_host(trimmed)
        .ok_or_else(|| format!("Invalid endpoint URL: '{}'", trimmed))?;

    let host_lower = host.to_lowercase();
    let is_localhost = host_lower == "localhost"
        || host_lower == "127.0.0.1"
        || host_lower == "::1"
        || host_lower == "[::1]";

    if scheme != "https" && !is_localhost {
        return Err(
            "HTTP is not allowed for remote endpoints. Use HTTPS or localhost for development."
                .into(),
        );
    }

    for exact in BLOCKED_HOST_EXACT {
        if host_lower == *exact {
            return Err(format!("Endpoint '{}' is blocked for security reasons.", host));
        }
    }

    if host_lower.starts_with("fc") || host_lower.starts_with("fd") || host_lower.starts_with("fe80") {
        return Err(format!(
            "Private/internal IPv6 network endpoints are not allowed: '{}'.",
            host
        ));
    }

    if host_lower.starts_with("::ffff:") || host_lower.starts_with("[::ffff:") {
        return Err(format!("IPv4-mapped IPv6 addresses are not allowed: '{}'.", host));
    }

    for prefix in BLOCKED_HOST_PREFIXES {
        if host_lower.starts_with(prefix) {
            return Err(format!(
                "Private/internal network endpoints are not allowed: '{}'.",
                host
            ));
        }
    }

    if host.contains('@') {
        return Err("Endpoint URL must not contain credentials.".into());
    }

    Ok((scheme.to_string(), trimmed.to_string()))
}

pub fn validate_ai_request(endpoint: &str, model: &str, api_key: &str) -> Result<String, String> {
    let (_scheme, safe_endpoint) = validate_endpoint(endpoint)?;
    if model.trim().is_empty() {
        return Err("模型名称不能为空".into());
    }
    if api_key.trim().is_empty() {
        return Err("API Key 不能为空".into());
    }
    Ok(safe_endpoint)
}

// ── 消息安全处理 ──────────────────────────────────────

pub fn truncate_message(content: &str) -> String {
    if content.len() <= MAX_MESSAGE_CONTENT_LEN {
        content.to_string()
    } else {
        let mut truncated: String = content.chars().take(MAX_MESSAGE_CONTENT_LEN).collect();
        truncated.push_str("\n\n[消息过长，已截断]");
        truncated
    }
}

pub fn trim_conversation(messages: &[LlmMessage]) -> Vec<LlmMessage> {
    if messages.len() <= MAX_CONVERSATION_MESSAGES {
        return messages.to_vec();
    }
    let system_msg = messages.iter().find(|m| m.role == "system").cloned();
    let skip = messages.len() - (MAX_CONVERSATION_MESSAGES - 1);
    let mut trimmed: Vec<LlmMessage> = messages.iter().skip(skip).cloned().collect();
    if let Some(sys) = system_msg {
        if trimmed.first().map(|m| m.role.as_str()) != Some("system") {
            trimmed.insert(0, sys);
        }
    }
    trimmed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_endpoint_rejects_private_network() {
        assert!(validate_endpoint("https://192.168.1.1/v1").is_err());
        assert!(validate_endpoint("https://10.0.0.1/v1").is_err());
        assert!(validate_endpoint("https://metadata.google.internal/v1").is_err());
    }

    #[test]
    fn validate_endpoint_accepts_https() {
        assert!(validate_endpoint("https://api.openai.com/v1").is_ok());
    }

    #[test]
    fn validate_endpoint_rejects_remote_http() {
        assert!(validate_endpoint("http://api.openai.com/v1").is_err());
    }

    #[test]
    fn validate_endpoint_accepts_localhost_http() {
        assert!(validate_endpoint("http://localhost:8080/v1").is_ok());
    }

    #[test]
    fn trim_conversation_preserves_system_msg() {
        let mut msgs = vec![LlmMessage::system("sys")];
        for i in 0..150 {
            msgs.push(LlmMessage::user(format!("u{}", i)));
        }
        let trimmed = trim_conversation(&msgs);
        assert!(trimmed.len() <= MAX_CONVERSATION_MESSAGES);
        assert_eq!(trimmed[0].role, "system");
    }
}
