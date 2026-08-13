//! AWDP 练习 Judge 仓储：配置（awdp_practice_judge_settings）+ 结果（awdp_judge_results）。

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
    QueryOrder,
};
use uuid::Uuid;

use crate::entity::{
    awdp_instances, awdp_judge_results, awdp_practice_judge_settings, awdp_runs, event_instances,
    gameboxes,
};
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

/// 刷新最近一次例行检查派发时间（sweep 成功派发后）。
pub async fn touch_last_sweep(db: &DatabaseConnection, event_id: Uuid) -> AwdpResult<()> {
    let row = ensure_settings(db, event_id).await?;
    let mut am: awdp_practice_judge_settings::ActiveModel = row.into();
    am.last_sweep_at = Set(Some(Utc::now().into()));
    am.updated_at = Set(Utc::now().into());
    am.update(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
    Ok(())
}

// ────────────────────────────────────────────────────────────────────────────
// 结果
// ────────────────────────────────────────────────────────────────────────────

/// 插入一条练习 Judge 检查结果（按 callback_id 幂等：重复回调跳过）。
#[allow(clippy::too_many_arguments)]
pub async fn insert_result(
    db: &DatabaseConnection,
    event_id: Uuid,
    run_id: Uuid,
    instance_id: Uuid,
    gamebox_id: Uuid,
    owner_user_id: Option<Uuid>,
    owner_team_id: Option<Uuid>,
    check_kind: &str,
    status: &str,
    detail: Option<&str>,
    callback_id: &str,
) -> AwdpResult<()> {
    let insert = awdp_judge_results::Entity::insert(awdp_judge_results::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        run_id: Set(run_id),
        instance_id: Set(instance_id),
        gamebox_id: Set(gamebox_id),
        owner_user_id: Set(owner_user_id),
        owner_team_id: Set(owner_team_id),
        check_kind: Set(check_kind.to_string()),
        status: Set(status.to_string()),
        detail: Set(detail.map(str::to_string)),
        callback_id: Set(Some(callback_id.to_string())),
        created_at: Set(Utc::now().into()),
    })
    .on_conflict(
        sea_orm::sea_query::OnConflict::column(awdp_judge_results::Column::CallbackId)
            .do_nothing()
            .to_owned(),
    )
    .exec(db)
    .await;
    match insert {
        // 幂等：冲突（重复回调）视为成功跳过。
        Ok(_) => Ok(()),
        Err(sea_orm::DbErr::RecordNotInserted) => Ok(()),
        Err(e) => Err(AwdpError::Database(e.to_string())),
    }
}

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

// ────────────────────────────────────────────────────────────────────────────
// 练习实例扫描（sweep 目标）
// ────────────────────────────────────────────────────────────────────────────

/// 当前全部**练习**运行中实例（practice = awdp_runs.gamebox_id 非空）。
/// 返回 (根实例, awdp 扩展, run, gamebox)。
pub async fn list_running_practice_instances(
    db: &DatabaseConnection,
) -> AwdpResult<
    Vec<(
        event_instances::Model,
        awdp_instances::Model,
        awdp_runs::Model,
        gameboxes::Model,
    )>,
> {
    use sea_orm::QueryOrder;

    // 1. 全部 running 根实例。
    let running = event_instances::Entity::find()
        .filter(event_instances::Column::RuntimeState.eq("running"))
        .order_by_desc(event_instances::Column::UpdatedAt)
        .all(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
    if running.is_empty() {
        return Ok(vec![]);
    }
    let running_ids: Vec<Uuid> = running.iter().map(|r| r.id).collect();

    // 2. awdp 扩展 + run（仅 practice：run.gamebox_id 非空）。
    let awdp_pairs = awdp_instances::Entity::find()
        .filter(awdp_instances::Column::InstanceId.is_in(running_ids.iter().copied()))
        .find_also_related(awdp_runs::Entity)
        .filter(awdp_runs::Column::GameboxId.is_not_null())
        .all(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;

    // 3. 批量拉 gamebox 身份。
    let gamebox_ids: Vec<Uuid> = awdp_pairs.iter().map(|(m, _)| m.gamebox_id).collect();
    let gameboxes = if gamebox_ids.is_empty() {
        vec![]
    } else {
        gameboxes::Entity::find()
            .filter(gameboxes::Column::Id.is_in(gamebox_ids))
            .all(db)
            .await
            .map_err(|e| AwdpError::Database(e.to_string()))?
    };
    let gamebox_map: std::collections::HashMap<Uuid, gameboxes::Model> =
        gameboxes.into_iter().map(|g| (g.id, g.clone())).collect();
    let instance_map: std::collections::HashMap<Uuid, event_instances::Model> =
        running.into_iter().map(|i| (i.id, i)).collect();

    let mut out = Vec::with_capacity(awdp_pairs.len());
    for (ext, run) in awdp_pairs {
        let Some(run) = run else { continue };
        let Some(instance) = instance_map.get(&ext.instance_id).cloned() else {
            continue;
        };
        let Some(gamebox) = gamebox_map.get(&ext.gamebox_id).cloned() else {
            continue;
        };
        out.push((instance, ext, run, gamebox));
    }
    Ok(out)
}
