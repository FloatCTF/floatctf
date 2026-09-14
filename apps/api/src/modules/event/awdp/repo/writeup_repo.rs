//! awdp_run_writeups 仓储（一 run 一份，练习 run 属主可读写）。

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait,
    IntoActiveModel,
};
use uuid::Uuid;

use crate::entity::awdp_run_writeups;
use crate::modules::event::awdp::{AwdpError, AwdpResult};

/// 读取 run 的 Writeup（无记录返回 None）。
pub async fn find_by_run(
    db: &DatabaseConnection,
    run_id: Uuid,
) -> AwdpResult<Option<awdp_run_writeups::Model>> {
    awdp_run_writeups::Entity::find_by_id(run_id)
        .one(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))
}

/// 保存 Writeup（upsert：已存在则更新 content/updated_at，否则插入）。
pub async fn upsert(
    db: &DatabaseConnection,
    run_id: Uuid,
    user_id: Uuid,
    content: String,
) -> AwdpResult<awdp_run_writeups::Model> {
    let now = Utc::now();
    let existing = find_by_run(db, run_id).await?;
    let row = match existing {
        Some(wp) => {
            let mut active = wp.into_active_model();
            active.content = Set(content);
            active.updated_at = Set(now.into());
            active
                .update(db)
                .await
                .map_err(|e| AwdpError::Database(e.to_string()))?
        }
        None => awdp_run_writeups::ActiveModel {
            run_id: Set(run_id),
            user_id: Set(user_id),
            content: Set(content),
            ..Default::default()
        }
        .insert(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?,
    };
    Ok(row)
}

/// 清理（测试用）。
pub async fn delete_for_run(db: &DatabaseConnection, run_id: Uuid) -> AwdpResult<()> {
    awdp_run_writeups::Entity::delete_by_id(run_id)
        .exec(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
    Ok(())
}
