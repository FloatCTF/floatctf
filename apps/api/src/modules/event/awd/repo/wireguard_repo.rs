//! WireGuard 对等体仓储。

use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
};
use uuid::Uuid;

use crate::entity::{awd_wireguard_peers, sea_orm_active_enums::WgPeerStatus};

pub async fn find_peer_by_id(
    db: &DatabaseConnection,
    event_id: Uuid,
    peer_id: Uuid,
) -> Result<Option<awd_wireguard_peers::Model>, sea_orm::DbErr> {
    awd_wireguard_peers::Entity::find()
        .filter(awd_wireguard_peers::Column::Id.eq(peer_id))
        .filter(awd_wireguard_peers::Column::EventId.eq(event_id))
        .one(db)
        .await
}

pub async fn find_peer_by_user(
    db: &DatabaseConnection,
    event_id: Uuid,
    user_id: Uuid,
) -> Result<Option<awd_wireguard_peers::Model>, sea_orm::DbErr> {
    awd_wireguard_peers::Entity::find()
        .filter(awd_wireguard_peers::Column::EventId.eq(event_id))
        .filter(awd_wireguard_peers::Column::UserId.eq(user_id))
        .one(db)
        .await
}

pub async fn find_active_peers_by_event(
    db: &DatabaseConnection,
    event_id: Uuid,
) -> Result<Vec<awd_wireguard_peers::Model>, sea_orm::DbErr> {
    awd_wireguard_peers::Entity::find()
        .filter(awd_wireguard_peers::Column::EventId.eq(event_id))
        .filter(awd_wireguard_peers::Column::Status.is_in([WgPeerStatus::Active]))
        .all(db)
        .await
}

pub async fn find_peers_by_team(
    db: &DatabaseConnection,
    event_id: Uuid,
    team_id: Uuid,
) -> Result<Vec<awd_wireguard_peers::Model>, sea_orm::DbErr> {
    awd_wireguard_peers::Entity::find()
        .filter(awd_wireguard_peers::Column::EventId.eq(event_id))
        .filter(awd_wireguard_peers::Column::TeamId.eq(team_id))
        .all(db)
        .await
}

pub async fn revoke_peer(db: &DatabaseConnection, id: Uuid) -> Result<(), sea_orm::DbErr> {
    let mut active: awd_wireguard_peers::ActiveModel = awd_wireguard_peers::ActiveModel {
        id: Set(id),
        status: Set(WgPeerStatus::Revoked),
        revoked_at: Set(Some(chrono::Utc::now().into())),
        ..Default::default()
    };
    active.update(db).await?;
    Ok(())
}

/// 标记 WG 配置（含私钥）已首次拉取（P1-15 私钥一次返回）。
pub async fn mark_wg_config_fetched(
    db: &DatabaseConnection,
    id: Uuid,
) -> Result<(), sea_orm::DbErr> {
    let mut active: awd_wireguard_peers::ActiveModel = awd_wireguard_peers::ActiveModel {
        id: Set(id),
        config_fetched_at: Set(Some(chrono::Utc::now().into())),
        ..Default::default()
    };
    active.update(db).await?;
    Ok(())
}

/// 轮换中间态：置 Rotating + 记录 rotated_at（host 操作前的 DB 标记，配合 Fail Closed）。
pub async fn mark_peer_rotating(db: &DatabaseConnection, id: Uuid) -> Result<(), sea_orm::DbErr> {
    let mut active: awd_wireguard_peers::ActiveModel = awd_wireguard_peers::ActiveModel {
        id: Set(id),
        status: Set(WgPeerStatus::Rotating),
        rotated_at: Set(Some(chrono::Utc::now().into())),
        ..Default::default()
    };
    active.update(db).await?;
    Ok(())
}

/// 轮换成功落库：置回 Active + 新密钥 + 事件当前 key_version（不再硬编码 1）。
pub async fn rotate_peer(
    db: &DatabaseConnection,
    id: Uuid,
    new_public_key: &str,
    private_key_ciphertext: &[u8],
    private_key_nonce: &[u8],
    key_version: i32,
) -> Result<(), sea_orm::DbErr> {
    let mut active: awd_wireguard_peers::ActiveModel = awd_wireguard_peers::ActiveModel {
        id: Set(id),
        status: Set(WgPeerStatus::Active),
        public_key: Set(new_public_key.to_string()),
        private_key_ciphertext: Set(private_key_ciphertext.to_vec()),
        private_key_nonce: Set(private_key_nonce.to_vec()),
        rotated_at: Set(Some(chrono::Utc::now().into())),
        key_version: Set(key_version),
        ..Default::default()
    };
    active.update(db).await?;
    Ok(())
}
