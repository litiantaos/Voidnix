//! 命令召回：从内存索引中按 buffer 拉候选。
//!
//! 三路召回（union 去重）：
//!   - Path A：精确字节前缀（与 zsh 历史一致）
//!   - Path C：首字母缩写倒排索引（`gcm` → `git checkout main`）
//!   - Path D：Damerau-Levenshtein ≤ 1 兜底（仅在 A+C 召回 < 3 时启用，
//!     识别 `gti` 类相邻字符 typo），Fuzzy 候选打分系数 0.6
//!
//! Path B（模板聚合）已移除：在 Path A 字节前缀设计下，任何 representative
//! 都已被 Path A 收录，模板路径无法贡献新候选。

use std::collections::{HashMap, HashSet};

use crate::db::CommandStat;
use crate::scorer::{Candidate, RecallKind};

const FUZZY_MIN_BUFFER: usize = 2;
const FUZZY_MAX_BUFFER: usize = 12;
const ABBREV_MAX_LEN: usize = 6;
const FUZZY_FALLBACK_THRESHOLD: usize = 3;

/// 构建 initials 倒排索引：token → 命令在 stats 中的下标列表。
pub(super) fn build_initials_index(stats: &[CommandStat]) -> HashMap<String, Vec<usize>> {
    let mut map: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, s) in stats.iter().enumerate() {
        for token in initials_of(&s.command) {
            map.entry(token).or_default().push(i);
        }
    }
    map
}

/// 提取命令的多级首字母缩写：每多识别一个 token 就追加一个缩写。
/// 非字母 token 立即中断（flag 类 token 不参与缩写）。
pub(super) fn initials_of(command: &str) -> Vec<String> {
    let tokens: Vec<&str> = command.split_whitespace().collect();
    let mut out = Vec::new();
    let max_take = tokens.len().min(6);
    let mut buf = String::new();
    for t in tokens.iter().take(max_take) {
        let ch = t.chars().next().and_then(|c| {
            if c.is_ascii_alphabetic() {
                Some(c.to_ascii_lowercase())
            } else {
                None
            }
        });
        match ch {
            Some(c) => buf.push(c),
            None => break,
        }
        if buf.len() >= 2 {
            out.push(buf.clone());
        }
    }
    out
}

/// stat 是否可被建议。
/// `count == 0 && accept_count == 0` 表示这条记录只来自 feedback（impression/reject）
/// 而从未真正执行过、也从未被 accept，是 feedback 早于 record 到达时插入的幽灵行，
/// 不应进入候选池。
pub(super) fn is_suggestable(s: &CommandStat) -> bool {
    s.count > 0 || s.accept_count > 0
}

/// 召回主入口。
///
/// 与原 `recall(inner, buffer)` 不同，这里直接接 `stats` 和 `initials_index`，
/// 让本模块与 `StateInner` 解耦——recall 是纯函数式逻辑，不持有状态。
pub(super) fn recall(
    stats: &[CommandStat],
    initials_index: &HashMap<String, Vec<usize>>,
    buffer: &str,
) -> Vec<Candidate> {
    let buffer = buffer.trim_start();
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<Candidate> = Vec::new();

    // Path A: 字节前缀（空 buffer 时返回全部 suggestable stats）
    if !buffer.is_empty() {
        for s in stats.iter().filter(|s| is_suggestable(s) && s.command.starts_with(buffer)) {
            if seen.insert(s.command.clone()) {
                out.push(Candidate {
                    stat: s.clone(),
                    kind: RecallKind::Prefix,
                });
            }
        }
    } else {
        for s in stats.iter().filter(|s| is_suggestable(s)) {
            if seen.insert(s.command.clone()) {
                out.push(Candidate {
                    stat: s.clone(),
                    kind: RecallKind::Prefix,
                });
            }
        }
        return out;
    }

    // Path C: 缩写。要求 buffer 全字母、长度 2..=ABBREV_MAX_LEN、无空格。
    if buffer.len() >= 2
        && buffer.len() <= ABBREV_MAX_LEN
        && buffer.chars().all(|c| c.is_ascii_alphabetic())
    {
        let key = buffer.to_ascii_lowercase();
        if let Some(idxs) = initials_index.get(&key) {
            for &i in idxs {
                if let Some(stat) = stats.get(i) {
                    if seen.insert(stat.command.clone()) {
                        out.push(Candidate {
                            stat: stat.clone(),
                            kind: RecallKind::Abbrev,
                        });
                    }
                }
            }
        }
    }

    // Path D: fuzzy 兜底。仅在 A+C 召回不足时启用。
    if out.len() < FUZZY_FALLBACK_THRESHOLD
        && buffer.len() >= FUZZY_MIN_BUFFER
        && buffer.len() <= FUZZY_MAX_BUFFER
    {
        let head = buffer.split_whitespace().next().unwrap_or("");
        if !head.is_empty() {
            for s in stats.iter() {
                if seen.contains(&s.command) {
                    continue;
                }
                let cmd_head = s.command.split_whitespace().next().unwrap_or("");
                if cmd_head.is_empty() {
                    continue;
                }
                if levenshtein_at_most_one(head, cmd_head) {
                    seen.insert(s.command.clone());
                    out.push(Candidate {
                        stat: s.clone(),
                        kind: RecallKind::Fuzzy,
                    });
                }
            }
        }
    }

    out
}

/// Damerau-Levenshtein 距离 ≤ 1：单次替换/插入/删除/相邻转置。
/// 全 ASCII 实现，按字节比较。
fn levenshtein_at_most_one(a: &str, b: &str) -> bool {
    if a == b {
        return false; // 完全相等不算 fuzzy
    }
    let la = a.len();
    let lb = b.len();
    if la.abs_diff(lb) > 1 {
        return false;
    }
    let ab = a.as_bytes();
    let bb = b.as_bytes();

    if la == lb {
        // 等长：检查单次替换 或 相邻转置
        let mut diffs: [usize; 3] = [0; 3];
        let mut n = 0usize;
        for i in 0..la {
            if ab[i] != bb[i] {
                if n >= 2 {
                    return false;
                }
                diffs[n] = i;
                n += 1;
            }
        }
        match n {
            1 => true,
            2 => {
                let (p, q) = (diffs[0], diffs[1]);
                q == p + 1 && ab[p] == bb[q] && ab[q] == bb[p]
            }
            _ => false,
        }
    } else {
        // 长度差 1：单次插入/删除
        let (s, l) = if la < lb { (ab, bb) } else { (bb, ab) };
        let mut i = 0usize;
        let mut j = 0usize;
        let mut skipped = false;
        while i < s.len() && j < l.len() {
            if s[i] == l[j] {
                i += 1;
                j += 1;
            } else if skipped {
                return false;
            } else {
                skipped = true;
                j += 1;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stat(command: &str, count: i64) -> CommandStat {
        CommandStat {
            command: command.to_string(),
            count,
            last_used: 0,
            fail_count: 0,
            accept_count: 0,
            reject_count: 0,
            suggested_count: 0,
        }
    }

    #[test]
    fn initials_basic() {
        let got = initials_of("git checkout main");
        assert!(got.contains(&"gc".to_string()));
        assert!(got.contains(&"gcm".to_string()));
    }

    #[test]
    fn initials_ignores_non_alpha() {
        let got = initials_of("./script.sh foo");
        assert!(got.is_empty());
    }

    #[test]
    fn recall_path_a_prefix() {
        let stats = vec![
            stat("git status", 5),
            stat("git commit", 3),
            stat("ls -la", 1),
        ];
        let idx = build_initials_index(&stats);
        let cands = recall(&stats, &idx, "git ");
        let cmds: Vec<&str> = cands.iter().map(|c| c.stat.command.as_str()).collect();
        assert_eq!(cmds, vec!["git status", "git commit"]);
        assert!(cands.iter().all(|c| matches!(c.kind, RecallKind::Prefix)));
    }

    #[test]
    fn recall_path_a_byte_prefix_includes_partial_token_match() {
        // Path A 字节前缀：buffer=git 同时召回 git/github/gitea（zsh 历史行为）
        let stats = vec![
            stat("git status", 5),
            stat("github PR", 3),
            stat("gitea push", 1),
        ];
        let idx = build_initials_index(&stats);
        let cands = recall(&stats, &idx, "git");
        let cmds: Vec<&str> = cands.iter().map(|c| c.stat.command.as_str()).collect();
        assert!(cmds.contains(&"git status"));
        assert!(cmds.contains(&"github PR"));
        assert!(cmds.contains(&"gitea push"));
    }

    #[test]
    fn recall_path_c_abbreviation() {
        let stats = vec![stat("git checkout main", 5), stat("git commit", 3)];
        let idx = build_initials_index(&stats);
        let cands = recall(&stats, &idx, "gcm");
        let cmds: Vec<&str> = cands.iter().map(|c| c.stat.command.as_str()).collect();
        assert!(cmds.contains(&"git checkout main"));
        assert!(cands.iter().any(|c| matches!(c.kind, RecallKind::Abbrev)));
    }

    #[test]
    fn recall_path_d_fuzzy_only_when_a_b_c_insufficient() {
        let stats = vec![stat("git status", 5)];
        let idx = build_initials_index(&stats);
        let cands = recall(&stats, &idx, "gti");
        let fuzzy_cmds: Vec<&str> = cands
            .iter()
            .filter(|c| matches!(c.kind, RecallKind::Fuzzy))
            .map(|c| c.stat.command.as_str())
            .collect();
        assert!(fuzzy_cmds.contains(&"git status"));
    }

    #[test]
    fn recall_path_d_skipped_when_a_b_c_sufficient() {
        let stats = vec![
            stat("git a", 1),
            stat("git b", 1),
            stat("git c", 1),
            stat("git status", 1),
        ];
        let idx = build_initials_index(&stats);
        let cands = recall(&stats, &idx, "git");
        assert!(cands.iter().all(|c| !matches!(c.kind, RecallKind::Fuzzy)));
    }

    #[test]
    fn recall_skips_phantom_stats() {
        let mut phantom = stat("never executed", 0);
        phantom.suggested_count = 5;
        let stats = vec![phantom, stat("real cmd", 3)];
        let idx = build_initials_index(&stats);
        let cands = recall(&stats, &idx, "");
        let cmds: Vec<&str> = cands.iter().map(|c| c.stat.command.as_str()).collect();
        assert!(!cmds.contains(&"never executed"));
        assert!(cmds.contains(&"real cmd"));
    }

    #[test]
    fn levenshtein_one() {
        assert!(levenshtein_at_most_one("gti", "git"));
        assert!(levenshtein_at_most_one("got", "go"));
        assert!(!levenshtein_at_most_one("gxx", "git"));
        assert!(!levenshtein_at_most_one("git", "git"));
    }
}
