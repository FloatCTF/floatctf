//! AWD 赛事时间模型（Wave 1 基础层）。
//!
//! 提供 `event_duration` / `attack_duration` / `hardening_duration`
//! 的权威计算，不依赖运行时状态。
//!
//! # 来源
//!
//! - `event_duration` = `events.end_time - events.start_time`（通用 Event 调度）
//! - `attack_duration` = `awd_events.round_count × awd_events.round_duration_secs`
//! - `hardening_duration` = `event_duration - attack_duration`
//!
//! 不持久化 `event_duration` 或衍生值到 AWD 专用列。

use chrono::{DateTime, Duration, FixedOffset};

/// 计算后的 AWD 赛事时间模型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AwdTiming {
    /// 总赛事时长（秒），来自 `events.end_time - events.start_time`。
    pub event_duration_secs: i64,
    /// Attack 阶段总时长（秒）= `round_count × round_duration_secs`。
    pub attack_duration_secs: i64,
    /// Hardening 阶段时长（秒）= `event_duration_secs - attack_duration_secs`。
    /// 可以为 0。
    pub hardening_duration_secs: i64,
    /// 轮次数。
    pub round_count: i32,
    /// 单轮时长（秒）。
    pub round_duration_secs: i32,
}

/// 时间模型校验错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimingValidationError {
    /// `events.end_time` 缺失。
    MissingEndTime,
    /// `events.end_time <= events.start_time`。
    EndTimeNotAfterStart,
    /// `round_count` 未配置（NULL）。
    RoundCountNotConfigured,
    /// `round_count <= 0`。
    RoundCountNotPositive,
    /// `round_duration_secs <= 0`。
    RoundDurationNotPositive,
    /// `round_count × round_duration_secs > event_duration_secs`。
    AttackExceedsEvent,
    /// 乘法溢出。
    ArithmeticOverflow,
}

impl std::fmt::Display for TimingValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingEndTime => write!(f, "event end_time is required"),
            Self::EndTimeNotAfterStart => write!(f, "event end_time must be after start_time"),
            Self::RoundCountNotConfigured => write!(f, "round_count is not configured"),
            Self::RoundCountNotPositive => write!(f, "round_count must be > 0"),
            Self::RoundDurationNotPositive => write!(f, "round_duration_secs must be > 0"),
            Self::AttackExceedsEvent => write!(
                f,
                "round_count × round_duration_secs exceeds event duration"
            ),
            Self::ArithmeticOverflow => write!(f, "arithmetic overflow in timing calculation"),
        }
    }
}

/// 从通用 Event 调度与 AWD 配置计算 AWD 时间模型。
///
/// # Errors
///
/// 返回 `TimingValidationError` 若输入不合法。
pub fn compute_timing(
    event_start: DateTime<FixedOffset>,
    event_end: Option<DateTime<FixedOffset>>,
    round_count: Option<i32>,
    round_duration_secs: i32,
) -> Result<AwdTiming, TimingValidationError> {
    // ── event_duration ──
    let end = event_end.ok_or(TimingValidationError::MissingEndTime)?;
    if end <= event_start {
        return Err(TimingValidationError::EndTimeNotAfterStart);
    }
    let event_duration = end - event_start;
    let event_duration_secs = event_duration.num_seconds();
    if event_duration_secs < 0 {
        return Err(TimingValidationError::EndTimeNotAfterStart);
    }

    // ── round_count ──
    let rc = round_count.ok_or(TimingValidationError::RoundCountNotConfigured)?;
    if rc <= 0 {
        return Err(TimingValidationError::RoundCountNotPositive);
    }

    // ── round_duration ──
    if round_duration_secs <= 0 {
        return Err(TimingValidationError::RoundDurationNotPositive);
    }

    // ── attack_duration ──
    let attack_duration_secs = (rc as i64)
        .checked_mul(round_duration_secs as i64)
        .ok_or(TimingValidationError::ArithmeticOverflow)?;

    if attack_duration_secs > event_duration_secs {
        return Err(TimingValidationError::AttackExceedsEvent);
    }

    // ── hardening_duration ──
    let hardening_duration_secs = event_duration_secs - attack_duration_secs;

    Ok(AwdTiming {
        event_duration_secs,
        attack_duration_secs,
        hardening_duration_secs,
        round_count: rc,
        round_duration_secs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn ts(s: &str) -> DateTime<FixedOffset> {
        chrono::Utc
            .with_ymd_and_hms(2026, 8, 1, 0, 0, 0)
            .unwrap()
            .fixed_offset()
            + Duration::seconds(s.parse::<i64>().unwrap())
    }

    fn start() -> DateTime<FixedOffset> {
        ts("0")
    }

    fn end(secs: i64) -> DateTime<FixedOffset> {
        ts(&secs.to_string())
    }

    #[test]
    fn valid_with_hardening() {
        let t = compute_timing(start(), Some(end(3600)), Some(10), 300).unwrap();
        assert_eq!(t.event_duration_secs, 3600);
        assert_eq!(t.attack_duration_secs, 3000);
        assert_eq!(t.hardening_duration_secs, 600);
        assert_eq!(t.round_count, 10);
        assert_eq!(t.round_duration_secs, 300);
    }

    #[test]
    fn hardening_zero() {
        let t = compute_timing(start(), Some(end(3000)), Some(10), 300).unwrap();
        assert_eq!(t.hardening_duration_secs, 0);
    }

    #[test]
    fn attack_exceeds_event() {
        let err = compute_timing(start(), Some(end(2999)), Some(10), 300).unwrap_err();
        assert_eq!(err, TimingValidationError::AttackExceedsEvent);
    }

    #[test]
    fn round_count_missing() {
        let err = compute_timing(start(), Some(end(3600)), None, 300).unwrap_err();
        assert_eq!(err, TimingValidationError::RoundCountNotConfigured);
    }

    #[test]
    fn round_count_zero() {
        let err = compute_timing(start(), Some(end(3600)), Some(0), 300).unwrap_err();
        assert_eq!(err, TimingValidationError::RoundCountNotPositive);
    }

    #[test]
    fn round_duration_zero() {
        let err = compute_timing(start(), Some(end(3600)), Some(10), 0).unwrap_err();
        assert_eq!(err, TimingValidationError::RoundDurationNotPositive);
    }

    #[test]
    fn missing_end_time() {
        let err = compute_timing(start(), None, Some(10), 300).unwrap_err();
        assert_eq!(err, TimingValidationError::MissingEndTime);
    }

    #[test]
    fn end_time_not_after_start() {
        let err = compute_timing(start(), Some(start()), Some(10), 300).unwrap_err();
        assert_eq!(err, TimingValidationError::EndTimeNotAfterStart);
    }

    #[test]
    fn large_values_ok() {
        // Large but valid values
        let t = compute_timing(
            start(),
            Some(end(1_000_000_000i64)),
            Some(1_000_000),
            100,
        )
        .unwrap();
        assert_eq!(t.attack_duration_secs, 100_000_000);
        assert_eq!(t.hardening_duration_secs, 900_000_000);
    }
}