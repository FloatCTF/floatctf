use std::{fmt, str::FromStr};

/// 存于 `scheduled_tasks.task_key` 的稳定标识。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TaskKey {
    CheckPracticeEvent,
    CleanInstances,
    CleanRustfs,
    AwdAutoPrecheck,
    AwdEventStart,
    AwdRoundStart,
    AwdRoundEnd,
    AwdRoundGraceEnd,
    AwdArchiveCleanup,
    AwdTeamUnban,
}

impl TaskKey {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CheckPracticeEvent => "CHECK_PRACTICE_EVENT",
            Self::CleanInstances => "CLEAN_INSTANCES",
            Self::CleanRustfs => "CLEAN_RUSTFS",
            Self::AwdAutoPrecheck => "awd.event.auto_precheck",
            Self::AwdEventStart => "awd.event.start",
            Self::AwdRoundStart => "awd.round.start",
            Self::AwdRoundEnd => "awd.round.end",
            Self::AwdRoundGraceEnd => "awd.round.grace_end",
            Self::AwdArchiveCleanup => "awd.archive.cleanup",
            Self::AwdTeamUnban => "awd.team.unban",
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
            "CHECK_PRACTICE_EVENT" | "CHECK_PRATICE_EVENT" => Ok(Self::CheckPracticeEvent),
            "CLEAN_INSTANCES" => Ok(Self::CleanInstances),
            "CLEAN_RUSTFS" => Ok(Self::CleanRustfs),
            "awd.event.auto_precheck" => Ok(Self::AwdAutoPrecheck),
            "awd.event.start" => Ok(Self::AwdEventStart),
            "awd.round.start" => Ok(Self::AwdRoundStart),
            "awd.round.end" => Ok(Self::AwdRoundEnd),
            "awd.round.grace_end" => Ok(Self::AwdRoundGraceEnd),
            "awd.archive.cleanup" => Ok(Self::AwdArchiveCleanup),
            "awd.team.unban" => Ok(Self::AwdTeamUnban),
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
            TaskKey::AwdRoundStart,
            TaskKey::AwdRoundEnd,
            TaskKey::AwdRoundGraceEnd,
            TaskKey::AwdArchiveCleanup,
            TaskKey::AwdTeamUnban,
        ];

        for key in keys {
            assert_eq!(key.as_str().parse::<TaskKey>(), Ok(key));
        }
    }

    #[test]
    fn unknown_task_key_is_rejected() {
        assert!("awd.unknown".parse::<TaskKey>().is_err());
    }
}
