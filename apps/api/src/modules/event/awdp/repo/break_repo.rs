//! awdp_breaks 仓储（Break 一次性成功证明）。

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseConnection,
    EntityTrait, PaginatorTrait, QueryFilter,
};
use uuid::Uuid;

use crate::entity::awdp_breaks;
use crate::modules::event::awdp::{AwdpError, AwdpResult};

/// 查询是否已 Break 过（幂等预查）。
pub async fn already_broken(
    db: &DatabaseConnection,
    event_id: Uuid,
    event_gamebox_id: Uuid,
    user_id: Option<Uuid>,
    team_id: Option<Uuid>,
) -> AwdpResult<bool> {
    let mut q = awdp_breaks::Entity::find()
        .filter(awdp_breaks::Column::EventId.eq(event_id))
        .filter(awdp_breaks::Column::EventGameboxId.eq(event_gamebox_id));
    q = match (user_id, team_id) {
        (Some(u), None) => q.filter(awdp_breaks::Column::UserId.eq(u)),
        (None, Some(t)) => q.filter(awdp_breaks::Column::TeamId.eq(t)),
        _ => return Err(AwdpError::Internal("exactly-one owner required".into())),
    };
    let count = q
        .count(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
    Ok(count > 0)
}

/// 记录一次成功 Break（partial unique 兜底并发；冲突按已存在处理）。
pub async fn record_break(
    db: &DatabaseConnection,
    event_id: Uuid,
    event_gamebox_id: Uuid,
    user_id: Option<Uuid>,
    team_id: Option<Uuid>,
    flag_sha256: &str,
) -> AwdpResult<bool> {
    let now = Utc::now().into();
    let res = awdp_breaks::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        event_gamebox_id: Set(event_gamebox_id),
        user_id: Set(user_id),
        team_id: Set(team_id),
        flag_sha256: Set(flag_sha256.to_string()),
        broken_at: Set(now),
    }
    .insert(db)
    .await;
    match res {
        Ok(_) => Ok(true),
        Err(sea_orm::DbErr::Exec(inner))
            if inner.to_string().contains("awdp_breaks_user_uidx")
                || inner.to_string().contains("awdp_breaks_team_uidx") =>
        {
            // 并发重放：已存在视为“首次已记录”。
            Ok(false)
        }
        Err(e) => Err(AwdpError::Database(e.to_string())),
    }
}

/// 事件的全部 Break 记录（计分/展示）。
pub async fn list_for_event(
    db: &DatabaseConnection,
    event_id: Uuid,
) -> AwdpResult<Vec<awdp_breaks::Model>> {
    awdp_breaks::Entity::find()
        .filter(awdp_breaks::Column::EventId.eq(event_id))
        .all(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))
}
