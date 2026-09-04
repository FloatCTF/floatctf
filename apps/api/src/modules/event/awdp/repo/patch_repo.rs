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
        // apply 开始时间（stale 回收判定）。
        apply_started_at: Set(Some(now)),
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

/// 本轮是否已有 APPLIED patch（评估资格，APPLIED-AT 语义 plan §45）：
/// 只有 applied_at <= round.cutoff_at 的 patch 属于该 Turn。
pub async fn has_applied_patch(
    db: &DatabaseConnection,
    instance_id: Uuid,
    fix_round_id: Uuid,
) -> AwdpResult<bool> {
    use crate::entity::awdp_fix_rounds;
    use chrono::Utc;
    let round = awdp_fix_rounds::Entity::find_by_id(fix_round_id)
        .one(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?
        .ok_or_else(|| AwdpError::NotFound("fix round not found".into()))?;
    let cutoff: chrono::DateTime<Utc> = round.cutoff_at.with_timezone(&Utc);
    let count = awdp_patch_submissions::Entity::find()
        .filter(awdp_patch_submissions::Column::InstanceId.eq(instance_id))
        .filter(awdp_patch_submissions::Column::FixRoundId.eq(fix_round_id))
        .filter(awdp_patch_submissions::Column::Status.eq("applied"))
        .filter(awdp_patch_submissions::Column::AppliedAt.lte(cutoff))
        .count(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
    Ok(count > 0)
}

/// 回收 stale applying（plan §43）：apply_started_at 早于阈值（exec 超时 + 裕量）且仍
/// applying → failed + reason。绝不静默视为 APPLIED（无法证明 Docker mutation 完整）。
pub async fn recover_stale_applying(
    db: &DatabaseConnection,
    instance_id: Uuid,
    stale_before: chrono::DateTime<Utc>,
) -> AwdpResult<usize> {
    use sea_orm::ActiveModelTrait;
    let stale = awdp_patch_submissions::Entity::find()
        .filter(awdp_patch_submissions::Column::InstanceId.eq(instance_id))
        .filter(awdp_patch_submissions::Column::Status.eq("applying"))
        .filter(awdp_patch_submissions::Column::ApplyStartedAt.lt(stale_before))
        .all(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
    let mut n = 0usize;
    for row in stale {
        let mut am: awdp_patch_submissions::ActiveModel = row.into();
        am.status = Set("failed".to_string());
        am.error_message = Set(Some(
            "stale applying recovered（平台崩溃/重启；不能证明容器修改完整，需重新上传）"
                .to_string(),
        ));
        am.update(db)
            .await
            .map_err(|e| AwdpError::Database(e.to_string()))?;
        n += 1;
    }
    Ok(n)
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
