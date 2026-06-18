//! 输出净化：把命令/工具输出中的 secret 模式替换为 `[REDACTED]`。
//!
//! 规则集参考 gitleaks（https://github.com/gitleaks/gitleaks），覆盖主流 API key、私钥、token。
//! Phase 2 可扩展熵阈值检测 + 自定义规则。

use std::borrow::Cow;
use std::sync::OnceLock;

use regex::Regex;

/// 全局编译的 secret 检测正则集合。
fn patterns() -> &'static Vec<Regex> {
    static PATTERNS: OnceLock<Vec<Regex>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        vec![
            // 私钥 PEM 块
            Regex::new(r"(?s)-----BEGIN [A-Z ]*?PRIVATE KEY-----.*?-----END [A-Z ]*?PRIVATE KEY-----").unwrap(),
            // OpenAI（旧格式 sk-...T3BlbkFJ...）
            Regex::new(r"sk-[A-Za-z0-9]{20}T3BlbkFJ[A-Za-z0-9]{16}").unwrap(),
            // OpenAI 新格式 sk-proj-...
            Regex::new(r"sk-proj-[A-Za-z0-9_\-]{40,}").unwrap(),
            // Anthropic
            Regex::new(r"sk-ant-api03-[A-Za-z0-9_\-]{60,}").unwrap(),
            // AWS Access Key
            Regex::new(r"(?:AKIA|ASIA|ABIA|ACCA|AGPA|AIDA|AROA|AIPA|ANPA|ANVA)[A-Z0-9]{16}").unwrap(),
            // AWS Secret Key（40 位 base64-ish，跟在 AWS Access 后）— 简化处理
            Regex::new(r"(?i)aws_secret_access_key\s*[=:]\s*[A-Za-z0-9/+=]{40}").unwrap(),
            // GitHub token
            Regex::new(r"gh[pousr]_[A-Za-z0-9]{36,}").unwrap(),
            // Slack token
            Regex::new(r"xox[baprs]-[A-Za-z0-9\-]{10,}").unwrap(),
            // Stripe（sk_live_...）
            Regex::new(r"sk_live_[A-Za-z0-9]{24,}").unwrap(),
            // Google API key（AIza...）
            Regex::new(r"AIza[0-9A-Za-z_\-]{35}").unwrap(),
            // JWT（eyJ... 三段）
            Regex::new(r"eyJ[A-Za-z0-9_\-]+\.eyJ[A-Za-z0-9_\-]+\.[A-Za-z0-9_\-]+").unwrap(),
            // Bearer token（headers 中的）
            Regex::new(r"(?i)bearer\s+[A-Za-z0-9_\-\.=]{20,}").unwrap(),
            // 通用 Authorization header
            Regex::new(r"(?i)(?:authorization|x-api-key)\s*[:=]\s*[A-Za-z0-9_\-\.=]{16,}").unwrap(),
        ]
    })
}

/// 净化输出：把匹配的 secret 段替换为 `[REDACTED:<kind>]`。
///
/// 送 LLM 前的最后一道兜底；命令执行结果、web 抓取内容、文件读取等都应过此函数。
pub fn scrub_secret(input: &str) -> Cow<'_, str> {
    let pats = patterns();
    let mut current: Cow<str> = Cow::Borrowed(input);
    let mut changed = false;
    for re in pats {
        if re.is_match(&current) {
            current = Cow::Owned(re.replace_all(&current, "[REDACTED]").into_owned());
            changed = true;
        }
    }
    let _ = changed; // 仅用于短期避免 warning
    current
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_text_passes_through() {
        let s = "hello world\nsome normal output";
        assert_eq!(scrub_secret(s), s);
    }

    #[test]
    fn redacts_openai_key() {
        // 严格按 OpenAI 格式：sk- + 20 字符 + T3BlbkFJ + 16 字符
        let s = "key: sk-abcd1234abcd1234abcdT3BlbkFJabcd1234abcd1234";
        let scrubbed = scrub_secret(s);
        assert!(!scrubbed.contains("sk-abcd"));
        assert!(scrubbed.contains("[REDACTED]"));
    }

    #[test]
    fn redacts_anthropic_key() {
        let s = "ANTHROPIC_API_KEY=sk-ant-api03-abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz1234567890ABCDEF";
        let scrubbed = scrub_secret(s);
        assert!(!scrubbed.contains("sk-ant-api03"));
    }

    #[test]
    fn redacts_github_token() {
        let s = "GITHUB_TOKEN=ghp_1234567890abcdefghijklmnopqrstuvwxyz";
        let scrubbed = scrub_secret(s);
        assert!(!scrubbed.contains("ghp_"));
    }

    #[test]
    fn redacts_jwt() {
        let s = "Authorization: eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
        let scrubbed = scrub_secret(s);
        assert!(!scrubbed.contains("eyJ"));
    }

    #[test]
    fn redacts_pem_private_key() {
        let s = "-----BEGIN RSA PRIVATE KEY-----\nMIIEpAIBAAKCAQEA...\n-----END RSA PRIVATE KEY-----";
        let scrubbed = scrub_secret(s);
        assert!(scrubbed.contains("[REDACTED]"));
        assert!(!scrubbed.contains("BEGIN RSA"));
    }

    #[test]
    fn redacts_bearer_token() {
        let s = "curl -H 'Bearer abc1234567890xyz789plus";
        let scrubbed = scrub_secret(s);
        assert!(scrubbed.contains("[REDACTED]"));
    }

    #[test]
    fn redacts_aws_access_key() {
        let s = "AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE";
        let scrubbed = scrub_secret(s);
        assert!(!scrubbed.contains("AKIA"));
    }

    #[test]
    fn multiple_secrets_in_one_string() {
        let s = "ghp_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa and sk-abcd1234abcd1234abcdT3BlbkFJabcd1234abcd1234";
        let scrubbed = scrub_secret(s);
        assert!(scrubbed.matches("[REDACTED]").count() >= 2);
    }
}
