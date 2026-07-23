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
            // 私钥 PEM 块（含 OpenSSH / PPK 头）
            Regex::new(r"(?s)-----BEGIN [A-Z ]*?PRIVATE KEY-----.*?-----END [A-Z ]*?PRIVATE KEY-----").unwrap(),
            Regex::new(r"(?s)-----BEGIN PGP PRIVATE KEY BLOCK-----.*?-----END PGP PRIVATE KEY BLOCK-----").unwrap(),
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
            // 通用 Authorization / X-API-Key header
            Regex::new(r"(?i)(?:authorization|x-api-key)\s*[:=]\s*[A-Za-z0-9_\-\.=]{16,}").unwrap(),
            // H14 扩充：GitLab PAT
            Regex::new(r"glpat-[A-Za-z0-9_\-]{20,}").unwrap(),
            // Twilio API Key / SID
            Regex::new(r"(?i)twilio[_-]?(?:api[_-]?key|auth[_-]?token)\s*[=:]\s*[A-Za-z0-9]{32}").unwrap(),
            // SendGrid
            Regex::new(r"SG\.[A-Za-z0-9_\-]{16,}\.[A-Za-z0-9_\-]{32,}").unwrap(),
            // Discord bot / webhook token
            Regex::new(r"(?:discord\.com/api/webhooks/|bot token)\s*[A-Za-z0-9_\-]{20,}").unwrap(),
            // Linear / Notion（integration token）
            Regex::new(r"(?:lin_api_|secret_)[A-Za-z0-9_\-]{20,}").unwrap(),
            // 通用 PASSWORD / SECRET / TOKEN / API_KEY 赋值（env / config 文件常见）
            // 注：raw string 内不宜用 \" 字符类（会被解析为 \ + 终止符）；只覆盖单引号/无引号
            Regex::new(r"(?i)(?:password|passwd|pwd|secret|token|api[_-]?key)\s*[=:]\s*'?[A-Za-z0-9/_+=!@#$%*{-}]{12,}'?").unwrap(),
            // 私有 registry token（npm/yarn _authToken）
            Regex::new(r"(?i)_authToken\s*=\s*[A-Za-z0-9_\-]{16,}").unwrap(),
        ]
    })
}

/// 净化输出：把匹配的 secret 段替换为 `[REDACTED:<kind>]`。
///
/// 送 LLM 前的最后一道兜底；命令执行结果、web 抓取内容、文件读取等都应过此函数。
pub fn scrub_secret(input: &str) -> Cow<'_, str> {
    let pats = patterns();
    let mut current: Cow<str> = Cow::Borrowed(input);
    for re in pats {
        if re.is_match(&current) {
            current = Cow::Owned(re.replace_all(&current, "[REDACTED]").into_owned());
        }
    }
    current
}

#[cfg(test)]
mod tests {
    use super::*;

    // 假凭据 fixture 用 concat! 在签名边界拆分拼接：源码中不出现连续可扫描字面量，
    // 规避 GitHub secret scanning 误报；编译期合并为同一 &'static str，零运行时开销。

    #[test]
    fn clean_text_passes_through() {
        let s = "hello world\nsome normal output";
        assert_eq!(scrub_secret(s), s);
    }

    #[test]
    fn redacts_openai_key() {
        // 严格按 OpenAI 格式：sk- + 20 字符 + T3BlbkFJ + 16 字符
        let s = concat!(
            "key: sk-",
            "abcd1234abcd1234abcd",
            "T3BlbkFJ",
            "abcd1234abcd1234"
        );
        let scrubbed = scrub_secret(s);
        assert!(!scrubbed.contains("sk-abcd"));
        assert!(scrubbed.contains("[REDACTED]"));
    }

    #[test]
    fn redacts_anthropic_key() {
        let s = concat!(
            "ANTHROPIC_API_KEY=sk-ant-api03-",
            "abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz1234567890ABCDEF"
        );
        let scrubbed = scrub_secret(s);
        assert!(!scrubbed.contains("sk-ant-api03"));
    }

    #[test]
    fn redacts_github_token() {
        let s = concat!("GITHUB_TOKEN=ghp_", "1234567890abcdefghijklmnopqrstuvwxyz");
        let scrubbed = scrub_secret(s);
        assert!(!scrubbed.contains("ghp_"));
    }

    #[test]
    fn redacts_jwt() {
        let s = concat!(
            "Authorization: eyJ",
            "hbGciOiJIUzI1NiJ9.",
            "eyJ",
            "zdWIiOiIxMjM0NTY3ODkwIn0.",
            "SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c"
        );
        let scrubbed = scrub_secret(s);
        assert!(!scrubbed.contains("eyJ"));
    }

    #[test]
    fn redacts_pem_private_key() {
        let s =
            "-----BEGIN RSA PRIVATE KEY-----\nMIIEpAIBAAKCAQEA...\n-----END RSA PRIVATE KEY-----";
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
        let s = concat!("AWS_ACCESS_KEY_ID=AKIA", "IOSFODNN7EXAMPLE");
        let scrubbed = scrub_secret(s);
        assert!(!scrubbed.contains("AKIA"));
    }

    #[test]
    fn multiple_secrets_in_one_string() {
        let s = concat!(
            "ghp_",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa and ",
            "sk-",
            "abcd1234abcd1234abcd",
            "T3BlbkFJ",
            "abcd1234abcd1234"
        );
        let scrubbed = scrub_secret(s);
        assert!(scrubbed.matches("[REDACTED]").count() >= 2);
    }

    #[test]
    fn redacts_gitlab_pat() {
        // H14：GitLab PAT
        let s = concat!("gitlab_token=glpat-", "abcdefghijklmnopqrst");
        let scrubbed = scrub_secret(s);
        assert!(!scrubbed.contains("glpat-"));
    }

    #[test]
    fn redacts_sendgrid() {
        // H14：SendGrid API Key（格式 SG.<id>.<secret>）
        let s = concat!(
            "SG.",
            "abcdefghijklmnop.",
            "apikeyabcdefghijklmnopqrst1234567890abcd"
        );
        let scrubbed = scrub_secret(s);
        assert!(!scrubbed.contains("SG.abcdefghijklmnop"));
    }

    #[test]
    fn redacts_generic_password_assignment() {
        // H14：通用 PASSWORD= 赋值
        let s = "DB_PASSWORD=supersecretvalue123";
        let scrubbed = scrub_secret(s);
        assert!(!scrubbed.contains("supersecretvalue123"));
    }

    #[test]
    fn redacts_npm_authtoken() {
        // H14：npm _authToken
        let s = concat!(
            "//registry.npmjs.org/:_authToken=npm_",
            "deadbeefcafef00d1234"
        );
        let scrubbed = scrub_secret(s);
        assert!(!scrubbed.contains("npm_deadbeef"));
    }

    #[test]
    fn normal_code_not_false_positive() {
        // H14：正常代码片段不应误伤
        let s = "const api_key = config.getKey();\nfunction password() { return hash; }";
        let scrubbed = scrub_secret(s);
        // 短串 + 函数调用形式不应触发（无 = : 后跟 12+ 字符的赋值）
        assert_eq!(scrubbed, s);
    }
}
