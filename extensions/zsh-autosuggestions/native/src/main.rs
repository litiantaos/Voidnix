//! zsh-as — Voidnix 终端命令补全 binary。
//!
//! 两个命令：
//!   rebuild  从 .zsh_history + signals.log 重建 sourceable zsh cache（含目录段）
//!   stats    输出诊断信息
//!
//! 无 SQLite，无 daemon，无 IPC。stateless compute kernel。

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::time::SystemTime;

mod frecency;
mod history;
mod signals;

#[derive(Parser)]
#[command(
    name = "zsh-autosuggestions",
    version,
    about = "Voidnix zsh frecency autosuggestions"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// 输出 zsh 集成脚本（直接 include_str，无模板替换，路径走环境变量）
    Init,
    /// 从 .zsh_history + signals.log 重建 index.zsh
    Rebuild {
        #[arg(long)]
        out: PathBuf,
        #[arg(long)]
        history: PathBuf,
        #[arg(long)]
        signals: PathBuf,
        #[arg(long, default_value_t = 7.0, value_parser = parse_half_life)]
        half_life_days: f64,
        #[arg(long, default_value_t = 0.8, value_parser = parse_fail_penalty)]
        fail_penalty: f64,
        #[arg(long, default_value_t = 5000, value_parser = parse_limit)]
        limit: usize,
    },
    /// 输出诊断：行数、cache 状态、最热命令
    Stats {
        #[arg(long)]
        cache: PathBuf,
        #[arg(long)]
        history: PathBuf,
        #[arg(long)]
        signals: PathBuf,
        #[arg(long, default_value_t = 7.0, value_parser = parse_half_life)]
        half_life_days: f64,
        #[arg(long, default_value_t = 0.8, value_parser = parse_fail_penalty)]
        fail_penalty: f64,
    },
}

fn parse_half_life(s: &str) -> Result<f64, String> {
    let v: f64 = s
        .parse()
        .map_err(|_| format!("half-life-days: 无效数值 '{s}'"))?;
    if v < 1e-6 {
        return Err("half-life-days 必须为正数".into());
    }
    Ok(v)
}

fn parse_fail_penalty(s: &str) -> Result<f64, String> {
    let v: f64 = s
        .parse()
        .map_err(|_| format!("fail-penalty: 无效数值 '{s}'"))?;
    if !(0.0..=1.0).contains(&v) {
        return Err("fail-penalty 必须在 0.0..=1.0 范围内".into());
    }
    Ok(v)
}

fn parse_limit(s: &str) -> Result<usize, String> {
    let v: usize = s.parse().map_err(|_| format!("limit: 无效正整数 '{s}'"))?;
    if v == 0 {
        return Err("limit 必须为正整数".into());
    }
    Ok(v)
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Init => {
            print!("{}", include_str!("../zsh/init.zsh"));
        }
        Commands::Rebuild {
            out,
            history,
            signals,
            half_life_days,
            fail_penalty,
            limit,
        } => {
            if let Err(e) = run_rebuild(
                &out,
                &history,
                &signals,
                half_life_days,
                fail_penalty,
                limit,
            ) {
                eprintln!("rebuild: {e}");
                std::process::exit(1);
            }
        }
        Commands::Stats {
            cache,
            history,
            signals,
            half_life_days,
            fail_penalty,
        } => {
            run_stats(&cache, &history, &signals, half_life_days, fail_penalty);
        }
    }
}

fn run_rebuild(
    out: &std::path::Path,
    history_path: &std::path::Path,
    signals_path: &std::path::Path,
    half_life_days: f64,
    fail_penalty: f64,
    limit: usize,
) -> Result<(), String> {
    let now = SystemTime::now();
    let half_life_secs = half_life_days * 86400.0;

    // 先 compact/rotate signals.log：过滤无效行（含旧格式）+ 超大时截断。
    rotate_signals_if_needed(signals_path);

    let mut stats = history::parse(history_path);
    let history_total = stats.len();
    let records = signals::load(signals_path);
    signals::apply(&mut stats, &records);

    let mut scored = frecency::compute(
        &stats.values().cloned().collect::<Vec<_>>(),
        now,
        half_life_secs,
        fail_penalty,
    );
    scored.truncate(limit);

    let dirs = dir_sections(&records, now, half_life_secs, fail_penalty);

    let content = render_cache(&scored, &dirs, history_path, now);
    // tmp 名带 pid：多 shell 并发 rebuild 时不会共享同一 tmp 产生写覆盖竞态。
    let tmp = out.with_extension(format!("tmp.{}", std::process::id()));
    std::fs::write(&tmp, content).map_err(|e| format!("write {}: {}", tmp.display(), e))?;
    std::fs::rename(&tmp, out).map_err(|e| format!("rename: {e}"))?;

    println!(
        "rebuild: {} commands, {} dirs (history: {})",
        scored.len(),
        dirs.len(),
        history_total
    );
    Ok(())
}

/// cache 目录段上限：最多跟踪 32 个目录、每目录 100 条命令。
/// zsh 端按键对目录列表线性扫描，该量级内存与扫描成本可忽略；
/// 信号 log 的 10000 行 rotate 窗口天然淘汰冷目录。
const DIR_LIMIT: usize = 32;
const DIR_CMD_LIMIT: usize = 100;

/// 一个目录的 cache 段：top 命令列表（frecency 降序）。
struct DirSection {
    pwd: String,
    total: u64,
    cmds: Vec<String>,
}

/// 按目录聚合 signals 记录：每目录独立跑 frecency（与全局同公式、同参数），
/// 目录按总执行次数降序取前 DIR_LIMIT 个，每目录命令截断 DIR_CMD_LIMIT。
fn dir_sections(
    records: &[signals::SignalRecord],
    now: SystemTime,
    half_life_secs: f64,
    fail_penalty: f64,
) -> Vec<DirSection> {
    let mut sections: Vec<DirSection> = signals::dir_stats(records)
        .into_iter()
        .map(|(pwd, cmds)| {
            let total: u64 = cmds.values().map(|s| s.count).sum();
            let mut scored = frecency::compute(
                &cmds.values().cloned().collect::<Vec<_>>(),
                now,
                half_life_secs,
                fail_penalty,
            );
            scored.truncate(DIR_CMD_LIMIT);
            DirSection {
                pwd,
                total,
                cmds: scored.into_iter().map(|(c, _)| c).collect(),
            }
        })
        .collect();
    sections.sort_by(|a, b| b.total.cmp(&a.total).then_with(|| a.pwd.cmp(&b.pwd)));
    sections.truncate(DIR_LIMIT);
    sections
}

fn render_cache(
    scored: &[(String, f64)],
    dirs: &[DirSection],
    history_path: &std::path::Path,
    now: SystemTime,
) -> String {
    let now_secs = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let hist_mtime = std::fs::metadata(history_path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let mut out = String::with_capacity(scored.len() * 30);
    out.push_str("# auto-generated by zsh-autosuggestions rebuild, do not edit\n");
    out.push_str(&format!(
        "# version=2 generated={} history_mtime={}\n",
        now_secs, hist_mtime
    ));
    out.push_str("typeset -ga _zsh_autosuggestions_sorted=(\n");
    for (cmd, _) in scored {
        out.push_str(&format!("  {}\n", quote_zsh(cmd)));
    }
    out.push_str(")\n");
    // 目录段：pwd（精确路径）→ 专属命令列表的索引。zsh 匹配侧从 PWD 逐级
    // 上溯消费这些精确键（子目录继承项目根命令），这里只负责按路径键生成。
    // index 用字面量整体赋值（zsh 5.9 下标赋值 m['k']=v 会把引号字符存进 key），
    // 段体为普通数组，均复用 quote_zsh 单行引用。
    out.push_str("typeset -gA _zsh_autosuggestions_dir_index\n");
    if !dirs.is_empty() {
        out.push_str("_zsh_autosuggestions_dir_index=(\n");
        for (i, d) in dirs.iter().enumerate() {
            out.push_str(&format!("  {} d{i}\n", quote_zsh(&d.pwd)));
        }
        out.push_str(")\n");
    }
    for (i, d) in dirs.iter().enumerate() {
        out.push_str(&format!("typeset -ga _zsh_autosuggestions_dir_d{i}=(\n"));
        for cmd in &d.cmds {
            out.push_str(&format!("  {}\n", quote_zsh(cmd)));
        }
        out.push_str(")\n");
    }
    out.push_str("typeset -gi _ZSH_AUTOSUGGESTIONS_IDX_VERSION=2\n");
    out
}

fn quote_zsh(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for c in s.chars() {
        if c == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(c);
        }
    }
    out.push('\'');
    out
}

/// 统计 cache 内命令条目数（全局 sorted 数组 + 目录段数组的元素行）。
/// 数组元素行 `  '...'` 以单引号结尾；目录索引键行 `  '...' d0` 以段名结尾，
/// 不计入命令数。
fn count_cache_entries(text: &str) -> usize {
    text.lines()
        .filter(|l| l.starts_with("  '") && l.ends_with('\''))
        .count()
}

fn run_stats(
    cache: &std::path::Path,
    history: &std::path::Path,
    signals: &std::path::Path,
    half_life_days: f64,
    fail_penalty: f64,
) {
    let cache_exists = cache.exists();
    let cache_size = std::fs::metadata(cache).map(|m| m.len()).unwrap_or(0);
    let cache_lines = if cache_exists {
        std::fs::read_to_string(cache)
            .map(|s| count_cache_entries(&s))
            .unwrap_or(0)
    } else {
        0
    };

    let hist_size = std::fs::metadata(history).map(|m| m.len()).unwrap_or(0);
    let mut stats = history::parse(history);
    let unique = stats.len();
    let total: u64 = stats.values().map(|s| s.count).sum();

    let sig_size = std::fs::metadata(signals).map(|m| m.len()).unwrap_or(0);
    let sig_lines = std::fs::read_to_string(signals)
        .map(|s| s.lines().filter(|l| !l.is_empty()).count())
        .unwrap_or(0);

    println!(
        "cache:    {} ({} bytes, {} entries)",
        exists_label(cache_exists),
        cache_size,
        cache_lines
    );
    println!(
        "history:  {} ({} bytes, {} unique, {} total)",
        history.display(),
        hist_size,
        unique,
        total
    );
    println!(
        "signals:  {} ({} bytes, {} records)",
        signals.display(),
        sig_size,
        sig_lines
    );

    if !stats.is_empty() {
        if !history::is_extended_history(history) {
            println!("\n! 未检测到 EXTENDED_HISTORY，frecency 退化为纯频次排序（所有命令共享文件 mtime），");
            println!("  且多行命令续行会被当作独立命令。建议 `setopt EXTENDED_HISTORY`。");
        }

        // 与 run_rebuild 一致：先 fold signals（fail/accept/reject 计数），再算分。
        // 否则诊断分数与实际 cache 排序不一致，误导排查。
        let records = signals::load(signals);
        signals::apply(&mut stats, &records);

        let now = SystemTime::now();
        let mut scored = frecency::compute(
            &stats.values().cloned().collect::<Vec<_>>(),
            now,
            half_life_days * 86400.0,
            fail_penalty,
        );
        scored.truncate(5);
        println!("\ntop-5 by frecency:");
        for (cmd, score) in scored {
            println!("  {:.4}  {}", score, cmd);
        }

        let dirs = dir_sections(&records, now, half_life_days * 86400.0, fail_penalty);
        if dirs.is_empty() {
            println!("\ndirs:     0 tracked（signals.log 无有效带目录记录）");
        } else {
            println!(
                "\ndirs:     {} tracked (top: {}, {} cmds)",
                dirs.len(),
                dirs[0].pwd,
                dirs[0].cmds.len()
            );
            for cmd in dirs[0].cmds.iter().take(3) {
                println!("  {cmd}");
            }
        }
    }
}

fn exists_label(exists: bool) -> &'static str {
    if exists {
        "ok"
    } else {
        "missing"
    }
}

/// signals.log 维护：过滤格式无效的行（清理旧格式残留），并在文件 >1MB 时
/// 保留最后 10000 行。atomic 写回（tmp + rename）。
///
/// 注意：rename 会覆盖 rebuild 期间 zsh append 的新行；race 窗口 = rebuild 耗时
/// （通常 <150ms），交互式场景丢失少量 signal，frecency 统计近似不受影响。
fn rotate_signals_if_needed(path: &std::path::Path) {
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };
    let all_lines: Vec<&str> = text.lines().filter(|l| !l.is_empty()).collect();
    if all_lines.is_empty() {
        return;
    }

    let valid: Vec<&str> = all_lines
        .iter()
        .filter(|l| signals::parse_line(l).is_some())
        .copied()
        .collect();

    let has_invalid = valid.len() < all_lines.len();
    let meta_len = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let too_big = meta_len > 1_000_000;

    if !has_invalid && !too_big {
        return;
    }

    let keep: &[&str] = if valid.len() > 10_000 {
        &valid[valid.len() - 10_000..]
    } else {
        &valid
    };

    let mut content = keep.join("\n");
    content.push('\n');

    // tmp 名带 pid，避免与并发 rebuild 的 tmp 冲突。
    let tmp = path.with_extension(format!("tmp.{}", std::process::id()));
    if std::fs::write(&tmp, &content).is_err() {
        let _ = std::fs::remove_file(&tmp);
        return;
    }
    if std::fs::rename(&tmp, path).is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(ts: i64, exit: i32, state: u8, pwd: &str, cmd: &str) -> signals::SignalRecord {
        signals::SignalRecord {
            ts,
            exit,
            state,
            pwd: pwd.to_string(),
            cmd: cmd.to_string(),
        }
    }

    #[test]
    fn render_cache_dir_section() {
        let scored = vec![("ls".to_string(), 1.0)];
        let dirs = vec![DirSection {
            pwd: "/Users/x/pro ject".to_string(),
            total: 9,
            cmds: vec!["cargo test".to_string(), "git's status".to_string()],
        }];
        let out = render_cache(
            &scored,
            &dirs,
            std::path::Path::new("/h"),
            SystemTime::now(),
        );
        assert!(out.contains("typeset -gi _ZSH_AUTOSUGGESTIONS_IDX_VERSION=2\n"));
        assert!(out.contains("_zsh_autosuggestions_dir_index=(\n"));
        assert!(out.contains("  '/Users/x/pro ject' d0\n"));
        assert!(out.contains("typeset -ga _zsh_autosuggestions_dir_d0=(\n"));
        assert!(out.contains("  'cargo test'\n"));
        assert!(out.contains("  'git'\\''s status'\n"));
    }

    #[test]
    fn render_cache_no_dirs_still_declares_index() {
        let out = render_cache(&[], &[], std::path::Path::new("/h"), SystemTime::now());
        assert!(out.contains("typeset -gA _zsh_autosuggestions_dir_index\n"));
        assert!(!out.contains("_zsh_autosuggestions_dir_index=("));
    }

    #[test]
    fn dir_sections_orders_by_total() {
        let records = vec![
            rec(1700000000, 0, 0, "/a", "ls"),
            rec(1700000001, 0, 0, "/b", "git status"),
            rec(1700000002, 0, 0, "/b", "git status"),
            rec(1700000003, 0, 0, "/b", "git status"),
        ];
        let sections = dir_sections(&records, SystemTime::now(), 7.0 * 86400.0, 0.8);
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].pwd, "/b", "总次数多的目录在前");
        assert_eq!(sections[0].total, 3);
        assert_eq!(sections[0].cmds, vec!["git status".to_string()]);
    }

    #[test]
    fn dir_sections_keeps_commands_outside_corpus() {
        // 目录维度不要求命令已入 history（会话内即时性 + SAVEHIST 截断不丢）
        let records = vec![
            rec(1700000000, 0, 0, "/a", "ls"),
            rec(1700000001, 0, 0, "/a", "make all"),
        ];
        let sections = dir_sections(&records, SystemTime::now(), 7.0 * 86400.0, 0.8);
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].cmds.len(), 2);
        assert!(sections[0].cmds.contains(&"make all".to_string()));
    }

    #[test]
    fn dir_sections_tie_break_by_pwd() {
        // total 并列时按路径字典序，cache 输出跨 rebuild 确定
        let records = vec![
            rec(1700000000, 0, 0, "/b", "ls"),
            rec(1700000001, 0, 0, "/a", "ls"),
        ];
        let sections = dir_sections(&records, SystemTime::now(), 7.0 * 86400.0, 0.8);
        assert_eq!(sections[0].pwd, "/a");
        assert_eq!(sections[1].pwd, "/b");
    }

    #[test]
    fn count_cache_entries_excludes_dir_index_keys() {
        let cache = r#"typeset -ga _zsh_autosuggestions_sorted=(
  'git status'
)
typeset -gA _zsh_autosuggestions_dir_index
_zsh_autosuggestions_dir_index=(
  '/proj' d0
)
typeset -ga _zsh_autosuggestions_dir_d0=(
  'cargo test'
  'echo 1'
)
"#;
        assert_eq!(
            count_cache_entries(cache),
            3,
            "2 目录命令 + 1 全局，索引键行不计"
        );
    }

    #[test]
    fn quote_zsh_escapes_single_quote() {
        assert_eq!(quote_zsh("it's"), "'it'\\''s'");
        assert_eq!(quote_zsh("plain"), "'plain'");
    }
}
