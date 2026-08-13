//! AWDP 练习 Judge 仓储：配置（awdp_practice_judge_settings）+ 结果（awdp_judge_results）。

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
    QueryOrder,
};
use uuid::Uuid;

use crate::entity::{awdp_judge_results, awdp_practice_judge_settings};
use crate::modules::event::awdp::{AwdpError, AwdpResult};

/// 读取练习 Judge 配置（无行返回 None）。
pub async fn get_settings(
    db: &DatabaseConnection,
    event_id: Uuid,
) -> AwdpResult<Option<awdp_practice_judge_settings::Model>> {
    awdp_practice_judge_settings::Entity::find_by_id(event_id)
        .one(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))
}

/// 幂等 ensure 配置行（不存在则插入默认行；并发唯一冲突 → 重查）。
pub async fn ensure_settings(
    db: &DatabaseConnection,
    event_id: Uuid,
) -> AwdpResult<awdp_practice_judge_settings::Model> {
    if let Some(row) = get_settings(db, event_id).await? {
        return Ok(row);
    }
    let now = Utc::now().into();
    let insert = awdp_practice_judge_settings::ActiveModel {
        event_id: Set(event_id),
        enabled: Set(false),
        judge_server_url: Set(String::new()),
        interval_secs: Set(60),
        flag_path: Set("/flag.php".to_string()),
        container_status: Set("stopped".to_string()),
        container_id: Set(None),
        last_sweep_at: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(db)
    .await;
    match insert {
        Ok(model) => Ok(model),
        Err(e) if is_duplicate_key(&e) => get_settings(db, event_id).await?.ok_or_else(|| {
            AwdpError::Internal("practice judge settings row vanished after conflict".into())
        }),
        Err(e) => Err(AwdpError::Database(e.to_string())),
    }
}

fn is_duplicate_key(e: &sea_orm::DbErr) -> bool {
    let msg = e.to_string().to_lowercase();
    msg.contains("23505") || msg.contains("duplicate")
}

/// 更新练习 Judge 配置（部分字段）。
#[derive(Default)]
pub struct PracticeJudgeSettingsPatch {
    pub enabled: Option<bool>,
    pub judge_server_url: Option<String>,
    pub interval_secs: Option<i32>,
    pub flag_path: Option<String>,
}

/// 应用配置补丁（None 字段保持不变）。
pub async fn update_settings(
    db: &DatabaseConnection,
    event_id: Uuid,
    patch: &PracticeJudgeSettingsPatch,
) -> AwdpResult<awdp_practice_judge_settings::Model> {
    let row = ensure_settings(db, event_id).await?;
    let mut am: awdp_practice_judge_settings::ActiveModel = row.into();
    if let Some(v) = patch.enabled {
        am.enabled = Set(v);
    }
    if let Some(v) = &patch.judge_server_url {
        am.judge_server_url = Set(v.clone());
    }
    if let Some(v) = patch.interval_secs {
        am.interval_secs = Set(v);
    }
    if let Some(v) = &patch.flag_path {
        am.flag_path = Set(v.clone());
    }
    am.updated_at = Set(Utc::now().into());
    am.update(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))
}

/// 记录 JudgeServer 容器状态（deploy/stop 后）。
pub async fn update_container_state(
    db: &DatabaseConnection,
    event_id: Uuid,
    status: &str,
    container_id: Option<&str>,
) -> AwdpResult<()> {
    let row = ensure_settings(db, event_id).await?;
    let mut am: awdp_practice_judge_settings::ActiveModel = row.into();
    am.container_status = Set(status.to_string());
    am.container_id = Set(container_id.map(str::to_string));
    am.updated_at = Set(Utc::now().into());
    am.update(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
    Ok(())
}

// ────────────────────────────────────────────────────────────────────────────
// 结果
// ────────────────────────────────────────────────────────────────────────────

/// 最近检查结果（按 created_at 倒序，limit 条）。
pub async fn list_results(
    db: &DatabaseConnection,
    event_id: Uuid,
    limit: u64,
) -> AwdpResult<Vec<awdp_judge_results::Model>> {
    use sea_orm::QuerySelect;
    awdp_judge_results::Entity::find()
        .filter(awdp_judge_results::Column::EventId.eq(event_id))
        .order_by_desc(awdp_judge_results::Column::CreatedAt)
        .limit(limit)
        .all(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))
}
