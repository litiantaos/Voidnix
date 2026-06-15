//! signals.log 解析。zsh 端 precmd 钩子按需 append 一行（3 字段 TSV）：
//!   `<exit>\t<state>\t<cmd>`
//! 字段：
//!   exit    命令退出码（0=成功，非 0=失败）。失败统计独立于 suggestion。
//!   state   suggestion 互动状态：
//!             0 = 无 suggestion 互动（未显示，或不计）
//!             1 = accepted（用户通过 →/end 等接受 suggestion）
//!             2 = rejected（显示了 suggestion 但用户未接受，自行改写或忽略）
//!   cmd     sanitize 后的命令文本（不含 \t \n \r 等控制字符）
//!
//! zsh 端仅在 `exit != 0 || state != 0` 时 append（成功且无 suggestion 互动的命令
//! 无信息量，不记录），以控制文件体积。
//!
//! 前期开发：仅解析新 3 字段格式，格式错误行直接 skip，不做兼容。

use std::collections::HashMap;
use std::path::Path;

use crate::frecency::CommandStat;

pub fn apply(stats: &mut HashMap<String, CommandStat>, path: &Path) {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(_) => return,
    };
    let text = String::from_utf8_lossy(&bytes);

    for line in text.lines() {
        let Some((exit, state, cmd)) = parse_line(line) else {
            continue;
        };
        let cmd = cmd.trim();
        if cmd.is_empty() {
            continue;
        }
        let Some(s) = stats.get_mut(cmd) else {
            continue;
        };

        if exit != 0 {
            s.fail_count += 1;
        }
        match state {
            1 => s.accept_count += 1,
            2 => s.reject_count += 1,
            _ => {}
        }
    }
}

/// 解析单行 signals 记录，返回 `(exit, state, cmd)`。格式不合法返回 `None`。
/// `state`：0=无 suggestion 互动，1=accepted，2=rejected。
pub fn parse_line(line: &str) -> Option<(i32, u8, &str)> {
    let mut parts = line.splitn(3, '\t');
    let exit = parts.next()?.parse::<i32>().ok()?;
    let state_str = parts.next()?;
    let cmd = parts.next()?;
    let state = match state_str {
        "0" => 0u8,
        "1" => 1u8,
        "2" => 2u8,
        _ => return None,
    };
    Some((exit, state, cmd))
}

#[cfg(test)]
fn make_signals(content: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let id = COUNTER.fetch_add(1, Ordering::SeqCst);
    let path =
        std::env::temp_dir().join(format!("zsh-as-sig-test-{}-{id}.tmp", std::process::id()));
    std::fs::write(&path, content).unwrap();
    path
}

#[cfg(test)]
fn make_stat(cmd: &str) -> CommandStat {
    CommandStat {
        command: cmd.to_string(),
        count: 1,
        last_used: 0,
        fail_count: 0,
        accept_count: 0,
        reject_count: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accept_increments_accept_count() {
        let mut stats = HashMap::new();
        stats.insert("ls".to_string(), make_stat("ls"));
        let p = make_signals("0\t1\tls\n");
        apply(&mut stats, &p);
        assert_eq!(stats.get("ls").unwrap().accept_count, 1);
        assert_eq!(stats.get("ls").unwrap().reject_count, 0);
        assert_eq!(stats.get("ls").unwrap().fail_count, 0);
    }

    #[test]
    fn reject_increments_reject_count() {
        let mut stats = HashMap::new();
        stats.insert("ls".to_string(), make_stat("ls"));
        let p = make_signals("0\t2\tls\n");
        apply(&mut stats, &p);
        assert_eq!(stats.get("ls").unwrap().reject_count, 1);
        assert_eq!(stats.get("ls").unwrap().accept_count, 0);
    }

    #[test]
    fn no_suggestion_does_not_count() {
        let mut stats = HashMap::new();
        stats.insert("ls".to_string(), make_stat("ls"));
        let p = make_signals("0\t0\tls\n");
        apply(&mut stats, &p);
        let s = stats.get("ls").unwrap();
        assert_eq!(s.accept_count, 0);
        assert_eq!(s.reject_count, 0);
        assert_eq!(s.fail_count, 0);
    }

    #[test]
    fn fail_count_independent_of_state() {
        let mut stats = HashMap::new();
        stats.insert("ls".to_string(), make_stat("ls"));
        // 失败 + 无 suggestion
        let p = make_signals("1\t0\tls\n2\t2\tls\n");
        apply(&mut stats, &p);
        let s = stats.get("ls").unwrap();
        assert_eq!(s.fail_count, 2, "exit!=0 always counts fail");
        assert_eq!(s.reject_count, 1, "state=2 on second line");
    }

    #[test]
    fn command_with_spaces() {
        let mut stats = HashMap::new();
        stats.insert("git status".to_string(), make_stat("git status"));
        let p = make_signals("0\t1\tgit status\n");
        apply(&mut stats, &p);
        assert_eq!(stats.get("git status").unwrap().accept_count, 1);
    }

    #[test]
    fn command_not_in_history_skipped() {
        let mut stats = HashMap::new();
        stats.insert("ls".to_string(), make_stat("ls"));
        let p = make_signals("0\t1\tunknown\n");
        apply(&mut stats, &p);
        assert!(!stats.contains_key("unknown"));
        assert_eq!(stats.get("ls").unwrap().accept_count, 0);
    }

    #[test]
    fn malformed_lines_skipped() {
        let mut stats = HashMap::new();
        stats.insert("ls".to_string(), make_stat("ls"));
        let p = make_signals(
            "notanumber\t1\tls\n\
             0\tX\tls\n\
             0\t1\n\
             \n\
             0\t1\tls\n",
        );
        apply(&mut stats, &p);
        assert_eq!(stats.get("ls").unwrap().accept_count, 1, "only last line valid");
    }

    #[test]
    fn empty_file_is_noop() {
        let mut stats = HashMap::new();
        stats.insert("ls".to_string(), make_stat("ls"));
        let p = make_signals("");
        apply(&mut stats, &p);
        let s = stats.get("ls").unwrap();
        assert_eq!(s.fail_count, 0);
        assert_eq!(s.accept_count, 0);
        assert_eq!(s.reject_count, 0);
    }

    #[test]
    fn missing_file_is_noop() {
        let mut stats = HashMap::new();
        stats.insert("ls".to_string(), make_stat("ls"));
        let p = std::path::PathBuf::from("/nonexistent/zsh-as-signals-test");
        apply(&mut stats, &p);
        let s = stats.get("ls").unwrap();
        assert_eq!(s.accept_count, 0);
    }
}
