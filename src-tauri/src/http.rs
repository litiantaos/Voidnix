use reqwest::Client;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::LazyLock;
use std::time::Duration;

/// 构建 client：`request_timeout = None` 表示不设整体超时（流式大文件下载用，仅建连超时兜底）。
/// 复用 SSRF 重定向逐跳校验：外部 URL `evil.com → 302 → 169.254.169.254` 的链式绕过在跟随前被拦截。
/// 简单 `Policy::limited(3)` 只限次数、不校验目标，已被证可绕过。
fn build_client(request_timeout: Option<Duration>) -> Client {
    let mut builder = Client::builder()
        // 建连超时（TCP+TLS），独立于整体 timeout：建连卡死时 30s 快速失败，不必等整体超时
        .connect_timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::custom(move |attempt| {
            match validate_url(attempt.url().as_str()) {
                Ok(()) => {
                    // 额外硬上限：避免无限循环，与原 limited(3) 行为对齐
                    if attempt.previous().len() >= 3 {
                        attempt.error("too many redirects")
                    } else {
                        attempt.follow()
                    }
                }
                Err(reason) => attempt.error(reason),
            }
        }))
        .pool_max_idle_per_host(10);
    if let Some(t) = request_timeout {
        builder = builder.timeout(t);
    }
    builder.build().expect("Failed to build HTTP client")
}

static HTTP_CLIENT: LazyLock<Client> =
    LazyLock::new(|| build_client(Some(Duration::from_secs(120))));

/// 流式大文件下载 client：无整体超时（慢网络下大文件总耗时不可控，整体超时会中途掐断流），
/// 仅建连 30s 超时。复用全局 client 的 SSRF 重定向防护。
static DOWNLOAD_CLIENT: LazyLock<Client> = LazyLock::new(|| build_client(None));

pub fn client() -> &'static Client {
    &HTTP_CLIENT
}

pub fn download_client() -> &'static Client {
    &DOWNLOAD_CLIENT
}

// ── URL 安全校验原语（agent/translate/http_get 共享，单一真相源）────────
/// 手动解析 URL 提取 scheme + host，不依赖 url crate。
/// 返回 (scheme, host)；host 已剥离端口与 userinfo；IPv6 literal 的方括号已剥离。
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
    // IPv6 literal：`[::1]:8080` / `[fe80::1]` → 取方括号内部分
    let host = if let Some(stripped) = host_port.strip_prefix('[') {
        let close = stripped.find(']')?;
        &stripped[..close]
    } else {
        host_port.split(':').next()?
    };
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
    "127.", "10.", "192.168.", "172.16.", "172.17.", "172.18.", "172.19.", "172.20.", "172.21.",
    "172.22.", "172.23.", "172.24.", "172.25.", "172.26.", "172.27.", "172.28.", "172.29.",
    "172.30.", "172.31.", "169.254.",
];

const BLOCKED_HOST_EXACT: &[&str] = &[
    "0.0.0.0",
    "metadata.google.internal",
    "metadata.tencentyun.com",
];

/// 将 host 字符串解析为 IpAddr，覆盖以下形式：
/// - 标准点分十进制 `127.0.0.1`、IPv6 `::1` / `fe80::1`
/// - 整数编码：十进制 `2130706433`、十六进制 `0x7f000001`、八进制 `017700000001`
///   （hyper/OS inet_aton 接受这些形式，必须同等拦截）
///
/// 域名（含 `localhost`）返回 None。
fn host_to_ipaddr(host: &str) -> Option<IpAddr> {
    // 标准 IpAddr 解析（覆盖点分十进制与所有 IPv6 形式）
    if let Ok(ip) = host.parse::<IpAddr>() {
        return Some(ip);
    }
    // 整数编码 IPv4：纯数字（含 0x / 0 前缀）转 u32 再转 Ipv4Addr
    let trimmed = host.trim_start_matches('+');
    let octets = trimmed.as_bytes();
    if octets.is_empty() {
        return None;
    }
    let (radix, digits) = if let Some(hex) = trimmed.strip_prefix("0x") {
        (16, hex)
    } else if octets[0] == b'0' && octets.len() > 1 {
        (8, &trimmed[1..])
    } else if octets.iter().all(|b| b.is_ascii_digit()) {
        (10, trimmed)
    } else {
        return None;
    };
    let n = u32::from_str_radix(digits, radix).ok()?;
    // 排除无意义零值（`0` 解析为 0.0.0.0 由后续段位判定）
    Some(IpAddr::V4(Ipv4Addr::from(n)))
}

/// 判断 host 是否为 localhost / loopback。
/// 容忍带方括号的 IPv6 literal（`[::1]`）——parse_scheme_host 已剥离，此处兜底。
pub fn is_localhost(host: &str) -> bool {
    let lower = host.to_lowercase();
    let h = strip_ipv6_brackets(&lower);
    if h == "localhost" {
        return true;
    }
    match host_to_ipaddr(h) {
        Some(IpAddr::V4(v4)) => v4.is_loopback() || v4.is_unspecified(),
        Some(IpAddr::V6(v6)) => v6.is_loopback() || v6.is_unspecified(),
        None => false,
    }
}

/// 判断 host 是否属于私网 / link-local / 元数据服务（不含 localhost）。
// 注意：字符串前缀匹配无法防御 DNS rebinding（域名解析到内网 IP）。
// 完整防御需在 DNS 解析后按 IpAddr 段校验，当前为纵深防御的一层。
pub fn is_private_or_reserved(host: &str) -> bool {
    let lower = host.to_lowercase();
    let h = strip_ipv6_brackets(&lower);
    if BLOCKED_HOST_EXACT.contains(&h) {
        return true;
    }
    // 能解析为 IpAddr 的按段位精确判定（覆盖 IPv6 / 整数编码 / 0.0.0.0 等）
    if let Some(ip) = host_to_ipaddr(h) {
        match ip {
            IpAddr::V4(v4) => {
                return v4.is_private()
                    || v4.is_link_local()
                    || v4.is_loopback()
                    || v4.is_unspecified()
                    || v4.is_multicast()
                    || v4.is_broadcast()
                    || v4.is_documentation();
            }
            IpAddr::V6(v6) => {
                // 文档地址段 2001:db8::/32（is_documentation() 不稳定，手动判定）
                let segs = v6.segments();
                let is_doc = segs[0] == 0x2001 && segs[1] == 0xdb8;
                // IPv4-mapped IPv6（`::ffff:a.b.c.d`）：段 [5]=0xffff，末两段为 IPv4。
                // 内嵌 IPv4 段位需独立判定（v6.is_loopback 不识别 ::ffff:127.0.0.1）。
                let is_v4mapped_loopback_or_private =
                    if segs[0..5].iter().all(|s| *s == 0) && segs[5] == 0xffff {
                        let v4 = Ipv4Addr::new(
                            (segs[6] >> 8) as u8,
                            segs[6] as u8,
                            (segs[7] >> 8) as u8,
                            segs[7] as u8,
                        );
                        v4.is_loopback()
                            || v4.is_private()
                            || v4.is_link_local()
                            || v4.is_unspecified()
                            || v4.is_broadcast()
                            || v4.is_documentation()
                    } else {
                        false
                    };
                return v6.is_loopback()
                    || v6.is_unspecified()
                    || v6.is_multicast()
                    || v6.is_unicast_link_local()
                    || v6.is_unique_local()
                    || is_doc
                    || is_v4mapped_loopback_or_private;
            }
        }
    }
    // 域名兜底：点分十进制前缀（历史遗留，host_to_ipaddr 已覆盖大部分）
    BLOCKED_HOST_PREFIXES.iter().any(|p| h.starts_with(p))
}

/// 剥离 IPv6 literal 的方括号（`[::1]` → `::1`），非括号形式原样返回。
fn strip_ipv6_brackets(host: &str) -> &str {
    if let Some(inner) = host.strip_prefix('[') {
        inner.strip_suffix(']').unwrap_or(inner)
    } else {
        host
    }
}

/// 判断 host 是否被完全阻断（localhost + 私网/保留地址）。
pub fn is_blocked_host(host: &str) -> bool {
    is_localhost(host) || is_private_or_reserved(host)
}

/// 校验 URL scheme + host 安全性（http_get 通用 SSRF 防护）。
/// 允许 http/https（通用查询接口可能为 http），拒绝私网/元数据 host。
pub fn validate_url(url: &str) -> Result<(), String> {
    let (scheme, host) =
        parse_scheme_host(url).ok_or_else(|| format!("Invalid URL: '{}'", url.trim()))?;
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

/// 校验 AI endpoint URL（agent + translate 共享，H3 单一真相源）。
///
/// 策略与 `validate_url` 的差异：开发场景允许 `http://localhost[:port]`（本地 LLM），
/// 远程必须 https。私网/保留地址、userinfo 一律拒。
pub fn validate_endpoint_url(url: &str) -> Result<(String, String), String> {
    let trimmed = url.trim();
    let (scheme, host) =
        parse_scheme_host(trimmed).ok_or_else(|| format!("Invalid endpoint URL: '{}'", trimmed))?;
    if scheme != "http" && scheme != "https" {
        return Err(format!("Unsupported scheme '{}' (only http/https)", scheme));
    }
    if url_has_userinfo(trimmed) {
        return Err("Endpoint URL must not contain credentials.".into());
    }
    let local = is_localhost(host);
    // 远程必须 https；localhost 开发允许 http
    if scheme != "https" && !local {
        return Err(
            "HTTP is not allowed for remote endpoints. Use HTTPS or localhost for development."
                .into(),
        );
    }
    // localhost 之外的私网/保留地址一律拒
    if !local && is_private_or_reserved(host) {
        return Err(format!(
            "Private/internal network endpoints are not allowed: '{}'.",
            host
        ));
    }
    Ok((scheme.to_string(), trimmed.to_string()))
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
        assert_eq!(
            parse_scheme_host("https://api.example.com/v1"),
            Some(("https", "api.example.com"))
        );
        assert_eq!(
            parse_scheme_host("http://localhost:8080"),
            Some(("http", "localhost"))
        );
        assert_eq!(parse_scheme_host("ftp://x.com"), Some(("ftp", "x.com")));
        assert_eq!(parse_scheme_host("not-a-url"), None);
    }

    #[test]
    fn parse_scheme_host_ipv6() {
        // [::1]:8080 → 剥离方括号
        assert_eq!(
            parse_scheme_host("http://[::1]:8080/"),
            Some(("http", "::1"))
        );
        assert_eq!(
            parse_scheme_host("http://[fe80::1]/"),
            Some(("http", "fe80::1"))
        );
        assert_eq!(
            parse_scheme_host("http://[fc00::1]/"),
            Some(("http", "fc00::1"))
        );
        assert_eq!(
            parse_scheme_host("http://[::ffff:127.0.0.1]/"),
            Some(("http", "::ffff:127.0.0.1"))
        );
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
    fn ipv6_loopback_and_private_blocked() {
        assert!(is_blocked_host("::1"));
        assert!(is_blocked_host("[::1]"));
        assert!(is_blocked_host("fe80::1"));
        assert!(is_blocked_host("fc00::1"));
        assert!(is_blocked_host("fd00::1"));
        assert!(is_blocked_host("::ffff:127.0.0.1"));
    }

    #[test]
    fn integer_encoded_ipv4_blocked() {
        // 十进制
        assert!(is_blocked_host("2130706433")); // 127.0.0.1
                                                // 十六进制
        assert!(is_blocked_host("0x7f000001"));
        // 八进制
        assert!(is_blocked_host("017700000001"));
        // 0 → 0.0.0.0
        assert!(is_blocked_host("0"));
    }

    #[test]
    fn legitimate_hosts_not_false_positive() {
        // 历史 `fc`/`fd` 前缀误杀已消除
        assert!(!is_blocked_host("fda.gov"));
        assert!(!is_blocked_host("fc.google.com"));
        assert!(!is_blocked_host("fdroid.org"));
        assert!(!is_blocked_host("api.example.com"));
    }

    #[test]
    fn validate_url_rejects_private() {
        assert!(validate_url("https://192.168.1.1").is_err());
        assert!(validate_url("https://169.254.169.254/latest").is_err());
        assert!(validate_url("ftp://example.com").is_err());
        assert!(validate_url("https://user:pass@example.com").is_err());
        // IPv6 私网 / loopback
        assert!(validate_url("http://[::1]/").is_err());
        assert!(validate_url("http://[fe80::1]/").is_err());
        assert!(validate_url("http://[fc00::1]/").is_err());
        // 整数编码
        assert!(validate_url("http://2130706433/").is_err());
        assert!(validate_url("http://0x7f000001/").is_err());
        // 合法
        assert!(validate_url("https://api.example.com/v1").is_ok());
        assert!(validate_url("http://api.example.com").is_ok());
        // 历史误杀修复
        assert!(validate_url("https://fda.gov").is_ok());
    }
}
