use reqwest::Client;
use std::sync::LazyLock;
use std::time::Duration;

static HTTP_CLIENT: LazyLock<Client> = LazyLock::new(|| {
    Client::builder()
        .timeout(Duration::from_secs(120))
        // 限制重定向次数：默认跟随 10 次可被 `safe.com → 302 → 169.254.169.254` 链式绕过
        .redirect(reqwest::redirect::Policy::limited(3))
        .pool_max_idle_per_host(10)
        .build()
        .expect("Failed to build HTTP client")
});

pub fn client() -> &'static Client {
    &HTTP_CLIENT
}

// ── URL 安全校验原语（agent/translate/http_get 共享，单一真相源）────────
/// 手动解析 URL 提取 scheme + host，不依赖 url crate。
/// 返回 (scheme, host)；host 已剥离端口与 userinfo（`user:pass@host` → host）。
pub fn parse_scheme_host(raw: &str) -> Option<(&str, &str)> {
    let s = raw.trim();
    let scheme_end = s.find("://")?;
    let scheme = &s[..scheme_end];
    let rest = &s[scheme_end + 3..];
    // 剥离 userinfo：取最后一个 @ 之后的部分（host:port/path...）
    let after_userinfo = rest.rsplit_once('@').map(|(_, h)| h).unwrap_or(rest);
    let host_end = after_userinfo
        .find(['/', '?', '#'])
        .unwrap_or(after_userinfo.len());
    let host_port = &after_userinfo[..host_end];
    let host = host_port.split(':').next()?;
    if scheme.is_empty() || host.is_empty() {
        return None;
    }
    Some((scheme, host))
}

/// 检测 URL 是否包含 userinfo（`scheme://user[:pass]@host`）。
pub fn url_has_userinfo(raw: &str) -> bool {
    let s = raw.trim();
    let Some(rest) = s.split("://").nth(1) else {
        return false;
    };
    // authority 部分含 @ 即有 credential
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    rest[..authority_end].contains('@')
}

/// 私网 / link-local / 元数据服务的 host 前缀黑名单。
// 注意：字符串前缀匹配无法防御 DNS rebinding（域名解析到内网 IP）。
// 完整防御需在 DNS 解析后按 IpAddr 段校验，当前为纵深防御的一层。
const BLOCKED_HOST_PREFIXES: &[&str] = &[
    "127.", "10.", "192.168.",
    "172.16.", "172.17.", "172.18.", "172.19.", "172.20.", "172.21.", "172.22.",
    "172.23.", "172.24.", "172.25.", "172.26.", "172.27.", "172.28.", "172.29.",
    "172.30.", "172.31.", "169.254.",
];

const BLOCKED_HOST_EXACT: &[&str] = &[
    "0.0.0.0",
    "metadata.google.internal",
    "metadata.tencentyun.com",
];

/// 判断 host 是否为 localhost / loopback。
pub fn is_localhost(host: &str) -> bool {
    let h = host.to_lowercase();
    h == "localhost" || h == "127.0.0.1" || h == "::1" || h == "[::1]"
}

/// 判断 host 是否属于私网 / link-local / 元数据服务（不含 localhost）。
// 注意：字符串前缀匹配无法防御 DNS rebinding（域名解析到内网 IP）。
// 完整防御需在 DNS 解析后按 IpAddr 段校验，当前为纵深防御的一层。
pub fn is_private_or_reserved(host: &str) -> bool {
    let h = host.to_lowercase();
    if BLOCKED_HOST_EXACT.iter().any(|e| *e == h) {
        return true;
    }
    // IPv6 私网 / link-local 前缀（粗粒度，可被 `fc-proxy.example.com` 误伤）
    if h.starts_with("fc") || h.starts_with("fd") || h.starts_with("fe80") {
        return true;
    }
    if h.starts_with("::ffff:") || h.starts_with("[::ffff:") {
        return true;
    }
    BLOCKED_HOST_PREFIXES.iter().any(|p| h.starts_with(p))
}

/// 判断 host 是否被完全阻断（localhost + 私网/保留地址）。
pub fn is_blocked_host(host: &str) -> bool {
    is_localhost(host) || is_private_or_reserved(host)
}

/// 校验 URL scheme + host 安全性（http_get 通用 SSRF 防护）。
/// 允许 http/https（通用查询接口可能为 http），拒绝私网/元数据 host。
pub fn validate_url(url: &str) -> Result<(), String> {
    let (scheme, host) = parse_scheme_host(url)
        .ok_or_else(|| format!("Invalid URL: '{}'", url.trim()))?;
    if scheme != "http" && scheme != "https" {
        return Err(format!("Unsupported scheme '{}' (only http/https)", scheme));
    }
    if url_has_userinfo(url) {
        return Err("URL must not contain credentials.".into());
    }
    if is_blocked_host(host) {
        return Err(format!("Blocked host for security: '{}'", host));
    }
    Ok(())
}

/// 浏览器伪装 UA：绕过基于 User-Agent 的反爬（与 webview 请求行为对齐）。
const BROWSER_UA: &str =
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

/// 通用 HTTP GET：绕过 webview 的 UA/Referer 反爬与 CORS 限制。
/// 纯 TS 扩展（ip/currency）等无 native 的消费者使用；返回响应体文本，前端 JSON.parse。
#[tauri::command]
pub async fn http_get(url: String) -> Result<String, String> {
    validate_url(&url)?;
    HTTP_CLIENT
        .get(&url)
        .header(reqwest::header::USER_AGENT, BROWSER_UA)
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .text()
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_scheme_host_basic() {
        assert_eq!(parse_scheme_host("https://api.example.com/v1"), Some(("https", "api.example.com")));
        assert_eq!(parse_scheme_host("http://localhost:8080"), Some(("http", "localhost")));
        assert_eq!(parse_scheme_host("ftp://x.com"), Some(("ftp", "x.com")));
        assert_eq!(parse_scheme_host("not-a-url"), None);
    }

    #[test]
    fn blocked_hosts_detected() {
        assert!(is_blocked_host("192.168.1.1"));
        assert!(is_blocked_host("10.0.0.5"));
        assert!(is_blocked_host("169.254.169.254"));
        assert!(is_blocked_host("metadata.google.internal"));
        assert!(is_blocked_host("localhost"));
        assert!(!is_blocked_host("api.openai.com"));
    }

    #[test]
    fn validate_url_rejects_private() {
        assert!(validate_url("https://192.168.1.1").is_err());
        assert!(validate_url("https://169.254.169.254/latest").is_err());
        assert!(validate_url("ftp://example.com").is_err());
        assert!(validate_url("https://user:pass@example.com").is_err());
        assert!(validate_url("https://api.example.com/v1").is_ok());
        assert!(validate_url("http://api.example.com").is_ok());
    }
}
