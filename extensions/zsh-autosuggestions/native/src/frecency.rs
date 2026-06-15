//! Frecency 评分。
//!
//! 核心公式（归一化到 0..1）：
//! ```text
//! raw     = (count + 1)^0.7 * exp(-dt / half_life)
//! base    = raw / (raw + K)          // K=10，饱和曲线
//! penalty = 1 - sqrt(fail_rate) * fail_penalty   // 失败率折扣
//! boost   = 0.7 + 0.3 * accept_rate  // 接受率加成
//! score   = base * penalty * boost
//! ```

use std::time::SystemTime;

const K: f64 = 10.0;
const POWER: f64 = 0.7;

#[derive(Clone, Debug)]
pub struct CommandStat {
    pub command: String,
    pub count: u64,
    pub last_used: i64,
    pub fail_count: u64,
    pub accept_count: u64,
    pub reject_count: u64,
}

/// 半衰期单位：秒。失败惩罚：`score *= 1 - sqrt(fail_rate) * fail_penalty`。
/// 接受加成：`score *= 0.7 + 0.3 * accept_rate`（accept_rate=1.0 不衰减，=0 时 ×0.7）。
pub fn compute(
    stats: &[CommandStat],
    now: SystemTime,
    half_life_secs: f64,
    fail_penalty: f64,
) -> Vec<(String, f64)> {
    let now_secs = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let mut scored: Vec<(String, f64)> = stats
        .iter()
        .map(|s| {
            let dt = (now_secs - s.last_used).max(0) as f64;
            let recency = (-dt / half_life_secs).exp();
            let raw = ((s.count as f64) + 1.0).powf(POWER) * recency;
            let mut score = raw / (raw + K);

            if s.count > 0 && s.fail_count > 0 {
                // fail_count 来自 signals（不去重），count 来自 history（受
                // HIST_IGNORE_DUPS 去重），二者不同总体，fail_count 可能 > count。
                // clamp 到 1.0 防止 sqrt(>1)*fail_penalty > 1 导致 score 钳 0。
                let fail_rate = (s.fail_count as f64 / s.count as f64).min(1.0);
                score *= 1.0 - fail_rate.sqrt() * fail_penalty;
            }

            let signals = s.accept_count + s.reject_count;
            if signals > 0 {
                let accept_rate = s.accept_count as f64 / signals as f64;
                score *= 0.7 + 0.3 * accept_rate;
            }

            (s.command.clone(), score.max(0.0))
        })
        .collect();

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stat(cmd: &str, count: u64, last_used: i64) -> CommandStat {
        CommandStat {
            command: cmd.to_string(),
            count,
            last_used,
            fail_count: 0,
            accept_count: 0,
            reject_count: 0,
        }
    }

    #[test]
    fn high_count_beats_low() {
        let now = SystemTime::now();
        let now_secs = now
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let stats = vec![
            stat("hot", 200, now_secs - 2 * 86400),
            stat("cold", 3, now_secs),
        ];
        let scored = compute(&stats, now, 7.0 * 86400.0, 0.8);
        assert_eq!(scored[0].0, "hot");
        assert!(scored[0].1 > scored[1].1);
    }

    #[test]
    fn recent_beats_old() {
        let now = SystemTime::now();
        let now_secs = now
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let stats = vec![
            stat("recent", 10, now_secs),
            stat("old", 10, now_secs - 30 * 86400),
        ];
        let scored = compute(&stats, now, 7.0 * 86400.0, 0.8);
        assert_eq!(scored[0].0, "recent");
    }

    #[test]
    fn fail_rate_lowers_score() {
        let now = SystemTime::now();
        let now_secs = now
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let good = stat("good", 10, now_secs);
        let mut bad = stat("bad", 10, now_secs);
        bad.fail_count = 8;
        let scored = compute(&[good, bad], now, 7.0 * 86400.0, 0.8);
        assert_eq!(scored[0].0, "good");
        assert!(scored[0].1 > scored[1].1);
    }

    #[test]
    fn accept_rate_boosts_score() {
        let now = SystemTime::now();
        let now_secs = now
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let mut low_accept = stat("low", 10, now_secs);
        low_accept.accept_count = 1;
        low_accept.reject_count = 9;
        let mut high_accept = stat("high", 10, now_secs);
        high_accept.accept_count = 9;
        high_accept.reject_count = 1;
        let scored = compute(&[low_accept, high_accept], now, 7.0 * 86400.0, 0.8);
        assert_eq!(scored[0].0, "high");
    }

    #[test]
    fn fail_count_exceeds_count_does_not_zero() {
        // fail_count 来自 signals（不去重），count 来自 history（HIST_IGNORE_DUPS 去重），
        // 实际可能出现 fail_count > count。clamp 到 1.0 后 score 应 > 0，
        // 且不超过「同 count 但 fail_count == count」（全失败率）的下界。
        let now = SystemTime::now();
        let now_secs = now
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let mut over = stat("over", 1, now_secs);
        over.fail_count = 5; // fail_count > count
        let mut full = stat("full", 1, now_secs);
        full.fail_count = 1; // fail_count == count（全失败率）
        let scored = compute(&[over, full], now, 7.0 * 86400.0, 0.8);
        let score_over = scored.iter().find(|(c, _)| c == "over").unwrap().1;
        let score_full = scored.iter().find(|(c, _)| c == "full").unwrap().1;
        assert!(score_over > 0.0, "fail_count>count 不应被钳为 0");
        assert!(
            (score_over - score_full).abs() < 1e-9,
            "fail_count>count 经 clamp 后应等同全失败率"
        );
    }
}
