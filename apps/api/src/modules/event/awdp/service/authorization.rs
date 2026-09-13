//! AWDP 授权校验（§58：Competition 参与者必须已加入且未被封禁）。

use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use uuid::Uuid;

use crate::modules::event::awdp::{AwdpError, AwdpResult};

/// 校验用户已注册为赛事参与者（event_users 行；join_event 或管理员预注册），
/// 且当前未被赛事管理员封禁。未加入/被封禁均返回 Forbidden。
///
/// AWDP 的 overview 会暴露本人 GameBox 运行端点，动作端点也统一依赖本门禁，
/// 因此 ban 必须在 subject 解析前收口，避免只禁计分、不禁实例控制。
pub async fn require_event_participant(
    db: &DatabaseConnection,
    event_id: Uuid,
    user_id: Uuid,
) -> AwdpResult<()> {
    let participant = crate::entity::event_users::Entity::find()
        .filter(crate::entity::event_users::Column::EventId.eq(event_id))
        .filter(crate::entity::event_users::Column::UserId.eq(user_id))
        .one(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
    let participant =
        participant.ok_or_else(|| AwdpError::Forbidden("you have not joined this event".into()))?;
    if participant.banned {
        return Err(AwdpError::Forbidden(
            "you are banned from this event".into(),
        ));
    }
    Ok(())
}

/// 校验 Team 模式下用户属于本赛事战队且战队未被封禁，返回 team_id。
pub async fn require_event_team_participant(
    db: &DatabaseConnection,
    event_id: Uuid,
    user_id: Uuid,
) -> AwdpResult<Uuid> {
    let membership =
        crate::modules::event::common::infrastructure::event_repository::find_user_team_membership(
            db, event_id, user_id,
        )
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?
        .ok_or_else(|| AwdpError::Forbidden("you are not in any team for this event".into()))?;

    let team = crate::entity::event_teams::Entity::find_by_id(membership.team_id)
        .filter(crate::entity::event_teams::Column::EventId.eq(event_id))
        .one(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?
        .ok_or_else(|| AwdpError::Forbidden("team does not belong to this event".into()))?;
    if team.banned {
        return Err(AwdpError::Forbidden(
            "your team is banned from this event".into(),
        ));
    }
    Ok(team.id)
}
