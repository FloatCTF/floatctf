//! 应用服务共享的提交请求/值类型。

use uuid::Uuid;

/// Jeopardy 解题归属主体，并决定实例作用域。
///
/// 由 [`crate::entity::sea_orm_active_enums::ParticipantMode`] 驱动：
/// - 个人 → [`SolveSubject::User`]
/// - 战队 → [`SolveSubject::Team`]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SolveSubject {
    /// 按用户归属/计分（个人参赛）。
    User,
    /// 按战队归属/计分（战队参赛；操作者用户仍会记入）。
    Team,
}

impl SolveSubject {
    pub fn is_team(self) -> bool {
        matches!(self, Self::Team)
    }
}

/// 正式 Jeopardy Flag 提交入参（竞赛计分路径）。
#[derive(Debug, Clone)]
pub struct JeopardySubmitRequest {
    pub event_id: Uuid,
    pub user_id: Uuid,
    pub instance_id: Uuid,
    pub flag: String,
    pub subject: SolveSubject,
}
