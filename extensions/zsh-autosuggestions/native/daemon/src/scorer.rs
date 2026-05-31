use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::Matcher;
use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use crate::db::CommandStat;

const HALF_LIFE_SECS: f64 = 7.0 * 24.0 * 3600.0;

const FUZZY_WEIGHT: f64 = 1.0;
const SEQUENCE_WEIGHT: f64 = 0.5;
const FRECENCY_WEIGHT: f64 = 0.4;
const DIR_WEIGHT: f64 = 0.3;

#[derive(Debug, Clone)]
pub struct RankedResult {
    pub command: String,
    pub score: f64,
}

pub fn rank(
    candidates: &[CommandStat],
    buffer: &str,
    dir: &str,
    prev: &str,
    seq_counts: &HashMap<String, i64>,
    dir_counts: &HashMap<String, HashMap<String, i64>>,
    now: SystemTime,
) -> Vec<RankedResult> {
    let n = candidates.len();
    if n == 0 {
        return Vec::new();
    }

    let fuzzy_scores = compute_fuzzy(candidates, buffer);
    let frecency_scores = compute_frecency(candidates, now);
    let dir_scores = compute_dir_affinity(candidates, dir, dir_counts);
    let seq_scores = compute_sequence(candidates, prev, seq_counts);

    let mut out = Vec::with_capacity(n);
    for (i, c) in candidates.iter().enumerate() {
        if !buffer.is_empty() && fuzzy_scores[i] == 0.0 {
            continue;
        }

        let final_score = FUZZY_WEIGHT * fuzzy_scores[i]
            + SEQUENCE_WEIGHT * seq_scores[i]
            + FRECENCY_WEIGHT * frecency_scores[i]
            + DIR_WEIGHT * dir_scores[i];

        out.push(RankedResult {
            command: c.command.clone(),
            score: final_score,
        });
    }

    out.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    out
}

fn compute_fuzzy(candidates: &[CommandStat], buffer: &str) -> Vec<f64> {
    let n = candidates.len();
    let mut scores = vec![0.0; n];

    if buffer.is_empty() {
        for s in scores.iter_mut() {
            *s = 1.0;
        }
        return scores;
    }

    let pattern = Pattern::parse(buffer, CaseMatching::Ignore, Normalization::Smart);
    let mut matcher = Matcher::default();

    let mut raw = vec![0u32; n];
    let mut matched = vec![false; n];
    let mut min = u32::MAX;
    let mut max = 0u32;

    for (i, c) in candidates.iter().enumerate() {
        let needle = nucleo_matcher::Utf32Str::Ascii(c.command.as_bytes());
        if let Some(score) = pattern.score(needle, &mut matcher) {
            raw[i] = score;
            matched[i] = true;
            if score < min {
                min = score;
            }
            if score > max {
                max = score;
            }
        }
    }

    let span = max.saturating_sub(min);
    for i in 0..n {
        if !matched[i] {
            continue;
        }
        if span == 0 {
            scores[i] = 1.0;
        } else {
            scores[i] = (raw[i] - min) as f64 / span as f64;
        }
        if scores[i] < 1e-6 {
            scores[i] = 1e-6;
        }
    }

    scores
}

fn compute_frecency(candidates: &[CommandStat], now: SystemTime) -> Vec<f64> {
    let n = candidates.len();
    let mut raw = vec![0.0f64; n];
    let mut max = 0.0f64;

    let now_secs = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    for (i, c) in candidates.iter().enumerate() {
        let last_used_secs = c.last_used.clamp(0, now_secs as i64) as u64;
        let dt = now
            .duration_since(SystemTime::UNIX_EPOCH + Duration::from_secs(last_used_secs))
            .unwrap_or(Duration::from_secs(0))
            .as_secs_f64()
            .max(0.0);

        let recency = (-dt / HALF_LIFE_SECS).exp();
        raw[i] = ((c.count as f64) + 1.0).ln() * recency;
        if raw[i] > max {
            max = raw[i];
        }
    }

    if max == 0.0 {
        return raw;
    }

    for r in raw.iter_mut() {
        *r /= max;
    }

    raw
}

fn compute_dir_affinity(
    candidates: &[CommandStat],
    dir: &str,
    dir_counts: &HashMap<String, HashMap<String, i64>>,
) -> Vec<f64> {
    let n = candidates.len();
    let mut scores = vec![0.0; n];

    if dir.is_empty() {
        return scores;
    }

    for (i, c) in candidates.iter().enumerate() {
        if let Some(dc) = dir_counts.get(&c.command) {
            let total: i64 = dc.values().sum();
            if total > 0 {
                scores[i] = *dc.get(dir).unwrap_or(&0) as f64 / total as f64;
            }
        }
    }

    scores
}

fn compute_sequence(
    candidates: &[CommandStat],
    prev: &str,
    seq_counts: &HashMap<String, i64>,
) -> Vec<f64> {
    let n = candidates.len();
    let mut scores = vec![0.0; n];

    if prev.is_empty() || seq_counts.is_empty() {
        return scores;
    }

    let max = seq_counts.values().copied().max().unwrap_or(0) as f64;
    if max == 0.0 {
        return scores;
    }

    for (i, c) in candidates.iter().enumerate() {
        if let Some(&count) = seq_counts.get(&c.command) {
            scores[i] = count as f64 / max;
        }
    }

    scores
}
