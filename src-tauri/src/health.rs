//! Health score: a single 0-100 number summarising how "clean" and alive a
//! project is. Simple, explainable formula (no AI).

use crate::model::Project;

const DAY: i64 = 86_400;

/// Compute the health score for a project given the current unix time.
///
/// Starts at 100 and applies penalties/bonuses:
///   - no git:            -20
///   - no readme:         -10
///   - junk ratio:        up to -30 (proportional to junk / size)
///   - stale activity:    -25 (>180d), -15 (>90d), -5 (>30d)
///   - recent activity:   +5 (<=30d)
pub fn compute(p: &Project, now_secs: i64) -> i32 {
    let mut score: f64 = 100.0;

    if !p.git_present {
        score -= 20.0;
    }
    if !p.has_readme {
        score -= 10.0;
    }

    if p.size_bytes > 0 {
        let ratio = p.junk_bytes as f64 / p.size_bytes as f64;
        score -= (ratio * 30.0).min(30.0);
    }

    if p.last_activity > 0 {
        let age_days = (now_secs - p.last_activity) / DAY;
        if age_days > 180 {
            score -= 25.0;
        } else if age_days > 90 {
            score -= 15.0;
        } else if age_days > 30 {
            score -= 5.0;
        } else {
            score += 5.0;
        }
    } else {
        // Unknown activity is treated as stale.
        score -= 15.0;
    }

    score.round().clamp(0.0, 100.0) as i32
}
