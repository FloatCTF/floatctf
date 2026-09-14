//! awd_network_allocations：平台地址池独占分配账本。
//!
//! released_at 非空 = 已释放（可再次分配给其他 Event，§57）。
//! 释放只允许在 Archive runtime cleanup 成功之后（§56）。

use crate::entity::{awd_network_allocations, sea_orm_active_enums::AwdNetworkAllocationKind};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseConnection,
    EntityTrait, QueryFilter, QueryOrder,
};
use uuid::Uuid;

use super::super::AwdError;

/// 字符串 → PG 原生 CIDR/INET 值（sqlx 以 IpNetwork 读写，§20）。
fn parse_ipnet(v: &str) -> Result<ipnetwork::IpNetwork, AwdError> {
    v.parse()
        .map_err(|e| AwdError::Validation(format!("invalid cidr/ip {v}: {e}")))
}

/// 全部 active（未释放）分配，按种类分组语义由调用方处理。
pub async fn list_active<C: ConnectionTrait + Send>(
    db: &C,
) -> Result<Vec<awd_network_allocations::Model>, AwdError> {
    awd_network_allocations::Entity::find()
        .filter(awd_network_allocations::Column::ReleasedAt.is_null())
        .all(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))
}

/// 某 Event 的全部分配（含已释放）。
pub async fn list_for_event(
    db: &DatabaseConnection,
    event_id: Uuid,
) -> Result<Vec<awd_network_allocations::Model>, AwdError> {
    awd_network_allocations::Entity::find()
        .filter(awd_network_allocations::Column::EventId.eq(event_id))
        .all(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))
}

/// 平台全部分配（observability，§7/§66），active 在前、按分配时间排序。
pub async fn list_all(
    db: &DatabaseConnection,
) -> Result<Vec<awd_network_allocations::Model>, AwdError> {
    awd_network_allocations::Entity::find()
        .order_by_desc(awd_network_allocations::Column::AllocatedAt)
        .all(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))
}

/// 事务内创建一条分配。
pub async fn create_in_tx<C>(
    tx: &C,
    event_id: Uuid,
    kind: AwdNetworkAllocationKind,
    cidr: &str,
) -> Result<awd_network_allocations::Model, AwdError>
where
    C: ConnectionTrait,
{
    awd_network_allocations::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        kind: Set(kind),
        cidr: Set(parse_ipnet(cidr)?),
        allocated_at: Set(chrono::Utc::now().into()),
        released_at: Set(None),
        ..Default::default()
    }
    .insert(tx)
    .await
    .map_err(|e| AwdError::Database(e.to_string()))
}

/// 释放某 Event 的某类分配（事务内，§56：仅在 runtime cleanup 成功后调用）。
pub async fn release_in_tx<C>(
    tx: &C,
    event_id: Uuid,
    kind: AwdNetworkAllocationKind,
) -> Result<(), AwdError>
where
    C: ConnectionTrait,
{
    awd_network_allocations::Entity::update_many()
        .col_expr(
            awd_network_allocations::Column::ReleasedAt,
            chrono::Utc::now().into(),
        )
        .filter(awd_network_allocations::Column::EventId.eq(event_id))
        .filter(awd_network_allocations::Column::Kind.eq(kind))
        .filter(awd_network_allocations::Column::ReleasedAt.is_null())
        .exec(tx)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;
    Ok(())
}

/// 事务内释放某 Event 的全部 active 分配（reallocate 失败回滚时保持旧分配，§33/§93）。
pub async fn release_all_in_tx<C>(tx: &C, event_id: Uuid) -> Result<(), AwdError>
where
    C: ConnectionTrait,
{
    awd_network_allocations::Entity::update_many()
        .col_expr(
            awd_network_allocations::Column::ReleasedAt,
            chrono::Utc::now().into(),
        )
        .filter(awd_network_allocations::Column::EventId.eq(event_id))
        .filter(awd_network_allocations::Column::ReleasedAt.is_null())
        .exec(tx)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;
    Ok(())
}
