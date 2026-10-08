use std::{fmt, str::FromStr};

/// 存于 `scheduled_tasks.task_key` 的稳定标识。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TaskKey {
    CheckPracticeEvent,
    CleanInstances,
    CleanRustfs,
    AwdAutoPrecheck,
    AwdEventStart,
    AwdHardeningEnd,
    AwdJudgeBatchDeadline,
    /// ⚠️ **无生产者**：全仓库没有任何代码写出 `awd.round.start` 行 —— 轮次推进是
    /// `round_service::end_round` 在进程内直接调用 `start_round`，不是由调度任务驱动。
    /// 本键 + 对应 handler 只是「可手工触发的入口」；读 `scheduled_tasks` 时不要据此
    /// 推断轮次时序。
    AwdRoundStart,
    AwdRoundEnd,
    /// ⚠️ **无生产者**：归档清理同样没有定时调度（没有代码写出该行），handler 仅作手工
    /// 触发入口。别以为归档有周期性清理。
    AwdArchiveCleanup,
    AwdpTick,
    AwdpEvalWorker,
    AwdpPracticeJudge,
    /// 训练站（floatctf-training）静态制品同步：下载 GitHub Release 制品 → 校验 →
    /// 安全解包 → 原子切换 `current` 软链。可手工触发（管理端「运行」），也可改为 cron。
    PlatformTrainingSync,
}

impl TaskKey {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CheckPracticeEvent => "system.practice.check",
            Self::CleanInstances => "system.practice.clean",
            Self::CleanRustfs => "platform.rustfs.clean",
            Self::AwdAutoPrecheck => "awd.event.auto_precheck",
            Self::AwdEventStart => "awd.event.start",
            Self::AwdHardeningEnd => "awd.event.hardening_end",
            Self::AwdJudgeBatchDeadline => "awd.judge.batch_deadline",
            Self::AwdRoundStart => "awd.round.start",
            Self::AwdRoundEnd => "awd.round.end",
            Self::AwdArchiveCleanup => "awd.archive.cleanup",
            Self::AwdpTick => "awdp.tick",
            Self::AwdpEvalWorker => "awdp.eval.worker",
            Self::AwdpPracticeJudge => "awdp.practice.judge",
            Self::PlatformTrainingSync => "platform.training.sync",
        }
    }
}

impl fmt::Display for TaskKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for TaskKey {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            // 旧的大写形式仅在存量行未迁移时兼容，新任务一律使用小写点分形式。
            "system.practice.check" | "CHECK_PRACTICE_EVENT" | "CHECK_PRATICE_EVENT" => {
                Ok(Self::CheckPracticeEvent)
            }
            "system.practice.clean" | "CLEAN_INSTANCES" => Ok(Self::CleanInstances),
            "platform.rustfs.clean" | "CLEAN_RUSTFS" => Ok(Self::CleanRustfs),
            "awd.event.auto_precheck" => Ok(Self::AwdAutoPrecheck),
            "awd.event.start" => Ok(Self::AwdEventStart),
            "awd.event.hardening_end" => Ok(Self::AwdHardeningEnd),
            "awd.judge.batch_deadline" => Ok(Self::AwdJudgeBatchDeadline),
            "awd.round.start" => Ok(Self::AwdRoundStart),
            "awd.round.end" => Ok(Self::AwdRoundEnd),
            "awd.archive.cleanup" => Ok(Self::AwdArchiveCleanup),
            "awdp.tick" => Ok(Self::AwdpTick),
            "awdp.eval.worker" => Ok(Self::AwdpEvalWorker),
            "awdp.practice.judge" => Ok(Self::AwdpPracticeJudge),
            "platform.training.sync" => Ok(Self::PlatformTrainingSync),
            _ => Err(format!("unknown scheduled task key: {value}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_keys_round_trip_through_database_strings() {
        let keys = [
            TaskKey::CheckPracticeEvent,
            TaskKey::CleanInstances,
            TaskKey::CleanRustfs,
            TaskKey::AwdAutoPrecheck,
            TaskKey::AwdEventStart,
            TaskKey::AwdHardeningEnd,
            TaskKey::AwdJudgeBatchDeadline,
            TaskKey::AwdRoundStart,
            TaskKey::AwdRoundEnd,
            TaskKey::AwdArchiveCleanup,
            TaskKey::AwdpTick,
            TaskKey::AwdpEvalWorker,
            TaskKey::AwdpPracticeJudge,
            TaskKey::PlatformTrainingSync,
        ];

        for key in keys {
            assert_eq!(key.as_str().parse::<TaskKey>(), Ok(key));
        }
    }

    #[test]
    fn unknown_task_key_is_rejected() {
        assert!("awd.unknown".parse::<TaskKey>().is_err());
    }

    #[test]
    fn renamed_platform_keys_parse_and_keep_legacy_aliases() {
        // 新小写点分形式。
        assert_eq!(
            "system.practice.check".parse::<TaskKey>(),
            Ok(TaskKey::CheckPracticeEvent)
        );
        assert_eq!(
            "system.practice.clean".parse::<TaskKey>(),
            Ok(TaskKey::CleanInstances)
        );
        assert_eq!(
            "platform.rustfs.clean".parse::<TaskKey>(),
            Ok(TaskKey::CleanRustfs)
        );
        // 旧大写形式（存量行未迁移时兼容）。
        assert_eq!(
            "CHECK_PRACTICE_EVENT".parse::<TaskKey>(),
            Ok(TaskKey::CheckPracticeEvent)
        );
        assert_eq!(
            "CLEAN_INSTANCES".parse::<TaskKey>(),
            Ok(TaskKey::CleanInstances)
        );
        assert_eq!("CLEAN_RUSTFS".parse::<TaskKey>(), Ok(TaskKey::CleanRustfs));
    }
}
