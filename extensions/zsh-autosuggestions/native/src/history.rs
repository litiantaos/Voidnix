//! .zsh_history 解析。zsh extended_history 格式：`: <ts>:<dur>;<cmd>`。
//! 多行命令（for / heredoc）折叠为单行（`\n` → 空格）。

use std::collections::HashMap;
use std::path::Path;
use std::sync::LazyLock;

use crate::frecency::CommandStat;

static EXT_LINE_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^:\s*(\d+):\d+;(.*)$").unwrap());

pub fn parse(path: &Path) -> HashMap<String, CommandStat> {
    let mut stats: HashMap<String, CommandStat> = HashMap::new();
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(_) => return stats,
    };
    let text = String::from_utf8_lossy(&bytes);

    // Fallback ts：history 文件 mtime。extended_history 未开启时所有命令共享此值。
    let fallback_ts = std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let mut current_cmd = String::new();
    let mut current_ts: i64 = fallback_ts;
    let mut have_current = false;

    for raw in text.lines() {
        let line = raw.trim_end_matches(['\r']);
        if line.is_empty() {
            continue;
        }
        if let Some(caps) = EXT_LINE_RE.captures(line) {
            if have_current {
                ingest(&mut stats, &current_cmd, current_ts);
            }
            current_ts = caps
                .get(1)
                .and_then(|m| m.as_str().parse::<i64>().ok())
                .unwrap_or(fallback_ts);
            current_cmd = caps
                .get(2)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            have_current = true;
        } else if have_current {
            if !current_cmd.is_empty() {
                current_cmd.push(' ');
            }
            current_cmd.push_str(line);
        } else if !line.trim().is_empty() {
            // 非 extended_history 模式：整行视作独立命令。
            // 限制：多行命令（for/heredoc）续行无法识别，会被当作独立命令 ingest。
            ingest(&mut stats, line.trim(), fallback_ts);
        }
    }
    if have_current {
        ingest(&mut stats, &current_cmd, current_ts);
    }

    stats
}

fn ingest(stats: &mut HashMap<String, CommandStat>, raw_cmd: &str, ts: i64) {
    let cmd = raw_cmd.trim();
    if cmd.is_empty() || !is_safe(cmd) {
        return;
    }
    let s = stats.entry(cmd.to_string()).or_insert_with(|| CommandStat {
        command: cmd.to_string(),
        count: 0,
        last_used: 0,
        fail_count: 0,
        accept_count: 0,
        reject_count: 0,
    });
    s.count += 1;
    if ts > s.last_used {
        s.last_used = ts;
    }
}

/// 拒绝含控制字符的命令：所有 ASCII 控制字符（< 0x20，含 \t \n \r \0 ESC 等）
/// 与 DEL（0x7f）。这些字符会破坏 sourceable cache 行结构或干扰终端渲染。
pub fn is_safe(s: &str) -> bool {
    !s.chars().any(|c| c < ' ' || c == '\x7f')
}

/// 检测 history 是否为 extended_history 格式（`: <ts>:<dur>;<cmd>`）。
/// 任一行匹配即视为开启。未开启时所有命令共享文件 mtime，frecency 退化为
/// 纯频次排序，且多行命令续行会被当作独立命令污染语料库。
pub fn is_extended_history(path: &Path) -> bool {
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    let text = String::from_utf8_lossy(&bytes);
    text.lines().any(|line| EXT_LINE_RE.is_match(line))
}

#[cfg(test)]
fn make_history(content: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let id = COUNTER.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!("zsh-as-hist-test-{}-{id}.tmp", std::process::id()));
    std::fs::write(&path, content).unwrap();
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extended_single_line() {
        let p = make_history(": 1700000000:0;ls -la\n: 1700000001:5;git status\n");
        let stats = parse(&p);
        assert_eq!(stats.len(), 2);
        assert_eq!(stats.get("ls -la").unwrap().count, 1);
        assert_eq!(stats.get("ls -la").unwrap().last_used, 1700000000);
        assert_eq!(stats.get("git status").unwrap().last_used, 1700000001);
    }

    #[test]
    fn multiline_command_folded() {
        let p = make_history(": 1700000000:0;for i in 1 2 3\ndo echo $i\ndone\n");
        let stats = parse(&p);
        let cmd = stats.get("for i in 1 2 3 do echo $i done");
        assert!(cmd.is_some(), "multiline folded into single command");
        assert_eq!(cmd.unwrap().count, 1);
    }

    #[test]
    fn duplicate_commands_increment_count() {
        let p = make_history(": 1700000000:0;ls\n: 1700000005:0;ls\n: 1700000002:0;ls\n");
        let stats = parse(&p);
        let s = stats.get("ls").unwrap();
        assert_eq!(s.count, 3);
        assert_eq!(s.last_used, 1700000005);
    }

    #[test]
    fn non_extended_fallback_uses_mtime() {
        let p = make_history("ls\ngit status\n");
        let stats = parse(&p);
        assert_eq!(stats.len(), 2);
        assert!(stats.get("ls").unwrap().last_used > 0, "fallback ts from mtime");
        assert!(stats.get("git status").unwrap().last_used > 0);
    }

    #[test]
    fn empty_file() {
        let p = make_history("");
        let stats = parse(&p);
        assert!(stats.is_empty());
    }

    #[test]
    fn is_safe_rejects_control_chars() {
        assert!(!is_safe("ls\t-la"));
        assert!(!is_safe("echo\n"));
        assert!(!is_safe("echo\r"));
        assert!(!is_safe("echo\x00null"));
        assert!(!is_safe("echo\x1bescape"));
        assert!(!is_safe("echo\x7fdel"));
    }

    #[test]
    fn is_safe_accepts_normal() {
        assert!(is_safe("ls -la"));
        assert!(is_safe("git commit -m 'feat: 新功能'"));
        assert!(is_safe("echo '中文命令'"));
        assert!(is_safe("cd /tmp && make"));
    }

    #[test]
    fn unsafe_commands_excluded() {
        let p = make_history(": 1700000000:0;safe\n: 1700000001:0;bad\tcmd\n");
        let stats = parse(&p);
        assert_eq!(stats.len(), 1);
        assert!(stats.contains_key("safe"));
        assert!(!stats.contains_key("bad\tcmd"));
    }

    #[test]
    fn is_extended_history_detects_format() {
        let p = make_history(": 1700000000:0;ls\n: 1700000001:5;git status\n");
        assert!(is_extended_history(&p));
    }

    #[test]
    fn is_extended_history_rejects_plain() {
        let p = make_history("ls\ngit status\n");
        assert!(!is_extended_history(&p));
    }

    #[test]
    fn is_extended_history_missing_file() {
        assert!(!is_extended_history(
            std::path::Path::new("/nonexistent/zsh-as-ext-hist-test")
        ));
    }
}
