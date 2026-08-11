//! AWDP 配置值对象：默认值、校验、回合数推导。
//!
//! V1 编辑规则（plan §3）：
//!   - Event 未开始（phase=pending）：可修改全部字段。
//!   - Break/Fix/Ended：全部冻结（配置修改被拒绝）。
//!   - 校验：break/fix/interval > 0；fix % interval == 0（不支持 partial round）。

use crate::modules::event::awdp::{AwdpError, AwdpResult};

pub const DEFAULT_BREAK_DURATION_SECS: i32 = 3600;
pub const DEFAULT_FIX_DURATION_SECS: i32 = 3600;
pub const DEFAULT_FIX_ROUND_INTERVAL_SECS: i32 = 600;
pub const DEFAULT_BREAK_SCORE: i64 = 1000;
pub const DEFAULT_FIX_ROUND_SCORE: i64 = 150;

/// awdp_events 配置值对象（与类型化列一一对应）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AwdpConfig {
    pub break_duration_secs: i32,
    pub fix_duration_secs: i32,
    pub fix_round_interval_secs: i32,
    pub break_score: i64,
    pub fix_round_score: i64,
}

impl Default for AwdpConfig {
    fn default() -> Self {
        Self {
            break_duration_secs: DEFAULT_BREAK_DURATION_SECS,
            fix_duration_secs: DEFAULT_FIX_DURATION_SECS,
            fix_round_interval_secs: DEFAULT_FIX_ROUND_INTERVAL_SECS,
            break_score: DEFAULT_BREAK_SCORE,
            fix_round_score: DEFAULT_FIX_ROUND_SCORE,
        }
    }
}

impl AwdpConfig {
    /// 从 run 快照列重建配置（run 启动后 snapshot 冻结，此后不随 awdp_events 变化）。
    pub fn from_run(run: &crate::entity::awdp_runs::Model) -> Self {
        Self {
            break_duration_secs: run.break_duration_secs,
            fix_duration_secs: run.fix_duration_secs,
            fix_round_interval_secs: run.fix_round_interval_secs,
            break_score: run.break_score,
            fix_round_score: run.fix_round_score,
        }
    }

    /// 校验全部时长/分值约束。
    pub fn validate(&self) -> AwdpResult<()> {
        if self.break_duration_secs <= 0 {
            return Err(AwdpError::Validation(
                "break_duration_secs must be > 0".into(),
            ));
        }
        if self.fix_duration_secs <= 0 {
            return Err(AwdpError::Validation(
                "fix_duration_secs must be > 0".into(),
            ));
        }
        if self.fix_round_interval_secs <= 0 {
            return Err(AwdpError::Validation(
                "fix_round_interval_secs must be > 0".into(),
            ));
        }
        if self.break_score < 0 || self.fix_round_score < 0 {
            return Err(AwdpError::Validation(
                "break_score and fix_round_score must be >= 0".into(),
            ));
        }
        // V1 强制：fix 时长必须被 interval 整除（无 partial round）。
        if self.fix_duration_secs % self.fix_round_interval_secs != 0 {
            return Err(AwdpError::Validation(format!(
                "fix_duration_secs ({}) must be divisible by fix_round_interval_secs ({})",
                self.fix_duration_secs, self.fix_round_interval_secs
            )));
        }
        Ok(())
    }

    /// 总回合数 = fix_duration / interval（默认 3600/600 = 6）。
    pub fn total_rounds(&self) -> i32 {
        self.fix_duration_secs / self.fix_round_interval_secs
    }
}

/// 配置 PATCH（乐观锁：expected_updated_at 必填）。
#[derive(Debug, Clone, Default)]
pub struct AwdpConfigPatch {
    pub expected_updated_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub break_duration_secs: Option<i32>,
    pub fix_duration_secs: Option<i32>,
    pub fix_round_interval_secs: Option<i32>,
    pub break_score: Option<i64>,
    pub fix_round_score: Option<i64>,
}

impl AwdpConfigPatch {
    /// 逐字段范围校验（组合校验：合并到完整 config 后）。
    pub fn validate(&self) -> AwdpResult<()> {
        for (name, v) in [
            (
                "break_duration_secs",
                self.break_duration_secs.map(|v| v as i64),
            ),
            (
                "fix_duration_secs",
                self.fix_duration_secs.map(|v| v as i64),
            ),
            (
                "fix_round_interval_secs",
                self.fix_round_interval_secs.map(|v| v as i64),
            ),
        ] {
            if let Some(v) = v {
                if v <= 0 {
                    return Err(AwdpError::Validation(format!("{name} must be > 0")));
                }
            }
        }
        for (name, v) in [
            ("break_score", self.break_score),
            ("fix_round_score", self.fix_round_score),
        ] {
            if let Some(v) = v {
                if v < 0 {
                    return Err(AwdpError::Validation(format!("{name} must be >= 0")));
                }
            }
        }
        Ok(())
    }

    /// 应用到现有 config（返回新 config，不校验——调用方负责 validate）。
    pub fn apply_to(&self, base: &AwdpConfig) -> AwdpConfig {
        AwdpConfig {
            break_duration_secs: self.break_duration_secs.unwrap_or(base.break_duration_secs),
            fix_duration_secs: self.fix_duration_secs.unwrap_or(base.fix_duration_secs),
            fix_round_interval_secs: self
                .fix_round_interval_secs
                .unwrap_or(base.fix_round_interval_secs),
            break_score: self.break_score.unwrap_or(base.break_score),
            fix_round_score: self.fix_round_score.unwrap_or(base.fix_round_score),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_plan() {
        let c = AwdpConfig::default();
        c.validate().unwrap();
        assert_eq!(c.break_duration_secs, 3600);
        assert_eq!(c.fix_duration_secs, 3600);
        assert_eq!(c.fix_round_interval_secs, 600);
        assert_eq!(c.break_score, 1000);
        assert_eq!(c.fix_round_score, 150);
        assert_eq!(c.total_rounds(), 6);
    }

    #[test]
    fn non_divisible_interval_rejected() {
        let c = AwdpConfig {
            fix_duration_secs: 3600,
            fix_round_interval_secs: 700,
            ..Default::default()
        };
        let err = c.validate().unwrap_err();
        assert!(err.to_string().contains("divisible"));
    }

    #[test]
    fn zero_duration_rejected() {
        let c = AwdpConfig {
            break_duration_secs: 0,
            ..Default::default()
        };
        assert!(c.validate().is_err());
    }

    #[test]
    fn patch_applies_partially() {
        let base = AwdpConfig::default();
        let patch = AwdpConfigPatch {
            fix_duration_secs: Some(1200),
            fix_round_interval_secs: Some(600),
            ..Default::default()
        };
        patch.validate().unwrap();
        let next = patch.apply_to(&base);
        assert_eq!(next.fix_duration_secs, 1200);
        assert_eq!(next.break_duration_secs, 3600);
        assert_eq!(next.total_rounds(), 2);
        next.validate().unwrap();
    }

    #[test]
    fn invalid_patch_field_rejected() {
        let patch = AwdpConfigPatch {
            break_duration_secs: Some(-5),
            ..Default::default()
        };
        assert!(patch.validate().is_err());
    }
}
