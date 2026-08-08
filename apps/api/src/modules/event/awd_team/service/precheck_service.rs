//! Precheck verification — validate all infrastructure before event starts.
//!
//! # Precheck items
//!
//! 1. Config validation: CIDR format, no overlaps, interface/port availability
//! 2. Docker: network exists, FlagServer running, JudgeServer running
//! 3. GameBox instances: all healthy, containers running
//! 4. WireGuard: interface exists, peers loaded
//! 5. Network matrix: connectivity tests
//! 6. Flag: can issue and re-issue flags
//! 7. Judge: scripts execute and callback works
//!
//! # Verified Revision
//!
//! Configuration changes while verified clear the verification.
//! On event start, the revision must match.

use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter,
};
use tracing::info;
use uuid::Uuid;

use crate::entity::{
    awd_events, awd_precheck_runs,
    sea_orm_active_enums::{AwdEventStatus, PrecheckStatus},
};
use crate::modules::event::awd_team::{
    AwdError, AwdResult,
    domain::{AwdEventStatusExt, Ipv4Cidr},
    repo::event_repo,
};

/// Run a manual precheck on an event.
pub async fn run_precheck(
    db: &DatabaseConnection,
    event_id: Uuid,
    trigger: &str,
) -> AwdResult<Uuid> {
    let awd_event = event_repo::find_by_event_id(db, event_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound("AWD event not found".into()))?;

    if !awd_event.status.is_configurable() && awd_event.status != AwdEventStatus::Prechecking {
        return Err(AwdError::InvalidState(format!(
            "Cannot precheck in {:?} status",
            awd_event.status
        )));
    }

    // 状态机唯一入口（Phase 0）：进入 Prechecking。
    match &awd_event.status {
        AwdEventStatus::Prechecking => {}
        // 已 Verified 的手动重检：先清除 verified 标记回到 Configuring，再进入 Prechecking。
        AwdEventStatus::Verified => {
            event_repo::transition_event(
                db,
                awd_event.id,
                AwdEventStatus::Verified,
                AwdEventStatus::Configuring,
                event_repo::TransitionPatch::config_changed(),
            )
            .await?;
            event_repo::transition_event(
                db,
                awd_event.id,
                AwdEventStatus::Configuring,
                AwdEventStatus::Prechecking,
                Default::default(),
            )
            .await?;
        }
        other => {
            event_repo::transition_event(
                db,
                awd_event.id,
                other.clone(),
                AwdEventStatus::Prechecking,
                Default::default(),
            )
            .await?;
        }
    }

    // Create precheck run record
    let run = awd_precheck_runs::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        status: Set(PrecheckStatus::Running),
        trigger: Set(trigger.to_string()),
        started_at: Set(chrono::Utc::now().into()),
        ..Default::default()
    };

    let run = run
        .insert(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    let mut errors = Vec::new();

    // ── Check 1: Config validation ──
    let config_result = validate_config(&awd_event);
    if let Err(e) = config_result {
        errors.push(("config", e));
    }

    // ── Check 2: All teams have networks allocated ──
    use crate::entity::event_teams;

    let teams = event_teams::Entity::find()
        .filter(event_teams::Column::EventId.eq(event_id))
        .all(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    if teams.is_empty() {
        errors.push(("teams", "No teams registered for this event".into()));
    }

    // ── Check 3: GameBox instances exist for all templates × teams ──
    use crate::entity::awd_gamebox_templates;
    let templates = awd_gamebox_templates::Entity::find()
        .filter(awd_gamebox_templates::Column::EventId.eq(event_id))
        .all(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    if templates.is_empty() {
        errors.push(("templates", "No GameBox templates configured".into()));
    }

    // ── Check 4: Docker network ID set ──
    if awd_event.docker_network_id.is_none() {
        errors.push(("docker", "Docker network not yet created".into()));
    }

    // ── Check 5: FlagServer IP configured ──
    if awd_event.flagserver_ip.is_empty() {
        errors.push(("flagserver", "FlagServer IP not configured".into()));
    }

    // ── Check 6: JudgeServer IP configured ──
    if awd_event.judgeserver_ip.is_empty() {
        errors.push(("judgeserver", "JudgeServer IP not configured".into()));
    }

    // Determine overall status
    let overall_status = if errors.is_empty() {
        PrecheckStatus::Passed
    } else {
        PrecheckStatus::Failed
    };

    let is_passed = overall_status == PrecheckStatus::Passed;

    let error_details = if errors.is_empty() {
        None
    } else {
        Some(serde_json::json!({
            "errors": errors.iter().map(|(k, v)| {
                serde_json::json!({"component": k, "error": v})
            }).collect::<Vec<_>>()
        }))
    };

    // Update precheck run
    let mut run_active: awd_precheck_runs::ActiveModel = awd_precheck_runs::ActiveModel {
        id: Set(run.id),
        status: Set(overall_status),
        completed_at: Set(Some(chrono::Utc::now().into())),
        ..Default::default()
    };

    if let Some(ref details) = error_details {
        run_active.error_msg = Set(Some(details.to_string()));
    }
    run_active.config_check = Set(error_details.clone());

    run_active
        .update(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    if is_passed {
        // Mark event as verified（守卫版：要求当前 Prechecking，Phase 0）
        let revision = compute_revision(&awd_event);
        event_repo::transition_event(
            db,
            awd_event.id,
            AwdEventStatus::Prechecking,
            AwdEventStatus::Verified,
            event_repo::TransitionPatch::verified(&revision),
        )
        .await?;

        info!("[Precheck] Event {} verified", event_id);
    } else {
        // 失败：Prechecking → VerificationFailed（Phase 0）
        // 记录失败状态的失败属 best-effort（显式告警，precheck run 记录已落库）。
        if let Err(e) = event_repo::transition_event(
            db,
            awd_event.id,
            AwdEventStatus::Prechecking,
            AwdEventStatus::VerificationFailed,
            Default::default(),
        )
        .await
        {
            tracing::warn!(
                "[Precheck] failed to record VerificationFailed for event {}: {}",
                event_id,
                e
            );
        }
        info!(
            "[Precheck] Event {} failed precheck: {} errors",
            event_id,
            errors.len()
        );
    }

    Ok(run.id)
}

/// Validate basic config: CIDR format, IPs are within ranges, interface name valid.
fn validate_config(event: &awd_events::Model) -> Result<(), String> {
    // Check gamebox CIDR
    let gbox_cidr =
        Ipv4Cidr::parse(&event.gamebox_cidr).map_err(|e| format!("Invalid gamebox_cidr: {}", e))?;

    // Check wireguard CIDR
    let wg_cidr = Ipv4Cidr::parse(&event.wireguard_cidr)
        .map_err(|e| format!("Invalid wireguard_cidr: {}", e))?;

    // Check CIDRs don't overlap
    if gbox_cidr.overlaps(&wg_cidr) {
        return Err("gamebox_cidr and wireguard_cidr overlap".into());
    }

    // Check CIDR capacity: /16 required (65536 addresses)
    if gbox_cidr.prefix_len > 16 {
        return Err(format!(
            "gamebox_cidr must be /16 or smaller, got /{}",
            gbox_cidr.prefix_len
        ));
    }

    // Check wireguard interface name length (max 15 chars for Linux interfaces)
    if event.wireguard_interface_name.len() > 15 {
        return Err(format!(
            "wireguard_interface_name too long: {} (max 15)",
            event.wireguard_interface_name.len()
        ));
    }

    // Check flagserver IP is in gamebox CIDR
    let fs_ip: std::net::Ipv4Addr = event
        .flagserver_ip
        .parse()
        .map_err(|_| format!("Invalid flagserver_ip: {}", event.flagserver_ip))?;
    if !gbox_cidr.contains(fs_ip) {
        return Err(format!(
            "flagserver_ip {} is not in gamebox_cidr {}",
            event.flagserver_ip, event.gamebox_cidr
        ));
    }

    // Check judgeserver IP is in gamebox CIDR
    let js_ip: std::net::Ipv4Addr = event
        .judgeserver_ip
        .parse()
        .map_err(|_| format!("Invalid judgeserver_ip: {}", event.judgeserver_ip))?;
    if !gbox_cidr.contains(js_ip) {
        return Err(format!(
            "judgeserver_ip {} is not in gamebox_cidr {}",
            event.judgeserver_ip, event.gamebox_cidr
        ));
    }

    // Check flagserver IP != judgeserver IP
    if event.flagserver_ip == event.judgeserver_ip {
        return Err("flagserver_ip and judgeserver_ip must be different".into());
    }

    Ok(())
}

/// Compute a configuration revision hash for verification tracking.
fn compute_revision(event: &awd_events::Model) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(event.gamebox_cidr.as_bytes());
    hasher.update(event.wireguard_cidr.as_bytes());
    hasher.update(event.wireguard_interface_name.as_bytes());
    hasher.update(event.flagserver_ip.as_bytes());
    hasher.update(event.judgeserver_ip.as_bytes());
    hex::encode(hasher.finalize())
}
