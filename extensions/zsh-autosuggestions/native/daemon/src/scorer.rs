use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use crate::db::{CommandStat, SeqStat};

const HALF_LIFE_SECS: f64 = 7.0 * 24.0 * 3600.0;
const SEQ_HALF_LIFE_SECS: f64 = 3.0 * 24.0 * 3600.0;

const FRECENCY_WEIGHT: f64 = 1.0;
const SEQUENCE_WEIGHT: f64 = 0.6;
const DIR_WEIGHT: f64 = 0.3;
const FAIL_PENALTY: f64 = 0.5;

const FRECENCY_K: f64 = 2.0;
const SEQUENCE_K: f64 = 2.0;

#[derive(Debug, Clone)]
pub struct RankedResult {
    pub command: String,
    pub score: f64,
}

pub fn rank(
    candidates: &[CommandStat],
    dir: &str,
    prev: &str,
    seq_data: &HashMap<String, SeqStat>,
    dir_counts: &HashMap<String, HashMap<String, i64>>,
    now: SystemTime,
) -> Vec<RankedResult> {
    let n = candidates.len();
    if n == 0 {
        return Vec::new();
    }

    let frecency_scores = compute_frecency(candidates, now);
    let dir_scores = compute_dir_affinity(candidates, dir, dir_counts);
    let seq_scores = compute_sequence(candidates, prev, seq_data, now);

    let mut out = Vec::with_capacity(n);
    for (i, c) in candidates.iter().enumerate() {
        let mut final_score = FRECENCY_WEIGHT * frecency_scores[i]
            + SEQUENCE_WEIGHT * seq_scores[i]
            + DIR_WEIGHT * dir_scores[i];

        if c.count > 0 && c.fail_count > 0 {
            let fail_rate = c.fail_count as f64 / c.count as f64;
            final_score *= 1.0 - fail_rate * FAIL_PENALTY;
        }

        out.push(RankedResult {
            command: c.command.clone(),
            score: final_score,
        });
    }

    out.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    out
}

fn compute_frecency(candidates: &[CommandStat], now: SystemTime) -> Vec<f64> {
    let now_secs = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    candidates
        .iter()
        .map(|c| {
            let last_used_secs = c.last_used.clamp(0, now_secs) as u64;
            let dt = now
                .duration_since(SystemTime::UNIX_EPOCH + Duration::from_secs(last_used_secs))
                .unwrap_or(Duration::from_secs(0))
                .as_secs_f64()
                .max(0.0);

            let recency = (-dt / HALF_LIFE_SECS).exp();
            let raw = ((c.count as f64) + 1.0).ln() * recency;
            raw / (raw + FRECENCY_K)
        })
        .collect()
}

fn compute_dir_affinity(
    candidates: &[CommandStat],
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
            if let Some(dc) = dir_counts.get(&c.command) {
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

fn compute_sequence(
    candidates: &[CommandStat],
    prev: &str,
    seq_data: &HashMap<String, SeqStat>,
    now: SystemTime,
) -> Vec<f64> {
    if prev.is_empty() || seq_data.is_empty() {
        return vec![0.0; candidates.len()];
    }

    let now_secs = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    candidates
        .iter()
        .map(|c| {
            if let Some(stat) = seq_data.get(&c.command) {
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
