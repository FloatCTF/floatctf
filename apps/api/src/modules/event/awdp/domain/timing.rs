//! AWDP Fix 回合时间线（确定性推导，纯函数）。

use chrono::{DateTime, Duration, Utc};

/// 一个 Fix 回合的时间窗。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoundWindow {
    pub sequence: i32,
    pub starts_at: DateTime<Utc>,
    pub cutoff_at: DateTime<Utc>,
}

/// 总回合数 = fix_duration / interval（plan §3；调用方保证整除）。
pub fn total_rounds(fix_duration_secs: i32, fix_round_interval_secs: i32) -> i32 {
    fix_duration_secs / fix_round_interval_secs
}

/// 预生成全部回合时间窗。
///
/// 约定（plan §20）：
///   - Round 1 起点 = fix_start（第一轮不是 Fix 开始瞬间 check，而是 patch 窗口起点）；
///   - Round N cutoff = fix_start + N * interval；
///   - 最后一轮 cutoff 与 Fix End 对齐。
pub fn round_windows(fix_start: DateTime<Utc>, interval: Duration, count: i32) -> Vec<RoundWindow> {
    (1..=count)
        .map(|i| RoundWindow {
            sequence: i,
            starts_at: fix_start + interval * (i - 1),
            cutoff_at: fix_start + interval * i,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dt(secs: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(1_700_000_000 + secs, 0).unwrap()
    }

    #[test]
    fn rounds_3600_over_600_is_six() {
        assert_eq!(total_rounds(3600, 600), 6);
        assert_eq!(total_rounds(1200, 600), 2);
    }

    #[test]
    fn first_cutoff_and_final_cutoff() {
        let fix_start = dt(0);
        let windows = round_windows(fix_start, Duration::seconds(600), 6);
        assert_eq!(windows.len(), 6);
        // Round 1 起点 = fix_start；cutoff = fix_start + 600。
        assert_eq!(windows[0].sequence, 1);
        assert_eq!(windows[0].starts_at, fix_start);
        assert_eq!(windows[0].cutoff_at, dt(600));
        // 最后一轮 cutoff 与 fix_end 对齐（fix 时长 3600）。
        assert_eq!(windows[5].cutoff_at, dt(3600));
        // 各轮连续。
        for w in windows.windows(2) {
            assert_eq!(w[0].cutoff_at, w[1].starts_at);
        }
    }
}
