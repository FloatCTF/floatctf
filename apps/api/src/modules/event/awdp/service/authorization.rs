//! AWDP 授权校验（§58：Competition Individual 必须已加入赛事）。

use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use uuid::Uuid;

use crate::modules::event::awdp::{AwdpError, AwdpResult};

/// 校验用户已注册为赛事参与者（event_users 行；join_event 或管理员预注册）。
/// 未加入 → Forbidden（覆盖 overview / instance start / source / patch / evaluations）。
pub async fn require_event_participant(
    db: &DatabaseConnection,
    event_id: Uuid,
    user_id: Uuid,
) -> AwdpResult<()> {
    let registered = crate::entity::event_users::Entity::find()
        .filter(crate::entity::event_users::Column::EventId.eq(event_id))
        .filter(crate::entity::event_users::Column::UserId.eq(user_id))
        .one(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?
        .is_some();
    if !registered {
        return Err(AwdpError::Forbidden(
            "you have not joined this event".into(),
        ));
    }
    Ok(())
}
