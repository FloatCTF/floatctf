//! AWD GameBox 重置服务。
//!
//! Wave 5: Removed reset protection window (spec §19.3).
//! Added phase-based eligibility, Pause guard, final settlement guard,
//! and crash-recovery idempotent flow.

use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
    QueryOrder,
};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::entity::{
    awd_reset_records, awd_team_networks, event_gamebox_instances, event_instances,
    sea_orm_active_enums::{AwdPhase, GameboxStatus, ScoreEventType},
};
use crate::modules::event::awd::{
    AwdError, AwdResult,
    domain::{AwdEventStatusExt, AwdPhaseExt, GameboxStatusExt},
    repo::{ban_repo, event_repo, gamebox_repo, round_repo, score_repo},
    service::{gamebox_service, score_service},
};

/// 重置发起方（P4-1 显式化，废弃 admin 传 `Uuid::nil()` hack）。
#[derive(Debug, Clone)]
pub enum ResetActor {
    Player {
        user_id: Uuid,
        team_id: Uuid,
    },
    /// charge_team=true 时该次重置也计入队伍重置次数并可能扣分。
    Admin {
        admin_id: Uuid,
        charge_team: bool,
    },
}

impl ResetActor {
    pub fn requester_id(&self) -> Uuid {
        match self {
            ResetActor::Player { user_id, .. } => *user_id,
            ResetActor::Admin { admin_id, .. } => *admin_id,
        }
    }
}

/// 重置操作中创建的重置记录。
pub struct ResetContext {
    pub event_id: Uuid,
    pub instance_id: Uuid,
    /// 归属队伍（Admin 豁免 ownership 校验，但仍需真实 team_id 记账）。
    pub team_id: Uuid,
    pub actor: ResetActor,
}

/// 检查是否允许 Reset 的规范条件。
///
/// Allowed: Running + (Hardening | Attack) + not Paused + not banned + not final settlement.
/// Forbidden: Paused, Finished, Archived, banned team, final-settlement derived state.
fn check_reset_eligibility(
    awd_event: &crate::entity::awd_events::Model,
    team_id: Uuid,
    has_active_round: bool,
    round_count: Option<i32>,
) -> AwdResult<()> {
    // 1. Event must be Running (not Paused, not Finished, not Archived)
    if !awd_event.status.is_active() {
        return Err(AwdError::Forbidden(
            "Event is not running (must be Running; Paused/Finished/Archived not allowed)".into(),
        ));
    }

    // 2. Pause guard: explicitly reject if Paused
    if awd_event.phase == AwdPhase::Pause {
        return Err(AwdError::Forbidden(
            "Reset is not allowed while the event is Paused".into(),
        ));
    }

    // 3. Phase guard: only Hardening or Attack
    if !matches!(awd_event.phase, AwdPhase::Hardening | AwdPhase::Attack) {
        return Err(AwdError::Forbidden(format!(
            "Reset is not allowed in {:?} phase",
            awd_event.phase
        )));
    }

    // 4. Final settlement guard: Running + Attack + no active round
    //    and final round completed → derived final settlement
    if awd_event.phase == AwdPhase::Attack && !has_active_round {
        if let Some(rc) = round_count {
            if rc > 0 {
                return Err(AwdError::Forbidden(
                    "Reset is not allowed during final settlement".into(),
                ));
            }
        }
    }

    Ok(())
}

/// 执行完整 GameBox 重置工作流。
///
/// Crash recovery: if the instance is already Resetting, attempt to complete the
/// in-flight reset before starting a new one. This makes the flow idempotent across
/// API restarts.
pub async fn execute_reset(
    db: &DatabaseConnection,
    containers: &dyn fcmc::AwdContainerRuntime,
    ctx: ResetContext,
) -> AwdResult<()> {
    // 1. Verify event
    let awd_event = event_repo::find_by_event_id(db, ctx.event_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound("AWD event not found".into()))?;

    // 2. Load instance（先解析真实 team_id，再按 actor 做 ownership 校验）
    // pair：扩展（AWD 领域状态）+ 归一化根（容器实现/代际/名称）。
    let (instance, root) = gamebox_repo::find_instance_by_id(db, ctx.instance_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound("GameBox instance not found".into()))?;
    let team_id = match &ctx.actor {
        // Player：必须 ownership；Admin：豁免（用 instance 真实 team_id 记账）
        ResetActor::Player { team_id, .. } => {
            if *team_id != instance.team_id {
                return Err(AwdError::Forbidden(
                    "This GameBox does not belong to your team".into(),
                ));
            }
            *team_id
        }
        ResetActor::Admin { .. } => instance.team_id,
    };

    // 3. Check team not banned
    if ban_repo::find_active_ban(db, ctx.event_id, team_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .is_some()
    {
        return Err(AwdError::Forbidden("Team is banned".into()));
    }

    // 4. Eligibility check (phase, pause, final settlement)
    let has_active_round = round_repo::find_active_round(db, ctx.event_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .is_some();
    check_reset_eligibility(
        &awd_event,
        team_id,
        has_active_round,
        awd_event.round_count,
    )?;

    // 5. Crash recovery: if instance is already Resetting, attempt recovery
    if instance.status == GameboxStatus::Resetting {
        warn!(
            "Instance {} is already Resetting — attempting recovery",
            ctx.instance_id
        );
        return recover_in_flight_reset(db, containers, &awd_event, &instance, &root, team_id).await;
    }

    // 6. Free reset count
    let used = team_reset_count(db, ctx.event_id, team_id).await?;
    let is_free = match &ctx.actor {
        ResetActor::Player { .. } => used < awd_event.free_reset_count as i64,
        ResetActor::Admin { charge_team, .. } => {
            !charge_team || used < awd_event.free_reset_count as i64
        }
    };

    // 7. Create reset record
    let reset_record = awd_reset_records::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(ctx.event_id),
        team_id: Set(team_id),
        gamebox_instance_id: Set(ctx.instance_id),
        requested_by: Set(Some(ctx.actor.requester_id())),
        free_reset: Set(is_free),
        status: Set("pending".to_string()),
        ..Default::default()
    };
    let reset_id = reset_record
        .insert(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .id;

    // 8. Mark instance as resetting
    gamebox_repo::update_instance_status(db, ctx.instance_id, GameboxStatus::Resetting)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    // 9. Perform the Docker reset
    do_docker_reset(db, containers, &awd_event, &instance, &root, &ctx, reset_id, team_id, is_free, None).await
}

/// Recover an in-flight reset that was interrupted (e.g., API restart).
///
/// States:
/// A. Reset record exists, instance Resetting, old container still there
///    → Resume: stop old container, create new one
/// B. Old container already removed, new container not yet created
///    → Resume: create new container
/// C. New container created, DB still Resetting
///    → Complete the DB update
/// D. Penalty may or may not have been written
///    → Idempotent penalty via reset:{reset_id} key
async fn recover_in_flight_reset(
    db: &DatabaseConnection,
    containers: &dyn fcmc::AwdContainerRuntime,
    awd_event: &crate::entity::awd_events::Model,
    instance: &event_gamebox_instances::Model,
    root: &event_instances::Model,
    team_id: Uuid,
) -> AwdResult<()> {
    // Find the most recent pending reset record
    let pending_record = awd_reset_records::Entity::find()
        .filter(awd_reset_records::Column::GameboxInstanceId.eq(instance.id))
        .filter(awd_reset_records::Column::Status.eq("pending"))
        .order_by_desc(awd_reset_records::Column::CreatedAt)
        .one(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    match pending_record {
        Some(record) => {
            info!(
                "Recovering in-flight reset: reset_id={} instance={}",
                record.id, instance.id
            );
            let ctx = ResetContext {
                event_id: awd_event.event_id,
                instance_id: instance.id,
                team_id,
                actor: ResetActor::Admin {
                    admin_id: Uuid::nil(),
                    charge_team: false,
                },
            };
            do_docker_reset(
                db, containers, awd_event, instance, root,
                &ctx, record.id, team_id, record.free_reset,
                Some(record.id),
            ).await
        }
        None => {
            // No pending record — instance is Resetting but no record found.
            // Treat as a fresh reset (boxed to break async recursion).
            warn!(
                "Instance {} is Resetting but no pending reset record found — starting fresh reset",
                instance.id
            );
            let ctx = ResetContext {
                event_id: awd_event.event_id,
                instance_id: instance.id,
                team_id,
                actor: ResetActor::Admin {
                    admin_id: Uuid::nil(),
                    charge_team: false,
                },
            };
            Box::pin(execute_reset(db, containers, ctx)).await
        }
    }
}

/// Perform the actual Docker stop/remove + recreate + DB update + penalty.
#[allow(clippy::too_many_arguments)]
async fn do_docker_reset(
    db: &DatabaseConnection,
    containers: &dyn fcmc::AwdContainerRuntime,
    awd_event: &crate::entity::awd_events::Model,
    instance: &event_gamebox_instances::Model,
    root: &event_instances::Model,
    ctx: &ResetContext,
    reset_id: Uuid,
    team_id: Uuid,
    is_free: bool,
    _recovery_record_id: Option<Uuid>,
) -> AwdResult<()> {
    // Docker reset: stop/remove + recreate with same IP / credential / pinned revision
    let resolved =
        gamebox_service::resolve_event_gamebox_spec(db, instance.event_gamebox_id).await?;

    let event_network =
        crate::modules::event::awd::repo::event_network_repo::require_by_event_id(db, ctx.event_id)
            .await?;
    let network_name = event_network.docker_network_name.clone();

    let crypto = crate::modules::event::awd::crypto::AwdCrypto::from_config_secret()
        .map_err(|e| AwdError::Crypto(e.to_string()))?;
    let team_net = awd_team_networks::Entity::find()
        .filter(awd_team_networks::Column::EventId.eq(ctx.event_id))
        .filter(awd_team_networks::Column::TeamId.eq(instance.team_id))
        .one(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound("team network not found".into()))?;
    let password =
        gamebox_service::decrypt_team_ssh_password(&crypto, ctx.event_id, &team_net).await?;

    // logical identity 不变（id / event_gamebox_id / team_id / IP / credential），
    // runtime_generation + 1
    let next_generation = root.runtime_generation + 1;
    let recreate_spec = gamebox_service::build_gamebox_runtime_spec(
        &resolved,
        awd_event,
        &event_network,
        instance.id,
        instance.event_gamebox_id,
        instance.team_id,
        &root.container_name,
        &instance.gamebox_ip.ip().to_string(),
        &network_name,
        password,
        next_generation,
    )?;

    match containers
        .reset_gamebox(fcmc::GameBoxResetSpec {
            event_id: ctx.event_id,
            team_id: instance.team_id,
            event_gamebox_id: instance.event_gamebox_id,
            instance_id: instance.id,
            container_name: root.container_name.clone(),
            recreate_spec,
        })
        .await
    {
        Ok(handle) => {
            // 扩展：Ready；归一化根：container_id + generation + running
            let inst: event_gamebox_instances::ActiveModel = event_gamebox_instances::ActiveModel {
                id: Set(ctx.instance_id),
                status: Set(GameboxStatus::Ready),
                ..Default::default()
            };
            inst.update(db)
                .await
                .map_err(|e| AwdError::Database(e.to_string()))?;
            gamebox_repo::update_runtime_root(
                db,
                root.id,
                Some(&handle.container_id),
                "running",
                Some(next_generation),
            )
            .await
            .map_err(|e| AwdError::Database(e.to_string()))?;
        }
        Err(e) => {
            error!("GameBox docker reset failed: {}", e);
            gamebox_repo::update_instance_status(db, ctx.instance_id, GameboxStatus::ResetFailed)
                .await
                .map_err(|e| AwdError::Database(e.to_string()))?;
            let mut reset_active: awd_reset_records::ActiveModel = awd_reset_records::ActiveModel {
                id: Set(reset_id),
                status: Set("failed".to_string()),
                ..Default::default()
            };
            reset_active
                .update(db)
                .await
                .map_err(|e| AwdError::Database(e.to_string()))?;
            return Err(AwdError::Docker(e.to_string()));
        }
    }

    // Penalty: only after successful rebuild, idempotent via reset:{reset_id}
    if !is_free {
        let penalty_penalty = awd_event.extra_reset_penalty;
        let idempotency_key = format!("reset:{}", reset_id);
        score_repo::create_score_event(
            db,
            ctx.event_id,
            None,
            team_id,
            ScoreEventType::ResetPenalty,
            -penalty_penalty,
            &idempotency_key,
            None,
            None,
            None,
            Some("excess reset penalty"),
        )
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;
    }

    // Mark reset record as completed
    let mut reset_active: awd_reset_records::ActiveModel = awd_reset_records::ActiveModel {
        id: Set(reset_id),
        status: Set("completed".to_string()),
        completed_at: Set(Some(chrono::Utc::now().into())),
        ..Default::default()
    };
    reset_active
        .update(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    info!(
        "GameBox reset completed: instance {} (free={}, reset_id={})",
        ctx.instance_id, is_free, reset_id
    );

    Ok(())
}

/// 获取某战队在某赛事中已使用的重置次数。
pub async fn team_reset_count(
    db: &DatabaseConnection,
    event_id: Uuid,
    team_id: Uuid,
) -> AwdResult<i64> {
    use crate::entity::awd_reset_records;
    use sea_orm::{EntityTrait, QueryFilter};

    let results = awd_reset_records::Entity::find()
        .filter(awd_reset_records::Column::EventId.eq(event_id))
        .filter(awd_reset_records::Column::TeamId.eq(team_id))
        .all(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    Ok(results.len() as i64)
}