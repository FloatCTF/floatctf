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
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter,
};
use tracing::{error, info};
use uuid::Uuid;

use crate::entity::{
    awd_events, awd_gamebox_instances, awd_gamebox_templates, awd_reset_records, awd_team_networks,
    sea_orm_active_enums::{GameboxStatus, ScoreEventType},
};
use crate::modules::event::awd_team::{
    AwdError, AwdResult,
    domain::{AwdEventStatusExt, GameboxStatusExt},
    repo::{ban_repo, event_repo, gamebox_repo, score_repo},
    service::score_service,
};

/// Reset record created during a reset operation.
pub struct ResetContext {
    pub event_id: Uuid,
    pub instance_id: Uuid,
    pub team_id: Uuid,
    pub requested_by: Uuid,
    pub is_free: bool,
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

    // 2. Verify instance belongs to team
    let instance = gamebox_repo::find_instance_by_id(db, ctx.instance_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound("GameBox instance not found".into()))?;

    if instance.team_id != ctx.team_id {
        return Err(AwdError::Forbidden(
            "This GameBox does not belong to your team".into(),
        ));
    }

    // 3. Check team not banned
    if ban_repo::find_active_ban(db, ctx.event_id, ctx.team_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .is_some()
    {
        return Err(AwdError::Forbidden("Team is banned".into()));
    }

    // 4. Check reset_protection_until — skip judge during protection
    let in_protection = instance
        .reset_protection_until
        .map(|t| t.with_timezone(&chrono::Utc) > chrono::Utc::now())
        .unwrap_or(false);

    // 5. Create reset record
    let reset_record = awd_reset_records::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(ctx.event_id),
        team_id: Set(ctx.team_id),
        gamebox_instance_id: Set(ctx.instance_id),
        requested_by: Set(Some(ctx.requested_by)),
        free_reset: Set(ctx.is_free),
        status: Set("pending".to_string()),
        ..Default::default()
    };
    let reset_id = reset_record
        .insert(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .id;

    // 6. If not free, apply penalty
    if !ctx.is_free {
        let penalty_penalty = awd_event.extra_reset_penalty;
        let idempotency_key = format!("reset:{}", reset_id);
        score_repo::create_score_event(
            db,
            ctx.event_id,
            None,
            ctx.team_id,
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

    // 9. Docker reset: stop/remove + recreate with same IP / name
    let template = awd_gamebox_templates::Entity::find_by_id(instance.template_id)
        .one(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound("GameBox template not found".into()))?;

    let network_name = awd_event
        .docker_network_name
        .clone()
        .unwrap_or_else(|| format!("fctf-awd-{}", &ctx.event_id.to_string()[..8]));

    let password = {
        use crate::modules::event::awd_team::crypto::{AwdCrypto, EncryptedBlob};
        let team_net = awd_team_networks::Entity::find()
            .filter(awd_team_networks::Column::EventId.eq(ctx.event_id))
            .filter(awd_team_networks::Column::TeamId.eq(instance.team_id))
            .one(db)
            .await
            .map_err(|e| AwdError::Database(e.to_string()))?
            .ok_or_else(|| AwdError::NotFound("team network not found".into()))?;
        let crypto =
            AwdCrypto::from_config_secret().map_err(|e| AwdError::Crypto(e.to_string()))?;
        let blob = EncryptedBlob {
            ciphertext: team_net.ssh_password_ciphertext,
            nonce: team_net.ssh_password_nonce,
            key_version: team_net.key_version,
        };
        let aad = AwdCrypto::build_aad(ctx.event_id, "ssh_password");
        let bytes = crypto
            .decrypt(&blob, &aad)
            .map_err(|e| AwdError::Crypto(e.to_string()))?;
        String::from_utf8(bytes).map_err(|e| AwdError::Crypto(e.to_string()))?
    };

    let recreate_spec = fcmc::GameBoxSpec {
        event_id: ctx.event_id,
        team_id: instance.team_id,
        template_id: instance.template_id,
        instance_id: instance.id,
        container_name: instance.container_name.clone(),
        image_ref: template.image_ref.clone(),
        network_name,
        fixed_ip: instance.gamebox_ip.clone(),
        username: template.username.clone(),
        password,
        cpu_millis: template.cpu_millis,
        memory_bytes: template.memory_bytes,
        pids_limit: template.pids_limit,
        healthcheck: None,
        extra_hosts: vec![
            format!("flagserver:{}", awd_event.flagserver_ip),
            format!("judgeserver:{}", awd_event.judgeserver_ip),
        ],
        labels: std::collections::HashMap::new(),
    };

    match containers
        .reset_gamebox(fcmc::GameBoxResetSpec {
            event_id: ctx.event_id,
            team_id: instance.team_id,
            template_id: instance.template_id,
            instance_id: instance.id,
            container_name: instance.container_name.clone(),
            recreate_spec,
        })
        .await
    {
        Ok(handle) => {
            let mut inst: awd_gamebox_instances::ActiveModel = awd_gamebox_instances::ActiveModel {
                id: Set(ctx.instance_id),
                container_id: Set(Some(handle.container_id)),
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

    // 10. Mark reset record as completed
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
        ctx.instance_id, ctx.is_free, reset_id
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
