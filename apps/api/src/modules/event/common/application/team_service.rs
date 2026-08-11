//! 战队成员关系服务：集中战队相关操作。
//!
//! 自分散的处理器查询抽出，统一管理端与选手端的战队成员校验。

use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
};
use uuid::Uuid;

use crate::entity::event_team_members;
use crate::modules::event::awd::{AwdError, AwdResult};

/// 查找用户在某赛事中的战队。
pub async fn find_user_team(
    db: &DatabaseConnection,
    event_id: Uuid,
    user_id: Uuid,
) -> AwdResult<Option<Uuid>> {
    let membership = event_team_members::Entity::find()
        .filter(event_team_members::Column::EventId.eq(event_id))
        .filter(event_team_members::Column::UserId.eq(user_id))
        .one(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    Ok(membership.map(|m| m.team_id))
}

/// 要求用户是该赛事某战队的成员。
/// 返回 team_id or error。
pub async fn require_member(
    db: &DatabaseConnection,
    event_id: Uuid,
    user_id: Uuid,
) -> AwdResult<Uuid> {
    find_user_team(db, event_id, user_id)
        .await?
        .ok_or_else(|| AwdError::NotFound("You are not in a team for this event".into()))
}

/// 检查if user is a captain of their team。
pub async fn is_captain(db: &DatabaseConnection, event_id: Uuid, user_id: Uuid) -> AwdResult<bool> {
    let membership = event_team_members::Entity::find()
        .filter(event_team_members::Column::EventId.eq(event_id))
        .filter(event_team_members::Column::UserId.eq(user_id))
        .one(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    Ok(membership
        .map(|m| format!("{:?}", m.role) == "Captain")
        .unwrap_or(false))
}

/// 加入战队（将用户加入战队）。
pub async fn join_team(
    db: &DatabaseConnection,
    event_id: Uuid,
    team_id: Uuid,
    user_id: Uuid,
) -> AwdResult<()> {
    // Check if already in a team
    let existing = find_user_team(db, event_id, user_id).await?;
    if existing.is_some() {
        return Err(AwdError::Conflict(
            "Already in a team for this event".into(),
        ));
    }

    let model = event_team_members::ActiveModel {
        event_id: Set(event_id),
        team_id: Set(team_id),
        user_id: Set(user_id),
        role: Set(crate::entity::sea_orm_active_enums::EventTeamMemberRole::Member),
        ..Default::default()
    };

    model
        .insert(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;
    Ok(())
}

/// 退出战队（移除用户与战队的成员关系）。
pub async fn leave_team(db: &DatabaseConnection, event_id: Uuid, user_id: Uuid) -> AwdResult<()> {
    let membership = event_team_members::Entity::find()
        .filter(event_team_members::Column::EventId.eq(event_id))
        .filter(event_team_members::Column::UserId.eq(user_id))
        .one(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    match membership {
        Some(membership) => {
            event_team_members::Entity::delete_by_id((
                membership.event_id,
                membership.team_id,
                membership.user_id,
            ))
            .exec(db)
            .await
            .map_err(|e| AwdError::Database(e.to_string()))?;
            Ok(())
        }
        None => Err(AwdError::NotFound("Not in a team for this event".into())),
    }
}
