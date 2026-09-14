//! AWDP run 生命周期：Break → PreparingFix → Fix（crash-safe reconcile，plan §41/§42）。

use bollard::Docker;
use chrono::Utc;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use tracing::{info, warn};
use uuid::Uuid;

use crate::entity::{
    event_teams, event_users, events,
    sea_orm_active_enums::{AwdpPhase, ParticipantMode},
};
use crate::modules::event::awdp::{
    AwdpError, AwdpResult,
    domain::AwdpConfig,
    repo::{event_gamebox_repo, instance_repo, round_repo, run_repo},
    service::runtime::{self, Subject},
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

// ────────────────────────────────────────────────────────────────────────────
// 赛事开始自动启动（到时间所有 GameBox 为队伍/用户启动）
// ────────────────────────────────────────────────────────────────────────────

/// 自动启动汇总（日志/可观测）。
#[derive(Debug, Default)]
pub struct AutoStartSummary {
    /// 已启动（含已 running 幂等跳过）。
    pub started: usize,
    /// 启动失败（记录 warn；玩家仍可手动启动）。
    pub failed: usize,
}

/// 枚举赛事参与者主体（Individual → 全部 event_users；Team → 全部 event_teams）。
pub async fn event_subjects(db: &DatabaseConnection, event_id: Uuid) -> AwdpResult<Vec<Subject>> {
    let event = events::Entity::find_by_id(event_id)
        .one(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?
        .ok_or_else(|| AwdpError::NotFound("event not found".into()))?;

    let subjects: Vec<Subject> = match event.participant_mode {
        ParticipantMode::Individual => event_users::Entity::find()
            .filter(event_users::Column::EventId.eq(event_id))
            .all(db)
            .await
            .map_err(|e| AwdpError::Database(e.to_string()))?
            .into_iter()
            .map(|row| Subject::user(row.user_id))
            .collect(),
        ParticipantMode::Team => event_teams::Entity::find()
            .filter(event_teams::Column::EventId.eq(event_id))
            .all(db)
            .await
            .map_err(|e| AwdpError::Database(e.to_string()))?
            .into_iter()
            .map(|row| Subject::team(row.id))
            .collect(),
    };
    Ok(subjects)
}

/// 枚举赛事自动启动目标：(Subject, gamebox_id) 全部组合（DB-only，供测试与启动循环共用）。
///
/// 主体：Individual → 全部 `event_users`；Team → 全部 `event_teams`。
/// GameBox：`awdp_event_gameboxes` 中非 hidden 的全部挂载。
pub async fn auto_start_targets(
    db: &DatabaseConnection,
    run_id: Uuid,
) -> AwdpResult<Vec<(Subject, Uuid)>> {
    let run = run_repo::require_by_id(db, run_id).await?;
    let event_id = run.event_id;
    let subjects = event_subjects(db, event_id).await?;
    if subjects.is_empty() {
        return Ok(Vec::new());
    }

    let gameboxes = event_gamebox_repo::list_for_event(db, event_id).await?;
    let mut out = Vec::new();
    for subject in &subjects {
        for eg in gameboxes.iter().filter(|eg| !eg.hidden) {
            out.push((*subject, eg.gamebox_id));
        }
    }
    Ok(out)
}

/// 赛事开始（Break 进入）时自动为全部参与者启动全部已挂载 GameBox 实例。
///
/// 幂等：`runtime::start_instance` 对已 running 实例直接返回；单实例失败不阻断整体。
///
/// 调用时机：tick `Pending→Break` 成功（自动开赛）与 admin 手动 Start 后。
#[allow(clippy::too_many_arguments)]
pub async fn start_all_event_instances(
    db: &DatabaseConnection,
    docker: &Docker,
    jwt_secret: &[u8],
    awdp_config: &crate::core::config::AwdpStaticConfig,
    run_id: Uuid,
) -> AwdpResult<AutoStartSummary> {
    let run = run_repo::require_by_id(db, run_id).await?;
    // 仅赛事（competition run）自动启动；practice run 由训练模块按需启动。
    if run.phase != AwdpPhase::Break {
        return Ok(AutoStartSummary::default());
    }

    let targets = auto_start_targets(db, run_id).await?;
    if targets.is_empty() {
        return Ok(AutoStartSummary::default());
    }

    let flag_prefix = crate::infrastructure::settings::get_setting(db, "FLAG_PREFIX")
        .await
        .unwrap_or_else(|_| "flag".into());

    let mut summary = AutoStartSummary::default();
    for (subject, gamebox_id) in targets {
        match runtime::start_instance(
            db,
            docker,
            jwt_secret,
            awdp_config,
            run_id,
            gamebox_id,
            subject,
            &flag_prefix,
        )
        .await
        {
            Ok(_) => summary.started += 1,
            Err(e) => {
                summary.failed += 1;
                warn!(
                    run_id = %run_id,
                    gamebox_id = %gamebox_id,
                    error = %e,
                    "AWDP auto-start instance skipped (manual start available)"
                );
            }
        }
    }
    info!(
        run_id = %run_id,
        targets = summary.started + summary.failed,
        started = summary.started,
        failed = summary.failed,
        "AWDP auto-start all event instances"
    );
    Ok(summary)
}

/// 比赛进行中（Break / Fix）新挂载 GameBox 的自动启动（BUG：attach 只写挂载行，
/// 不会为参与者创建实例）。为全部参与者启动该 gamebox 的实例，语义与赛事开始时
/// `start_all_event_instances` 一致；Fix 阶段新容器天然 pristine（从未参与 Break），
/// 无需额外 reset。幂等：已 running 直接返回；单实例失败不阻断整体（玩家仍可手动启动）。
pub async fn start_gamebox_for_active_run(
    db: &DatabaseConnection,
    docker: &Docker,
    jwt_secret: &[u8],
    awdp_config: &crate::core::config::AwdpStaticConfig,
    run_id: Uuid,
    gamebox_id: Uuid,
) -> AwdpResult<AutoStartSummary> {
    let run = run_repo::require_by_id(db, run_id).await?;
    if !matches!(run.phase, AwdpPhase::Break | AwdpPhase::Fix) {
        return Ok(AutoStartSummary::default());
    }
    let subjects = event_subjects(db, run.event_id).await?;
    if subjects.is_empty() {
        return Ok(AutoStartSummary::default());
    }

    let flag_prefix = crate::infrastructure::settings::get_setting(db, "FLAG_PREFIX")
        .await
        .unwrap_or_else(|_| "flag".into());

    let mut summary = AutoStartSummary::default();
    for subject in subjects {
        match runtime::start_instance(
            db,
            docker,
            jwt_secret,
            awdp_config,
            run_id,
            gamebox_id,
            subject,
            &flag_prefix,
        )
        .await
        {
            Ok(_) => summary.started += 1,
            Err(e) => {
                summary.failed += 1;
                warn!(
                    run_id = %run_id,
                    gamebox_id = %gamebox_id,
                    error = %e,
                    "AWDP attach auto-start instance skipped (manual start available)"
                );
            }
        }
    }
    info!(
        run_id = %run_id,
        gamebox_id = %gamebox_id,
        targets = summary.started + summary.failed,
        started = summary.started,
        failed = summary.failed,
        "AWDP auto-start gamebox instances (attached mid-run)"
    );
    Ok(summary)
}
