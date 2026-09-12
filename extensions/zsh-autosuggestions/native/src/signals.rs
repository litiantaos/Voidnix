//! signals.log 解析。zsh 端 precmd 钩子在每条命令执行后 append 一行（5 字段 TSV）：
//!   `<ts>\t<exit>\t<state>\t<pwd>\t<cmd>`
//! 字段：
//!   ts      记录时间（EPOCHSECONDS，precmd 时刻；目录 frecency 的时间源）
//!   exit    命令退出码（0=成功，非 0=失败）。失败统计独立于 suggestion。
//!   state   suggestion 互动状态：
//!             0 = 无 suggestion 互动（未显示，或不计）
//!             1 = accepted（用户通过 →/end 等接受 suggestion）
//!             2 = rejected（显示了 suggestion 但用户未接受，自行改写或忽略）
//!   pwd     命令执行时所在目录（preexec 时刻的 $PWD，目录建议按此聚合）
//!   cmd     sanitize 后的命令文本（不含 \t \n \r 等控制字符）
//!
//! 每条执行过的命令都 append（目录频次需要正向信号，非仅失败/互动）；
//! zsh 端跳过空白前缀命令（对齐 HIST_IGNORE_SPACE 的隐私语义），体积由
//! rebuild 入口 rotate 控制。
//!
//! 前期开发：仅解析 5 字段格式，格式错误行（含旧 3 字段格式）直接 skip，不做兼容。

use std::collections::HashMap;
use std::path::Path;

use crate::frecency::CommandStat;
use crate::history::is_safe;

#[derive(Clone, Debug)]
pub struct SignalRecord {
    pub ts: i64,
    pub exit: i32,
    pub state: u8,
    pub pwd: String,
    pub cmd: String,
}

/// 读取并解析整个 signals.log，格式无效的行 skip。
pub fn load(path: &Path) -> Vec<SignalRecord> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(_) => return Vec::new(),
    };
    String::from_utf8_lossy(&bytes)
        .lines()
        .filter_map(parse_line)
        .collect()
}

/// 全局反馈聚合：fail/accept/reject 计数叠加到 history 语料上（不消费 pwd）。
/// 只修正已存在条目，不引入新命令（history 是全局语料的唯一权威）。
pub fn apply(stats: &mut HashMap<String, CommandStat>, records: &[SignalRecord]) {
    for r in records {
        let cmd = r.cmd.trim();
        if cmd.is_empty() {
            continue;
        }
        let Some(s) = stats.get_mut(cmd) else {
            continue;
        };

        if r.exit != 0 {
            s.fail_count += 1;
        }
        match r.state {
            1 => s.accept_count += 1,
            2 => s.reject_count += 1,
            _ => {}
        }
    }
}

/// 目录聚合：按 pwd 分组统计每命令的频次/最近使用/失败/接受。
/// 目录维度以 signals 执行记录为准，不要求已入 history——默认 zsh 配置
/// history 退出才落盘，会话内命令需即时建议；SAVEHIST 截断的老命令不丢。
pub fn dir_stats(records: &[SignalRecord]) -> HashMap<String, HashMap<String, CommandStat>> {
    let mut dirs: HashMap<String, HashMap<String, CommandStat>> = HashMap::new();
    for r in records {
        let cmd = r.cmd.trim();
        if cmd.is_empty() {
            continue;
        }
        let per_dir = dirs.entry(r.pwd.clone()).or_default();
        let s = per_dir
            .entry(cmd.to_string())
            .or_insert_with(|| CommandStat {
                command: cmd.to_string(),
                count: 0,
                last_used: 0,
                fail_count: 0,
                accept_count: 0,
                reject_count: 0,
            });
        s.count += 1;
        if r.ts > s.last_used {
            s.last_used = r.ts;
        }
        if r.exit != 0 {
            s.fail_count += 1;
        }
        match r.state {
            1 => s.accept_count += 1,
            2 => s.reject_count += 1,
            _ => {}
        }
    }
    dirs
}

/// 解析单行 signals 记录。格式不合法返回 `None`。
/// `state`：0=无 suggestion 互动，1=accepted，2=rejected。
/// pwd 必须为不含控制字符的绝对路径（TSV 行结构 + 路径合法性）。
pub fn parse_line(line: &str) -> Option<SignalRecord> {
    let mut parts = line.splitn(5, '\t');
    let ts = parts.next()?.parse::<i64>().ok()?;
    let exit = parts.next()?.parse::<i32>().ok()?;
    let state = match parts.next()? {
        "0" => 0u8,
        "1" => 1,
        "2" => 2,
        _ => return None,
    };
    let pwd = parts.next()?;
    let cmd = parts.next()?;
    if ts < 0
        || pwd.is_empty()
        || !pwd.starts_with('/')
        || !is_safe(pwd)
        || cmd.is_empty()
        || !is_safe(cmd)
    {
        return None;
    }
    Some(SignalRecord {
        ts,
        exit,
        state,
        pwd: pwd.to_string(),
        cmd: cmd.to_string(),
    })
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
        let p = make_signals("1700000000\t0\t1\t/tmp\tls\n");
        apply(&mut stats, &load(&p));
        assert_eq!(stats.get("ls").unwrap().accept_count, 1);
        assert_eq!(stats.get("ls").unwrap().reject_count, 0);
        assert_eq!(stats.get("ls").unwrap().fail_count, 0);
    }

    #[test]
    fn reject_increments_reject_count() {
        let mut stats = HashMap::new();
        stats.insert("ls".to_string(), make_stat("ls"));
        let p = make_signals("1700000000\t0\t2\t/tmp\tls\n");
        apply(&mut stats, &load(&p));
        assert_eq!(stats.get("ls").unwrap().reject_count, 1);
        assert_eq!(stats.get("ls").unwrap().accept_count, 0);
    }

    #[test]
    fn no_suggestion_does_not_count() {
        let mut stats = HashMap::new();
        stats.insert("ls".to_string(), make_stat("ls"));
        let p = make_signals("1700000000\t0\t0\t/tmp\tls\n");
        apply(&mut stats, &load(&p));
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
        let p = make_signals("1700000000\t1\t0\t/tmp\tls\n1700000001\t2\t2\t/tmp\tls\n");
        apply(&mut stats, &load(&p));
        let s = stats.get("ls").unwrap();
        assert_eq!(s.fail_count, 2, "exit!=0 always counts fail");
        assert_eq!(s.reject_count, 1, "state=2 on second line");
    }

    #[test]
    fn command_with_spaces() {
        let mut stats = HashMap::new();
        stats.insert("git status".to_string(), make_stat("git status"));
        let p = make_signals("1700000000\t0\t1\t/tmp\tgit status\n");
        apply(&mut stats, &load(&p));
        assert_eq!(stats.get("git status").unwrap().accept_count, 1);
    }

    #[test]
    fn command_not_in_history_skipped() {
        let mut stats = HashMap::new();
        stats.insert("ls".to_string(), make_stat("ls"));
        let p = make_signals("1700000000\t0\t1\t/tmp\tunknown\n");
        apply(&mut stats, &load(&p));
        assert!(!stats.contains_key("unknown"));
        assert_eq!(stats.get("ls").unwrap().accept_count, 0);
    }

    #[test]
    fn malformed_lines_skipped() {
        let mut stats = HashMap::new();
        stats.insert("ls".to_string(), make_stat("ls"));
        let p = make_signals(
            "notanumber\t0\t1\t/tmp\tls\n\
             1700000000\t0\tX\t/tmp\tls\n\
             1700000000\t0\t1\tls\n\
             \n\
             1700000000\t0\t1\t/tmp\tls\n",
        );
        apply(&mut stats, &load(&p));
        assert_eq!(
            stats.get("ls").unwrap().accept_count,
            1,
            "only last line valid"
        );
    }

    #[test]
    fn legacy_3_field_line_rejected() {
        assert!(parse_line("0\t1\tls").is_none());
        // 4 字段（无 ts）同样拒绝
        assert!(parse_line("0\t1\t/tmp\tls").is_none());
    }

    #[test]
    fn parse_line_validates_fields() {
        assert!(parse_line("1700000000\t0\t1\t/tmp\tls").is_some());
        // 相对路径 pwd
        assert!(parse_line("1700000000\t0\t1\ttmp\tls").is_none());
        // 空 pwd
        assert!(parse_line("1700000000\t0\t1\t\tls").is_none());
        // 负 ts
        assert!(parse_line("-1\t0\t1\t/tmp\tls").is_none());
        // cmd 含控制字符
        assert!(parse_line("1700000000\t0\t1\t/tmp\tls\t-la").is_none());
        // 空 cmd
        assert!(parse_line("1700000000\t0\t1\t/tmp\t").is_none());
    }

    #[test]
    fn dir_stats_aggregates_per_pwd() {
        let p = make_signals(
            "1700000000\t0\t0\t/proj\tcargo test\n\
             1700000005\t1\t0\t/proj\tcargo test\n\
             1700000003\t0\t1\t/other\tcargo test\n",
        );
        let dirs = dir_stats(&load(&p));
        assert_eq!(dirs.len(), 2);
        let proj = dirs.get("/proj").unwrap();
        let s = proj.get("cargo test").unwrap();
        assert_eq!(s.count, 2);
        assert_eq!(s.last_used, 1700000005);
        assert_eq!(s.fail_count, 1);
        assert_eq!(s.accept_count, 0);
        let other = dirs.get("/other").unwrap();
        assert_eq!(other.get("cargo test").unwrap().accept_count, 1);
    }

    #[test]
    fn dir_stats_keeps_commands_outside_corpus() {
        // 目录维度不要求命令已入 history：会话内未落盘命令、SAVEHIST 截断命令均可建议
        let p = make_signals("1700000000\t0\t0\t/proj\tls\n1700000001\t0\t0\t/proj\tmake all\n");
        let dirs = dir_stats(&load(&p));
        let proj = dirs.get("/proj").unwrap();
        assert!(proj.contains_key("ls"));
        assert!(proj.contains_key("make all"));
    }

    #[test]
    fn empty_file_is_noop() {
        let mut stats = HashMap::new();
        stats.insert("ls".to_string(), make_stat("ls"));
        let p = make_signals("");
        apply(&mut stats, &load(&p));
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
        apply(&mut stats, &load(&p));
        let s = stats.get("ls").unwrap();
        assert_eq!(s.accept_count, 0);
    }
}
