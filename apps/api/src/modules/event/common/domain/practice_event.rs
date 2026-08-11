//! 系统托管 Jeopardy 练习赛事的查询与幂等确保。

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
};

use crate::entity::{
    events,
    sea_orm_active_enums::{EventFamily, EventPurpose, ParticipantMode},
};
use crate::modules::event::common::domain::event_mode::{
    PRACTICE_JEOPARDY_EVENT_ID, PRACTICE_JEOPARDY_SYSTEM_KEY,
};

/// 按 `system_key` 查找系统练习赛事（权威语义查询）。
pub async fn find_practice_jeopardy_event<C: ConnectionTrait>(
    db: &C,
) -> Result<Option<events::Model>, sea_orm::DbErr> {
    events::Entity::find()
        .filter(events::Column::SystemKey.eq(PRACTICE_JEOPARDY_SYSTEM_KEY))
        .one(db)
        .await
}

/// 要求系统练习赛事存在；缺失则返回 `RecordNotFound`。
pub async fn require_practice_jeopardy_event<C: ConnectionTrait>(
    db: &C,
) -> Result<events::Model, sea_orm::DbErr> {
    find_practice_jeopardy_event(db)
        .await?
        .ok_or_else(|| sea_orm::DbErr::RecordNotFound("practice:jeopardy event not found".into()))
}

/// 幂等确保 `practice:jeopardy` 系统赛事存在。
///
/// 首次插入固定使用 [`PRACTICE_JEOPARDY_EVENT_ID`]（定义于 `core::system_ids`，
/// 与调度任务种子同一套固定 UUID 约定）。已存在则原样返回
/// （历史非固定主键由迁移规范到固定值）。
pub async fn ensure_practice_jeopardy_event<C: ConnectionTrait>(
    db: &C,
) -> Result<events::Model, sea_orm::DbErr> {
    if let Some(existing) = find_practice_jeopardy_event(db).await? {
        return Ok(existing);
    }

    let now = Utc::now().fixed_offset();
    let model = events::ActiveModel {
        id: Set(PRACTICE_JEOPARDY_EVENT_ID),
        family: Set(EventFamily::Jeopardy),
        purpose: Set(EventPurpose::Practice),
        participant_mode: Set(ParticipantMode::Individual),
        system_key: Set(Some(PRACTICE_JEOPARDY_SYSTEM_KEY.into())),
        title: Set("JeopardyPractice".into()),
        description: Set(Some("Practice Event".into())),
        hidden: Set(true),
        allow_join: Set(false),
        start_time: Set(now),
        end_time: Set(None),
        rules: Set("do not cheat".into()),
        flag_prefix: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
    };

    match model.insert(db).await {
        Ok(created) => Ok(created),
        Err(err) => {
            // 并发 ensure：`system_key` 唯一或固定主键冲突 → 重新查询
            if let Some(existing) = find_practice_jeopardy_event(db).await? {
                Ok(existing)
            } else {
                Err(err)
            }
        }
    }
}
