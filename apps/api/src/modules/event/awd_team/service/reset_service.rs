//! GameBox reset workflow — destroy and recreate containers with same IP.
//!
//! Flow:
//! 1. Validate reset permissions and free/penalty status
//! 2. Mark instance as resetting
//! 3. Stop + remove old container (via fcmc)
//! 4. Create new container with same spec (IP, image, password)
//! 5. Wait for healthcheck + SSH ready
//! 6. Mark as ready
//! 7. Apply reset protection period
//!
//! Failed resets: mark as reset_failed, leave container state for admin inspection.

use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
};
use tracing::{error, info};
use uuid::Uuid;

use crate::entity::{
    awd_gamebox_instances, awd_reset_records, awd_team_networks,
    sea_orm_active_enums::{GameboxStatus, ScoreEventType},
};
use crate::modules::event::awd_team::{
    AwdError, AwdResult,
    domain::{AwdEventStatusExt, GameboxStatusExt},
    repo::{ban_repo, event_repo, gamebox_repo, score_repo},
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

/// Reset record created during a reset operation.
pub struct ResetContext {
    pub event_id: Uuid,
    pub instance_id: Uuid,
    /// 归属队伍（Admin 豁免 ownership 校验，但仍需真实 team_id 记账）。
    pub team_id: Uuid,
    pub actor: ResetActor,
}

/// Execute the full GameBox reset workflow.
pub async fn execute_reset(
    db: &DatabaseConnection,
    containers: &dyn fcmc::AwdContainerRuntime,
    ctx: ResetContext,
) -> AwdResult<()> {
    // 1. Verify event is active
    let awd_event = event_repo::find_by_event_id(db, ctx.event_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound("AWD event not found".into()))?;

    if !awd_event.status.is_active() {
        return Err(AwdError::Forbidden("Event is not running".into()));
    }

    // 2. Load instance（先解析真实 team_id，再按 actor 做 ownership 校验）
    let instance = gamebox_repo::find_instance_by_id(db, ctx.instance_id)
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

    // 4. Protection 强制（P4-3）：保护窗口内拒绝（防滥用）
    let in_protection = instance
        .reset_protection_until
        .map(|t| t.with_timezone(&chrono::Utc) > chrono::Utc::now())
        .unwrap_or(false);
    if in_protection {
        return Err(AwdError::Forbidden(
            "Reset is in protection window; wait for it to expire".into(),
        ));
    }

    // 5. 免费次数判定（P4-3）：count < free_reset_count 为免费；其余付费
    let used = team_reset_count(db, ctx.event_id, team_id).await?;
    let is_free = match &ctx.actor {
        ResetActor::Player { .. } => used < awd_event.free_reset_count as i64,
        ResetActor::Admin { charge_team, .. } => {
            !charge_team || used < awd_event.free_reset_count as i64
        }
    };

    // 6. Create reset record
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

    // 7. Mark instance as resetting
    gamebox_repo::update_instance_status(db, ctx.instance_id, GameboxStatus::Resetting)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    // 8. Set reset protection window
    let protection_until =
        chrono::Utc::now() + chrono::Duration::seconds(awd_event.reset_protection_secs as i64);
    let mut active: awd_gamebox_instances::ActiveModel = awd_gamebox_instances::ActiveModel {
        id: Set(ctx.instance_id),
        reset_protection_until: Set(Some(protection_until.into())),
        ..Default::default()
    };
    active
        .update(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    // 9. Docker reset: stop/remove + recreate with same IP / credential / pinned revision
    //
    // §24/§25/§68：Reset 必须使用 Instance → EventGameBox → **pinned** revision，
    // 即使全局 GameBox 已发布 Revision N+1，本赛事仍按 pin 的 Revision 重建。
    let resolved =
        gamebox_service::resolve_event_gamebox_spec(db, instance.event_gamebox_id).await?;

    // Docker 网络逻辑名来自 Event Network（desired）；实际 ID 属 Observed
    let event_network =
        crate::modules::event::awd_team::repo::event_network_repo::require_by_event_id(
            db,
            ctx.event_id,
        )
        .await?;
    let network_name = event_network.docker_network_name.clone();

    let crypto = crate::modules::event::awd_team::crypto::AwdCrypto::from_config_secret()
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
    // runtime_generation + 1（§20/§24）
    let next_generation = instance.runtime_generation + 1;
    let recreate_spec = gamebox_service::build_gamebox_runtime_spec(
        &resolved,
        &awd_event,
        &event_network,
        instance.id,
        instance.event_gamebox_id,
        instance.team_id,
        &instance.container_name,
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
            container_name: instance.container_name.clone(),
            recreate_spec,
        })
        .await
    {
        Ok(handle) => {
            let mut inst: awd_gamebox_instances::ActiveModel = awd_gamebox_instances::ActiveModel {
                id: Set(ctx.instance_id),
                current_container_id: Set(Some(handle.container_id)),
                runtime_generation: Set(next_generation),
                status: Set(GameboxStatus::Ready),
                ..Default::default()
            };
            inst.update(db)
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

    // 10. Penalty 时机（P4-4）：重建成功后才扣分（原实现在重建前扣，
    // 重建失败用户已被扣分却拿不到新容器）
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

    // 11. Mark reset record as completed
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

/// Get the count of resets used by a team for an event.
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
