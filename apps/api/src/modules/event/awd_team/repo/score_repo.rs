//! Score event repository.

use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseConnection,
    EntityTrait, QueryFilter, QueryOrder, QueryTrait, sea_query::OnConflict,
};
use uuid::Uuid;

use crate::entity::{awd_score_events, sea_orm_active_enums::ScoreEventType};

pub async fn create_score_event(
    db: &impl ConnectionTrait,
    event_id: Uuid,
    round_id: Option<Uuid>,
    team_id: Uuid,
    event_type: ScoreEventType,
    delta: i64,
    idempotency_key: &str,
    related_team_id: Option<Uuid>,
    gamebox_instance_id: Option<Uuid>,
    gamebox_template_id: Option<Uuid>,
    reason: Option<&str>,
) -> Result<awd_score_events::Model, sea_orm::DbErr> {
    let model = awd_score_events::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        round_id: Set(round_id),
        team_id: Set(team_id),
        event_type: Set(event_type),
        delta: Set(delta),
        idempotency_key: Set(idempotency_key.to_string()),
        related_team_id: Set(related_team_id),
        gamebox_instance_id: Set(gamebox_instance_id),
        gamebox_template_id: Set(gamebox_template_id),
        reason: Set(reason.map(|s| s.to_string())),
        ..Default::default()
    };

    model.insert(db).await
}

/// 幂等写入：`INSERT ... ON CONFLICT (idempotency_key) DO NOTHING`。
///
/// 返回 `true` 表示本次真正插入（首胜/新记录），`false` 表示键已存在。
/// 与「裸 INSERT + 捕获 duplicate」的区别：ON CONFLICT DO NOTHING **不会**让
/// PostgreSQL 把整个事务置为 aborted（裸 INSERT 的唯一冲突会 abort 事务，
/// 后续 COMMIT 静默变成 ROLLBACK，把同事务内已成功写入的有效记录一起丢掉）。
///
/// 用于事务内「可选写入」场景（如 first-blood bonus：冲突是预期事件而非错误）。
pub async fn create_score_event_if_absent(
    db: &impl ConnectionTrait,
    event_id: Uuid,
    round_id: Option<Uuid>,
    team_id: Uuid,
    event_type: ScoreEventType,
    delta: i64,
    idempotency_key: &str,
    related_team_id: Option<Uuid>,
    gamebox_instance_id: Option<Uuid>,
    gamebox_template_id: Option<Uuid>,
    reason: Option<&str>,
) -> Result<bool, sea_orm::DbErr> {
    let model = awd_score_events::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        round_id: Set(round_id),
        team_id: Set(team_id),
        event_type: Set(event_type),
        delta: Set(delta),
        idempotency_key: Set(idempotency_key.to_string()),
        related_team_id: Set(related_team_id),
        gamebox_instance_id: Set(gamebox_instance_id),
        gamebox_template_id: Set(gamebox_template_id),
        reason: Set(reason.map(|s| s.to_string())),
        ..Default::default()
    };

    // 显式冲突目标列：sea-orm 的 on_conflict_do_nothing() 默认针对主键 (id)，
    // 随机 UUID 永不冲突 → 仍会抛 idempotency_key 重复错误（还会 abort 事务）。
    use sea_orm::TryInsert;
    let stmt = TryInsert::one(model)
        .on_conflict(
            OnConflict::column(awd_score_events::Column::IdempotencyKey)
                .do_nothing()
                .to_owned(),
        )
        .build(db.get_database_backend());
    let res = db.execute(stmt).await?;
    Ok(res.rows_affected() == 1)
}

/// Return the current total score for a team by summing deltas.
pub async fn team_total_score(
    db: &DatabaseConnection,
    event_id: Uuid,
    team_id: Uuid,
) -> Result<i64, sea_orm::DbErr> {
    let scores = awd_score_events::Entity::find()
        .filter(awd_score_events::Column::EventId.eq(event_id))
        .filter(awd_score_events::Column::TeamId.eq(team_id))
        .all(db)
        .await?;
    Ok(scores.iter().map(|s| s.delta).sum())
}

/// Sum deltas for a team filtered by score event types.
pub async fn team_score_for_types(
    db: &DatabaseConnection,
    event_id: Uuid,
    team_id: Uuid,
    types: &[ScoreEventType],
) -> Result<i64, sea_orm::DbErr> {
    if types.is_empty() {
        return Ok(0);
    }
    let scores = awd_score_events::Entity::find()
        .filter(awd_score_events::Column::EventId.eq(event_id))
        .filter(awd_score_events::Column::TeamId.eq(team_id))
        .filter(awd_score_events::Column::EventType.is_in(types.iter().cloned()))
        .all(db)
        .await?;
    Ok(scores.iter().map(|s| s.delta).sum())
}

pub async fn find_score_events_by_team(
    db: &DatabaseConnection,
    event_id: Uuid,
    team_id: Uuid,
) -> Result<Vec<awd_score_events::Model>, sea_orm::DbErr> {
    awd_score_events::Entity::find()
        .filter(awd_score_events::Column::EventId.eq(event_id))
        .filter(awd_score_events::Column::TeamId.eq(team_id))
        .order_by_desc(awd_score_events::Column::CreatedAt)
        .all(db)
        .await
}

pub async fn find_score_event_by_idempotency_key(
    db: &DatabaseConnection,
    key: &str,
) -> Result<Option<awd_score_events::Model>, sea_orm::DbErr> {
    awd_score_events::Entity::find()
        .filter(awd_score_events::Column::IdempotencyKey.eq(key))
        .one(db)
        .await
}
