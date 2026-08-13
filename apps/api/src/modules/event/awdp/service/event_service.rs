//! AWDP run 生命周期：Break → PreparingFix → Fix（crash-safe reconcile，plan §41/§42）。

use bollard::Docker;
use chrono::Utc;
use sea_orm::DatabaseConnection;
use tracing::{info, warn};
use uuid::Uuid;

use crate::entity::sea_orm_active_enums::AwdpPhase;
use crate::modules::event::awdp::{
    AwdpError, AwdpResult,
    domain::AwdpConfig,
    repo::{instance_repo, round_repo, run_repo},
    service::runtime,
};

/// PreparingFix 阶段 reconcile 重试间隔（秒）：reset 失败后 tick 稍后重试。
const PREPARING_RECONCILE_INTERVAL_SECS: i64 = 15;

/// Break → PreparingFix（durable 过渡态）：
///   仅 CAS 阶段迁移 + 设置重试游标（now：下一次 tick 立即 reconcile）；不做任何 Docker 操作（crash 安全）。
///   PreparingFix 期间：source 锁定、patch 禁止、Break flag 禁止（各阶段门禁天然满足）。
pub async fn transition_break_to_preparing_fix(
    db: &DatabaseConnection,
    run_id: Uuid,
) -> AwdpResult<()> {
    let now = Utc::now();
    run_repo::transition_phase(
        db,
        run_id,
        AwdpPhase::Break,
        AwdpPhase::PreparingFix,
        run_repo::PhaseTransitionPatch {
            // now：下一 tick 立即 reconcile（crash 后同样按游标重试）。
            next_action_at: Some(now),
            ..Default::default()
        },
    )
    .await?;
    info!(run_id = %run_id, "AWDP break expired → preparing_fix");
    Ok(())
}

/// PreparingFix reconcile：把所有已启动实例 reset 到 pristine（instance 级 advisory lock，
/// plan §42）；全部成功 → 物化回合时间线 + CAS → Fix（fix 时间戳/游标）。
///
/// 任一 reset 失败 → 留在 PreparingFix（下次 tick 重试；reset 幂等可重放）。
/// crash 半途：下次 tick 从 PreparingFix 分支继续 reconcile。
pub async fn reconcile_preparing_fix(
    db: &DatabaseConnection,
    docker: &Docker,
    jwt_secret: &[u8],
    run_id: Uuid,
) -> AwdpResult<bool> {
    let run = run_repo::require_by_id(db, run_id).await?;
    if run.phase != AwdpPhase::PreparingFix {
        return Ok(false);
    }
    let config = AwdpConfig::from_run(&run);
    config.validate()?;

    // 1. reset 全部已启动实例（instance advisory lock 串行化；失败即返回保持 PreparingFix）。
    let flag_prefix = crate::infrastructure::settings::get_setting(db, "FLAG_PREFIX")
        .await
        .unwrap_or_else(|_| "flag".into());
    let rows = instance_repo::list_for_run(db, run_id).await?;
    let mut reset_ok = true;
    for (instance, _ext) in &rows {
        if instance.runtime_state == "pending" {
            continue; // 未启动 = 未参赛
        }
        match reset_one_locked(db, docker, jwt_secret, instance.id, &flag_prefix).await {
            Ok(_) => {}
            Err(e) => {
                reset_ok = false;
                warn!(
                    run_id = %run_id,
                    instance_id = %instance.id,
                    error = %e,
                    "PreparingFix reconcile: instance reset failed — stay preparing_fix"
                );
                break;
            }
        }
    }
    if !reset_ok {
        // 保持 PreparingFix；bump 重试游标。
        let _ = run_repo::touch_tick_state(
            db,
            run_id,
            run.current_round,
            Utc::now() + chrono::Duration::seconds(PREPARING_RECONCILE_INTERVAL_SECS),
        )
        .await;
        return Ok(false);
    }

    // 2. 全部 pristine → CAS PreparingFix→Fix（fix 时间戳 + 游标）。
    let now = Utc::now();
    let fix_ends = now + chrono::Duration::seconds(config.fix_duration_secs as i64);
    let first_cutoff = now + chrono::Duration::seconds(config.fix_round_interval_secs as i64);
    run_repo::transition_phase(
        db,
        run_id,
        AwdpPhase::PreparingFix,
        AwdpPhase::Fix,
        run_repo::PhaseTransitionPatch {
            fix_started_at: Some(now),
            fix_ends_at: Some(fix_ends),
            current_round: Some(0),
            next_action_at: Some(first_cutoff),
            ..Default::default()
        },
    )
    .await?;
    info!(run_id = %run_id, "AWDP preparing_fix reconcile complete → Fix");

    // 3. 物化回合时间线（幂等；crash 后由 tick Fix 分支兜底 ensure）。
    round_repo::materialize_rounds(db, run_id).await?;
    Ok(true)
}

/// 单实例 reset（instance advisory lock）。
async fn reset_one_locked(
    db: &DatabaseConnection,
    docker: &Docker,
    jwt_secret: &[u8],
    instance_id: Uuid,
    flag_prefix: &str,
) -> AwdpResult<()> {
    let lock =
        crate::modules::event::awdp::service::lock::InstanceAdvisoryLock::acquire(db, instance_id)
            .await?;
    let result =
        runtime::reset_instance_unchecked(db, docker, jwt_secret, instance_id, flag_prefix).await;
    lock.release().await;
    result.map(|_| ())
}

/// 兼容入口（测试/手动跳转）：Break → PreparingFix → 立即 reconcile → Fix。
/// tick 走两段式（crash-safe）；本函数只用于明确的一次性跳转。
pub async fn transition_break_to_fix(
    db: &DatabaseConnection,
    docker: &Docker,
    jwt_secret: &[u8],
    run_id: Uuid,
) -> AwdpResult<()> {
    // 若已在 PreparingFix（崩溃后手动重试）直接 reconcile。
    let run = run_repo::require_by_id(db, run_id).await?;
    if run.phase == AwdpPhase::Break {
        transition_break_to_preparing_fix(db, run_id).await?;
    }
    let ok = reconcile_preparing_fix(db, docker, jwt_secret, run_id).await?;
    if !ok {
        return Err(AwdpError::InvalidState(
            "PreparingFix reconcile 未完成（实例 reset 失败）".into(),
        ));
    }
    Ok(())
}

/// Fix → Break 回退（练习模式手动控制阶段）：撤销 fix 会话（时间戳/回合/计分清零）。
/// 仅练习 run 允许；competition run 受 tick/管理端线性状态机约束。
pub async fn transition_fix_to_break(db: &DatabaseConnection, run_id: Uuid) -> AwdpResult<()> {
    run_repo::transition_fix_to_break(db, run_id).await
}

/// run 下全部已启动实例 reset 到 pristine（管理端 / 玩家 run reset；instance 级锁）。
pub async fn reset_all_run_instances(
    db: &DatabaseConnection,
    docker: &Docker,
    jwt_secret: &[u8],
    run_id: Uuid,
) -> AwdpResult<usize> {
    let rows = instance_repo::list_for_run(db, run_id).await?;
    let flag_prefix = crate::infrastructure::settings::get_setting(db, "FLAG_PREFIX")
        .await
        .unwrap_or_else(|_| "flag".into());
    let mut n = 0usize;
    for (instance, _ext) in rows {
        if instance.runtime_state == "pending" {
            continue;
        }
        reset_one_locked(db, docker, jwt_secret, instance.id, &flag_prefix).await?;
        n += 1;
    }
    Ok(n)
}
