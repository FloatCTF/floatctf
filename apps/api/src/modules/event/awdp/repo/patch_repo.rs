//! awdp_patch_submissions 仓储（run 作用域）。

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter, QueryOrder,
};
use uuid::Uuid;

use crate::entity::awdp_patch_submissions;
use crate::modules::event::awdp::{AwdpError, AwdpResult, repo::run_repo};

/// 创建 patch 提交（status=applying）。
#[allow(clippy::too_many_arguments)]
pub async fn create_submission(
    db: &DatabaseConnection,
    run_id: Uuid,
    instance_id: Uuid,
    fix_round_id: Uuid,
    user_id: Option<Uuid>,
    team_id: Option<Uuid>,
    script_sha256: &str,
    script_content: &str,
) -> AwdpResult<awdp_patch_submissions::Model> {
    let event_id = run_repo::event_id_for_team_fk(db, run_id).await?;
    let now = Utc::now().into();
    awdp_patch_submissions::ActiveModel {
        id: Set(Uuid::new_v4()),
        run_id: Set(run_id),
        event_id: Set(Some(event_id)),
        instance_id: Set(instance_id),
        fix_round_id: Set(Some(fix_round_id)),
        user_id: Set(user_id),
        team_id: Set(team_id),
        script_sha256: Set(script_sha256.to_string()),
        script_content: Set(script_content.to_string()),
        status: Set("applying".to_string()),
        submitted_at: Set(now),
        ..Default::default()
    }
    .insert(db)
    .await
    .map_err(|e| AwdpError::Database(e.to_string()))
}

/// apply 成功：exit 0 → applied；非 0 → failed。
pub async fn finish_apply(
    db: &DatabaseConnection,
    submission_id: Uuid,
    ok: bool,
    exit_code: Option<i32>,
    stdout_limited: &str,
    stderr_limited: &str,
    error_message: Option<&str>,
) -> AwdpResult<awdp_patch_submissions::Model> {
    let now = Utc::now();
    let mut am: awdp_patch_submissions::ActiveModel =
        awdp_patch_submissions::Entity::find_by_id(submission_id)
            .one(db)
            .await
            .map_err(|e| AwdpError::Database(e.to_string()))?
            .ok_or_else(|| AwdpError::NotFound("patch submission not found".into()))?
            .into();
    am.status = Set(if ok { "applied" } else { "failed" }.to_string());
    am.applied_at = Set(Some(now.into()));
    am.exit_code = Set(exit_code);
    am.stdout_limited = Set(Some(stdout_limited.to_string()));
    am.stderr_limited = Set(Some(stderr_limited.to_string()));
    if let Some(m) = error_message {
        am.error_message = Set(Some(m.to_string()));
    }
    am.update(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))
}

/// 本轮是否已有 APPLIED patch（评估资格）。
pub async fn has_applied_patch(
    db: &DatabaseConnection,
    instance_id: Uuid,
    fix_round_id: Uuid,
) -> AwdpResult<bool> {
    let count = awdp_patch_submissions::Entity::find()
        .filter(awdp_patch_submissions::Column::InstanceId.eq(instance_id))
        .filter(awdp_patch_submissions::Column::FixRoundId.eq(fix_round_id))
        .filter(awdp_patch_submissions::Column::Status.eq("applied"))
        .count(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
    Ok(count > 0)
}

/// 实例最近的 patch 提交（按时间倒序）。
pub async fn latest_for_instance(
    db: &DatabaseConnection,
    instance_id: Uuid,
) -> AwdpResult<Option<awdp_patch_submissions::Model>> {
    awdp_patch_submissions::Entity::find()
        .filter(awdp_patch_submissions::Column::InstanceId.eq(instance_id))
        .order_by_desc(awdp_patch_submissions::Column::SubmittedAt)
        .one(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))
}

/// run 全部 patch 提交（管理端审计）。
pub async fn list_for_run(
    db: &DatabaseConnection,
    run_id: Uuid,
) -> AwdpResult<Vec<awdp_patch_submissions::Model>> {
    awdp_patch_submissions::Entity::find()
        .filter(awdp_patch_submissions::Column::RunId.eq(run_id))
        .order_by_desc(awdp_patch_submissions::Column::SubmittedAt)
        .all(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))
}
