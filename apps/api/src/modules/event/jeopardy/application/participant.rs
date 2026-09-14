//! 解析 Jeopardy 操作的个人/战队归属主体。

use anyhow::{Result, anyhow};
use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter};
use uuid::Uuid;

use crate::entity::event_team_members;
use crate::entity::sea_orm_active_enums::ParticipantMode;
use crate::modules::event::jeopardy::application::context::EventContext;
use crate::modules::event::jeopardy::domain::solve::SolveSubject;

/// 已解析的参赛作用域（实例 / 解题 / Writeup 查询共用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedParticipant {
    pub subject: SolveSubject,
    /// Team id when subject is Team.
    pub team_id: Option<Uuid>,
    /// Acting user (always the request user).
    pub user_id: Uuid,
    /// Member count when Team (for concurrency); None for Individual.
    pub team_member_count: Option<u64>,
}

impl ResolvedParticipant {
    pub fn team_id_for_instance(self) -> Option<Uuid> {
        self.team_id
    }
}

/// 由赛事 `participant_mode` 与当前用户成员关系解析归属。
pub async fn resolve_participant(ctx: &EventContext) -> Result<ResolvedParticipant> {
    use crate::modules::event::jeopardy::domain::policy::JeopardyPolicy;
    let policy = JeopardyPolicy::from_event(&ctx.event).map_err(|e| anyhow!(e))?;
    let user_id = ctx.user.id;
    debug_assert_eq!(
        policy.is_individual(),
        matches!(ctx.event.participant_mode, ParticipantMode::Individual)
    );
    match ctx.event.participant_mode {
        ParticipantMode::Individual => Ok(ResolvedParticipant {
            subject: SolveSubject::User,
            team_id: None,
            user_id,
            team_member_count: None,
        }),
        ParticipantMode::Team => {
            let db = ctx.db.get_ref();
            let membership = event_team_members::Entity::find()
                .filter(event_team_members::Column::EventId.eq(ctx.event.id))
                .filter(event_team_members::Column::UserId.eq(user_id))
                .one(db)
                .await?
                .ok_or_else(|| anyhow!("you are not in any team"))?;

            let team_member_count = event_team_members::Entity::find()
                .filter(event_team_members::Column::TeamId.eq(membership.team_id))
                .count(db)
                .await?;

            Ok(ResolvedParticipant {
                subject: SolveSubject::Team,
                team_id: Some(membership.team_id),
                user_id,
                team_member_count: Some(team_member_count),
            })
        }
    }
}

/// 无 `EventContext` 时解析战队成员关系（解题状态等辅助路径）。
pub async fn resolve_team_id_for_user(
    db: &sea_orm::DatabaseConnection,
    event_id: Uuid,
    user_id: Uuid,
) -> Result<Uuid> {
    event_team_members::Entity::find()
        .filter(event_team_members::Column::EventId.eq(event_id))
        .filter(event_team_members::Column::UserId.eq(user_id))
        .one(db)
        .await?
        .map(|m| m.team_id)
        .ok_or_else(|| anyhow!("you are not in any team"))
}
