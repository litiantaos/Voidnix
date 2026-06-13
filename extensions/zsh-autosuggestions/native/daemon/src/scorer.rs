use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use crate::db::{CommandStat, SeqStat};

const HALF_LIFE_SECS: f64 = 7.0 * 24.0 * 3600.0;
const SEQ_HALF_LIFE_SECS: f64 = 3.0 * 24.0 * 3600.0;

const FRECENCY_WEIGHT: f64 = 1.0;
const SEQUENCE_WEIGHT: f64 = 0.6;
const PROJECT_WEIGHT: f64 = 0.4;
const DIR_WEIGHT: f64 = 0.2;
const FAIL_PENALTY: f64 = 0.5;

const FRECENCY_K: f64 = 10.0;
const SEQUENCE_K: f64 = 2.0;
const FRECENCY_POWER: f64 = 0.7;

const FUZZY_PENALTY: f64 = 0.6;

const ACCEPT_ALPHA: f64 = 1.0;
const ACCEPT_BETA: f64 = 2.0;
const ACCEPT_MIN_IMPRESSIONS: i64 = 5;

const MMR_LAMBDA: f64 = 0.7;
const MMR_TOP_POOL: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecallKind {
    Prefix,
    Abbrev,
    Fuzzy,
}

#[derive(Debug, Clone)]
pub struct Candidate {
    pub stat: CommandStat,
    pub kind: RecallKind,
}

#[derive(Debug, Clone)]
pub struct RankedResult {
    pub command: String,
    pub score: f64,
    pub kind: RecallKind,
}

#[derive(Debug, Clone)]
pub struct RankContext<'a> {
    pub dir: &'a str,
    pub project_root: &'a str,
    pub prev: &'a str,
    pub prev_prev: &'a str,
    pub prev_exit: i32,
    pub bigram: &'a HashMap<String, SeqStat>,
    pub trigram: &'a HashMap<String, SeqStat>,
    pub recovery: &'a HashMap<String, SeqStat>,
    pub dir_counts: &'a HashMap<String, HashMap<String, i64>>,
    pub project_counts: &'a HashMap<String, HashMap<String, i64>>,
}

pub fn rank(
    candidates: &[Candidate],
    ctx: &RankContext<'_>,
    now: SystemTime,
) -> Vec<RankedResult> {
    let n = candidates.len();
    if n == 0 {
        return Vec::new();
    }

    let frecency_scores = compute_frecency(candidates, now);
    let dir_scores = compute_dir_affinity(candidates, ctx.dir, ctx.dir_counts);
    let project_scores = compute_project_affinity(candidates, ctx.project_root, ctx.project_counts);
    let seq_scores = compute_sequence(candidates, ctx, now);

    let mut out = Vec::with_capacity(n);
    for (i, c) in candidates.iter().enumerate() {
        let mut final_score = FRECENCY_WEIGHT * frecency_scores[i]
            + SEQUENCE_WEIGHT * seq_scores[i]
            + PROJECT_WEIGHT * project_scores[i]
            + DIR_WEIGHT * dir_scores[i];

        let stat = &c.stat;
        if stat.count > 0 && stat.fail_count > 0 {
            let fail_rate = stat.fail_count as f64 / stat.count as f64;
            final_score *= 1.0 - fail_rate * FAIL_PENALTY;
        }

        if stat.suggested_count >= ACCEPT_MIN_IMPRESSIONS {
            let accepts = stat.accept_count as f64;
            let impressions = stat.suggested_count as f64;
            let prior = (accepts + ACCEPT_ALPHA) / (impressions + ACCEPT_ALPHA + ACCEPT_BETA);
            // map [0,1] → [0.7, 1.3]
            final_score *= 0.7 + 0.6 * prior;
        }

        if matches!(c.kind, RecallKind::Fuzzy) {
            final_score *= FUZZY_PENALTY;
        }

        out.push(RankedResult {
            command: stat.command.clone(),
            score: final_score,
            kind: c.kind,
        });
    }

    out.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    out
}

fn compute_frecency(candidates: &[Candidate], now: SystemTime) -> Vec<f64> {
    let now_secs = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    candidates
        .iter()
        .map(|c| {
            let stat = &c.stat;
            let last_used_secs = stat.last_used.clamp(0, now_secs) as u64;
            let dt = now
                .duration_since(SystemTime::UNIX_EPOCH + Duration::from_secs(last_used_secs))
                .unwrap_or(Duration::from_secs(0))
                .as_secs_f64()
                .max(0.0);

            let recency = (-dt / HALF_LIFE_SECS).exp();
            let raw = ((stat.count as f64) + 1.0).powf(FRECENCY_POWER) * recency;
            raw / (raw + FRECENCY_K)
        })
        .collect()
}

fn compute_dir_affinity(
    candidates: &[Candidate],
    dir: &str,
    dir_counts: &HashMap<String, HashMap<String, i64>>,
) -> Vec<f64> {
    if dir.is_empty() {
        return vec![0.0; candidates.len()];
    }

    let ancestors = build_dir_ancestors(dir);

    candidates
        .iter()
        .map(|c| {
            if let Some(dc) = dir_counts.get(&c.stat.command) {
                let total: i64 = dc.values().sum();
                if total > 0 {
                    for (depth, ancestor) in ancestors.iter().enumerate() {
                        if let Some(&count) = dc.get(ancestor) {
                            let affinity = count as f64 / total as f64;
                            let depth_factor = 1.0 / (1.0 + depth as f64 * 0.2);
                            return affinity * depth_factor;
                        }
                    }
                }
            }
            0.0
        })
        .collect()
}

fn compute_project_affinity(
    candidates: &[Candidate],
    project_root: &str,
    project_counts: &HashMap<String, HashMap<String, i64>>,
) -> Vec<f64> {
    if project_root.is_empty() {
        return vec![0.0; candidates.len()];
    }
    candidates
        .iter()
        .map(|c| {
            if let Some(pc) = project_counts.get(&c.stat.command) {
                let total: i64 = pc.values().sum();
                if total > 0 {
                    if let Some(&count) = pc.get(project_root) {
                        return count as f64 / total as f64;
                    }
                }
            }
            0.0
        })
        .collect()
}

fn compute_sequence(
    candidates: &[Candidate],
    ctx: &RankContext<'_>,
    now: SystemTime,
) -> Vec<f64> {
    if ctx.prev.is_empty() {
        return vec![0.0; candidates.len()];
    }

    // Decide which table to use: trigram > recovery > bigram
    let use_trigram = !ctx.prev_prev.is_empty() && !ctx.trigram.is_empty();
    let use_recovery = !use_trigram && ctx.prev_exit != 0 && !ctx.recovery.is_empty();

    let now_secs = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    candidates
        .iter()
        .map(|c| {
            let cmd = &c.stat.command;
            let stat_opt = if use_trigram {
                ctx.trigram.get(cmd)
            } else if use_recovery {
                ctx.recovery.get(cmd)
            } else {
                ctx.bigram.get(cmd)
            };
            if let Some(stat) = stat_opt {
                let dt = (now_secs - stat.last_seen).max(0) as f64;
                let recency = (-dt / SEQ_HALF_LIFE_SECS).exp();
                let weighted = (stat.count as f64) * recency;
                weighted / (weighted + SEQUENCE_K)
            } else {
                0.0
            }
        })
        .collect()
}

fn build_dir_ancestors(dir: &str) -> Vec<String> {
    let mut ancestors = Vec::new();
    let mut current = dir.trim_end_matches('/').to_string();
    loop {
        if current.is_empty() || current == "/" {
            break;
        }
        ancestors.push(current.clone());
        match current.rfind('/') {
            Some(idx) if idx > 0 => {
                current.truncate(idx);
            }
            _ => break,
        }
    }
    ancestors
}

/// MMR diversification: pick top-1 as primary, then iteratively select alternatives that
/// balance score with dissimilarity to already-selected items.
pub fn diversify(ranked: &[RankedResult], take: usize) -> Vec<RankedResult> {
    if ranked.is_empty() || take == 0 {
        return Vec::new();
    }

    let pool_size = ranked.len().min(MMR_TOP_POOL);
    let pool = &ranked[..pool_size];

    let mut selected: Vec<usize> = Vec::with_capacity(take);
    selected.push(0);

    while selected.len() < take && selected.len() < pool.len() {
        let pick = pick_next(pool, &selected, true).or_else(|| pick_next(pool, &selected, false));
        match pick {
            Some(i) => selected.push(i),
            None => break,
        }
    }

    selected.into_iter().map(|i| pool[i].clone()).collect()
}

fn pick_next(
    pool: &[RankedResult],
    selected: &[usize],
    exclude_prefix_dupes: bool,
) -> Option<usize> {
    let mut best_idx = None;
    let mut best_val = f64::NEG_INFINITY;
    for (i, candidate) in pool.iter().enumerate() {
        if selected.contains(&i) {
            continue;
        }
        if exclude_prefix_dupes
            && selected
                .iter()
                .any(|&j| is_prefix_extension(&candidate.command, &pool[j].command))
        {
            continue;
        }
        let max_sim = selected
            .iter()
            .map(|&j| similarity(&candidate.command, &pool[j].command))
            .fold(0.0_f64, f64::max);
        let mmr = MMR_LAMBDA * candidate.score - (1.0 - MMR_LAMBDA) * max_sim;
        if mmr > best_val {
            best_val = mmr;
            best_idx = Some(i);
        }
    }
    best_idx
}

fn is_prefix_extension(a: &str, b: &str) -> bool {
    // True if one of (a, b) is a strict token-prefix extension of the other.
    let a_tokens: Vec<&str> = a.split_whitespace().collect();
    let b_tokens: Vec<&str> = b.split_whitespace().collect();
    if a_tokens.is_empty() || b_tokens.is_empty() || a_tokens.len() == b_tokens.len() {
        return false;
    }
    let (short, long) = if a_tokens.len() < b_tokens.len() {
        (&a_tokens, &b_tokens)
    } else {
        (&b_tokens, &a_tokens)
    };
    short.iter().zip(long.iter()).all(|(x, y)| x == y)
}

fn similarity(a: &str, b: &str) -> f64 {
    let a_tokens: Vec<&str> = a.split_whitespace().collect();
    let b_tokens: Vec<&str> = b.split_whitespace().collect();
    if a_tokens.is_empty() || b_tokens.is_empty() {
        return 0.0;
    }
    let min_len = a_tokens.len().min(b_tokens.len());
    let mut common = 0usize;
    for i in 0..min_len {
        if a_tokens[i] == b_tokens[i] {
            common += 1;
        } else {
            break;
        }
    }
    if common == 0 {
        return 0.0;
    }
    // If one is a strict prefix-token-superset of the other, treat as highly
    // similar — these are almost always redundant alternatives in cycle UI.
    if common == min_len {
        return 1.0;
    }
    common as f64 / min_len as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stat(command: &str, count: i64, last_used: i64) -> CommandStat {
        CommandStat {
            command: command.to_string(),
            count,
            last_used,
            fail_count: 0,
            accept_count: 0,
            reject_count: 0,
            suggested_count: 0,
        }
    }

    fn cand(s: CommandStat, kind: RecallKind) -> Candidate {
        Candidate { stat: s, kind }
    }

    fn empty_ctx<'a>(
        bigram: &'a HashMap<String, SeqStat>,
        trigram: &'a HashMap<String, SeqStat>,
        recovery: &'a HashMap<String, SeqStat>,
        dir_counts: &'a HashMap<String, HashMap<String, i64>>,
        project_counts: &'a HashMap<String, HashMap<String, i64>>,
    ) -> RankContext<'a> {
        RankContext {
            dir: "",
            project_root: "",
            prev: "",
            prev_prev: "",
            prev_exit: 0,
            bigram,
            trigram,
            recovery,
            dir_counts,
            project_counts,
        }
    }

    #[test]
    fn frecency_separates_high_count_from_low() {
        let now = SystemTime::now();
        let now_secs = now.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs() as i64;
        let hot = stat("hot", 200, now_secs - 2 * 86400);
        let cold = stat("cold", 3, now_secs);
        let cands = vec![
            cand(hot, RecallKind::Prefix),
            cand(cold, RecallKind::Prefix),
        ];
        let bigram = HashMap::new();
        let trigram = HashMap::new();
        let recovery = HashMap::new();
        let dir_counts = HashMap::new();
        let project_counts = HashMap::new();
        let ctx = empty_ctx(&bigram, &trigram, &recovery, &dir_counts, &project_counts);
        let ranked = rank(&cands, &ctx, now);
        assert_eq!(ranked[0].command, "hot");
        assert!(ranked[0].score - ranked[1].score > 0.2);
    }

    #[test]
    fn trigram_used_when_pp_present() {
        let now = SystemTime::now();
        let now_secs = now.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs() as i64;

        let a = stat("a", 1, now_secs);
        let b = stat("b", 1, now_secs);
        let cands = vec![cand(a, RecallKind::Prefix), cand(b, RecallKind::Prefix)];

        let mut bigram = HashMap::new();
        bigram.insert("a".to_string(), SeqStat { count: 10, last_seen: now_secs });
        bigram.insert("b".to_string(), SeqStat { count: 1, last_seen: now_secs });

        let mut trigram = HashMap::new();
        trigram.insert("b".to_string(), SeqStat { count: 5, last_seen: now_secs });

        let recovery = HashMap::new();
        let dir_counts = HashMap::new();
        let project_counts = HashMap::new();

        let ctx = RankContext {
            dir: "",
            project_root: "",
            prev: "x",
            prev_prev: "y",
            prev_exit: 0,
            bigram: &bigram,
            trigram: &trigram,
            recovery: &recovery,
            dir_counts: &dir_counts,
            project_counts: &project_counts,
        };
        let ranked = rank(&cands, &ctx, now);
        assert_eq!(ranked[0].command, "b");
    }

    #[test]
    fn recovery_used_only_when_prev_failed() {
        let now = SystemTime::now();
        let now_secs = now.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs() as i64;

        let a = stat("a", 1, now_secs);
        let b = stat("b", 1, now_secs);
        let cands = vec![cand(a, RecallKind::Prefix), cand(b, RecallKind::Prefix)];

        let mut bigram = HashMap::new();
        bigram.insert("a".to_string(), SeqStat { count: 10, last_seen: now_secs });

        let mut recovery = HashMap::new();
        recovery.insert("b".to_string(), SeqStat { count: 10, last_seen: now_secs });

        let trigram = HashMap::new();
        let dir_counts = HashMap::new();
        let project_counts = HashMap::new();

        let ctx_fail = RankContext {
            dir: "",
            project_root: "",
            prev: "p",
            prev_prev: "",
            prev_exit: 1,
            bigram: &bigram,
            trigram: &trigram,
            recovery: &recovery,
            dir_counts: &dir_counts,
            project_counts: &project_counts,
        };
        let ranked = rank(&cands, &ctx_fail, now);
        assert_eq!(ranked[0].command, "b");

        let ctx_ok = RankContext {
            prev_exit: 0,
            ..ctx_fail.clone()
        };
        let ranked = rank(&cands, &ctx_ok, now);
        assert_eq!(ranked[0].command, "a");
    }

    #[test]
    fn acceptance_prior_kicks_in_after_threshold() {
        let now = SystemTime::now();
        let now_secs = now.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs() as i64;

        let mut accepted = stat("loved", 5, now_secs);
        accepted.accept_count = 8;
        accepted.suggested_count = 10;

        let neutral = stat("plain", 5, now_secs);

        let cands = vec![
            cand(accepted, RecallKind::Prefix),
            cand(neutral, RecallKind::Prefix),
        ];

        let bigram = HashMap::new();
        let trigram = HashMap::new();
        let recovery = HashMap::new();
        let dir_counts = HashMap::new();
        let project_counts = HashMap::new();
        let ctx = empty_ctx(&bigram, &trigram, &recovery, &dir_counts, &project_counts);

        let ranked = rank(&cands, &ctx, now);
        assert_eq!(ranked[0].command, "loved");
    }

    #[test]
    fn acceptance_prior_skipped_below_threshold() {
        let now = SystemTime::now();
        let now_secs = now.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs() as i64;

        let mut low_impr = stat("a", 5, now_secs);
        low_impr.accept_count = 4;
        low_impr.suggested_count = 4;

        let neutral = stat("b", 5, now_secs);

        let cands = vec![
            cand(low_impr.clone(), RecallKind::Prefix),
            cand(neutral.clone(), RecallKind::Prefix),
        ];

        let bigram = HashMap::new();
        let trigram = HashMap::new();
        let recovery = HashMap::new();
        let dir_counts = HashMap::new();
        let project_counts = HashMap::new();
        let ctx = empty_ctx(&bigram, &trigram, &recovery, &dir_counts, &project_counts);

        let ranked = rank(&cands, &ctx, now);
        // Both have identical base score, low_impr should not get the prior boost
        assert!((ranked[0].score - ranked[1].score).abs() < 1e-9);
    }

    #[test]
    fn fuzzy_candidate_demoted() {
        let now = SystemTime::now();
        let now_secs = now.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs() as i64;

        let a = stat("a", 10, now_secs);
        let b = stat("b", 10, now_secs);
        let cands = vec![cand(a, RecallKind::Fuzzy), cand(b, RecallKind::Prefix)];

        let bigram = HashMap::new();
        let trigram = HashMap::new();
        let recovery = HashMap::new();
        let dir_counts = HashMap::new();
        let project_counts = HashMap::new();
        let ctx = empty_ctx(&bigram, &trigram, &recovery, &dir_counts, &project_counts);
        let ranked = rank(&cands, &ctx, now);
        assert_eq!(ranked[0].command, "b");
    }

    #[test]
    fn project_affinity_prefers_current_project() {
        let now = SystemTime::now();
        let now_secs = now.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs() as i64;
        let a = stat("a", 5, now_secs);
        let b = stat("b", 5, now_secs);
        let cands = vec![cand(a, RecallKind::Prefix), cand(b, RecallKind::Prefix)];

        let mut project_counts: HashMap<String, HashMap<String, i64>> = HashMap::new();
        let mut ma = HashMap::new();
        ma.insert("/proj/x".to_string(), 5);
        project_counts.insert("a".to_string(), ma);
        let mut mb = HashMap::new();
        mb.insert("/proj/y".to_string(), 5);
        project_counts.insert("b".to_string(), mb);

        let bigram = HashMap::new();
        let trigram = HashMap::new();
        let recovery = HashMap::new();
        let dir_counts = HashMap::new();
        let ctx = RankContext {
            dir: "",
            project_root: "/proj/x",
            prev: "",
            prev_prev: "",
            prev_exit: 0,
            bigram: &bigram,
            trigram: &trigram,
            recovery: &recovery,
            dir_counts: &dir_counts,
            project_counts: &project_counts,
        };
        let ranked = rank(&cands, &ctx, now);
        assert_eq!(ranked[0].command, "a");
    }

    #[test]
    fn mmr_removes_prefix_duplicates() {
        let ranked = vec![
            RankedResult { command: "git pull".to_string(), score: 1.0, kind: RecallKind::Prefix },
            RankedResult { command: "git pull origin main".to_string(), score: 0.95, kind: RecallKind::Prefix },
            RankedResult { command: "git pull --rebase".to_string(), score: 0.94, kind: RecallKind::Prefix },
            RankedResult { command: "git status".to_string(), score: 0.6, kind: RecallKind::Prefix },
            RankedResult { command: "ls -la".to_string(), score: 0.5, kind: RecallKind::Prefix },
        ];
        let picked = diversify(&ranked, 3);
        assert_eq!(picked[0].command, "git pull");
        // Second pick must NOT be a prefix-derivative of git pull
        assert!(!picked[1].command.starts_with("git pull "));
    }
}

