//! AWDP Practice Training Ground（plan §8/§9，run 中心化）。
//!
//! - `start_training`：玩家 Start Training → 创建 practice run（phase=Break，默认配置快照）
//!   并同步创建/启动逻辑实例。幂等：同 user+gamebox 已有 active run 直接返回。
//! - 练习 gamebox 统一挂载到系统虚拟赛事 `AWDPlusPractice`（`ensure_mounted`），
//!   操作日志随 run.event_id 落入该事件。
//! - `train_again`：ended run 重新训练 → 创建**新** run（复用 gamebox），
//!   不触碰旧 run 行（历史/分数/rounds 永久保留）。

use bollard::Docker;
use sea_orm::DatabaseConnection;
use uuid::Uuid;

use crate::entity::{awdp_runs, gameboxes};
use crate::modules::event::awdp::{
    AwdpError, AwdpResult,
    domain::AwdpConfig,
    repo::{event_gamebox_repo, run_repo},
    service::runtime::{self, Subject},
};

/// Start Training（幂等）：返回 active practice run（已存在则直接返回）。
pub async fn start_training(
    db: &DatabaseConnection,
    docker: &Docker,
    jwt_secret: &[u8],
    user_id: Uuid,
    gamebox_id: Uuid,
    flag_prefix: &str,
) -> AwdpResult<awdp_runs::Model> {
    // 1. GameBox 必须是可见且完整的 [awdp] capability。
    let gamebox = require_trainable_gamebox(db, gamebox_id).await?;
    let _ = gamebox;

    // 2. 幂等：同 user+gamebox 已有 active run 直接返回。
    if let Some(run) = run_repo::find_active_practice_for(db, gamebox_id, user_id).await? {
        return Ok(run);
    }

    // 3. 创建 run（phase=Break，默认配置快照：started_at/break_ends_at/next_action_at 内部计算）。
    let config = AwdpConfig::default();
    let run = run_repo::create_practice_run(db, gamebox_id, user_id, &config).await?;

    // 3.5 幂等挂载：练习 gamebox 统一挂到 AWDPlusPractice 虚拟赛事（失败不阻断训练）。
    if let Err(e) = event_gamebox_repo::ensure_mounted(db, run.event_id, gamebox_id).await {
        tracing::warn!(
            run_id = %run.id,
            gamebox_id = %gamebox_id,
            error = %e,
            "ensure_mounted practice gamebox skipped"
        );
    }

    // 4. 同步创建逻辑实例并启动。
    let subject = Subject::user(user_id);
    runtime::start_instance(
        db,
        docker,
        jwt_secret,
        run.id,
        gamebox_id,
        subject,
        flag_prefix,
    )
    .await
    .map_err(|e| {
        // 启动失败不回滚 run 行（实例 pending 保留，前端可重试）。
        tracing::warn!(run_id = %run.id, error = %e, "practice instance start failed");
        e
    })?;

    Ok(run)
}

/// Train Again：校验旧 run 属主且 phase=ended → 创建新 run（复用 gamebox），不触碰旧 run。
pub async fn train_again(
    db: &DatabaseConnection,
    docker: &Docker,
    jwt_secret: &[u8],
    user_id: Uuid,
    old_run_id: Uuid,
    flag_prefix: &str,
) -> AwdpResult<awdp_runs::Model> {
    let old = run_repo::require_by_id(db, old_run_id).await?;
    if old.owner_user_id != Some(user_id) {
        return Err(AwdpError::Forbidden("该训练 run 不属于你".into()));
    }
    if old.phase != crate::entity::sea_orm_active_enums::AwdpPhase::Ended {
        return Err(AwdpError::InvalidState(format!(
            "只有 ended 的训练 run 可以重新训练（当前 {:?}）",
            old.phase
        )));
    }
    let gamebox_id = old
        .gamebox_id
        .ok_or_else(|| AwdpError::Internal("practice run 必须携带 gamebox".into()))?;

    // 创建新 run（幂等入口内部会复用 active run —— 旧 run 已 ended，不会冲突）。
    start_training(db, docker, jwt_secret, user_id, gamebox_id, flag_prefix).await
}

/// 校验 GameBox 可用于训练（hidden=false、build=ready、完整 [awdp] capability）。
async fn require_trainable_gamebox(
    db: &DatabaseConnection,
    gamebox_id: Uuid,
) -> AwdpResult<gameboxes::Model> {
    let gamebox = event_gamebox_repo::find_gamebox_identity(db, gamebox_id).await?;
    if gamebox.hidden {
        return Err(AwdpError::NotFound("GameBox not found".into()));
    }
    if gamebox.build_status.as_deref() != Some(crate::modules::gamebox::BUILD_STATUS_READY) {
        return Err(AwdpError::Validation(format!(
            "GameBox {} 未就绪（build_status={:?}）",
            gamebox.id, gamebox.build_status
        )));
    }
    if gamebox.awdp_source_artifact_key.is_none() {
        return Err(AwdpError::Validation(format!(
            "GameBox {} 没有 [awdp] capability（缺少 source.zip 产物）",
            gamebox.safe_name
        )));
    }
    Ok(gamebox)
}
