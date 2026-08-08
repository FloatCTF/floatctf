//! Apply phase-specific firewall policy + conntrack flush for an AWD event.

use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use uuid::Uuid;

use crate::entity::{awd_team_networks, sea_orm_active_enums::AwdPhase};
use crate::modules::event::awd_team::system::firewall::RenderedRules;
use crate::modules::event::awd_team::{
    AwdError, AwdResult,
    domain::Ipv4Cidr,
    infrastructure::network::{AwdNetworkRuntime, EventNetworkIdentity, EventNetworkPolicy},
    repo::event_repo,
    system::firewall,
};

/// Pure phase → rules mapping (used by apply path and unit tests).
pub fn render_rules_for_phase(
    event_id: &str,
    phase: AwdPhase,
    gamebox_cidr: &Ipv4Cidr,
    wg_cidr: &Ipv4Cidr,
    flagserver_ip: &str,
    judgeserver_ip: &str,
    // (team_id, gamebox_subnet, wireguard_subnet)
    team_subnets: &[(String, String, String)],
) -> RenderedRules {
    match phase {
        AwdPhase::Hardening => firewall::render_hardening_rules(
            event_id,
            gamebox_cidr,
            wg_cidr,
            flagserver_ip,
            judgeserver_ip,
            team_subnets,
        ),
        AwdPhase::Attack => firewall::render_attack_rules(
            event_id,
            gamebox_cidr,
            wg_cidr,
            flagserver_ip,
            judgeserver_ip,
        ),
        AwdPhase::Pause => firewall::render_pause_rules(event_id, gamebox_cidr),
    }
}

/// Collect rendered rules for one event at a given phase.
async fn render_event_rules(
    db: &DatabaseConnection,
    event_id: Uuid,
    phase: AwdPhase,
) -> AwdResult<RenderedRules> {
    let awd_event = event_repo::find_by_event_id(db, event_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound("AWD event not found".into()))?;

    let gamebox_cidr = Ipv4Cidr::parse(&awd_event.gamebox_cidr)
        .map_err(|e| AwdError::Validation(e.to_string()))?;
    let wg_cidr = Ipv4Cidr::parse(&awd_event.wireguard_cidr)
        .map_err(|e| AwdError::Validation(e.to_string()))?;

    let team_nets = awd_team_networks::Entity::find()
        .filter(awd_team_networks::Column::EventId.eq(event_id))
        .all(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;
    let team_subnets: Vec<(String, String, String)> = team_nets
        .into_iter()
        .map(|t| (t.team_id.to_string(), t.gamebox_subnet, t.wireguard_subnet))
        .collect();

    Ok(render_rules_for_phase(
        &event_id.to_string(),
        phase,
        &gamebox_cidr,
        &wg_cidr,
        &awd_event.flagserver_ip,
        &awd_event.judgeserver_ip,
        &team_subnets,
    ))
}

/// Render and apply network policy for `event_id` at `phase`, merging all other
/// active events so a single-event restore never wipes sibling FCTF chains.
pub async fn apply_phase_policy(
    db: &DatabaseConnection,
    network: &dyn AwdNetworkRuntime,
    event_id: Uuid,
    phase: AwdPhase,
) -> AwdResult<()> {
    let awd_event = event_repo::find_by_event_id(db, event_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound("AWD event not found".into()))?;

    let mut parts: Vec<RenderedRules> = Vec::new();

    // Primary event at the requested phase (may not yet be reflected in DB).
    parts.push(render_event_rules(db, event_id, phase).await?);

    // Sibling Running/Paused events keep their current phase rules.
    let actives = event_repo::find_active_events(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;
    for other in actives {
        if other.event_id == event_id {
            continue;
        }
        match render_event_rules(db, other.event_id, other.phase.clone()).await {
            Ok(r) => parts.push(r),
            Err(e) => {
                tracing::warn!(
                    event_id = %other.event_id,
                    "skip sibling event rules in multi-event merge: {}",
                    e
                );
            }
        }
    }

    let rules = firewall::merge_rendered_rules(&parts);

    if let Err(e) = network
        .apply_policy(EventNetworkPolicy {
            event_id,
            rules,
            dry_run: false,
        })
        .await
    {
        // On failure, keep previous rules (HostNetworkRuntime rolls back) and surface error.
        return Err(e);
    }

    // Conntrack flush on phase change so established flows cannot bypass new policy.
    // Phase 0 P0-4：不再静默吞错；主策略已成功应用，此处失败显式记录。
    // 完整失败模型（reconcile 失败 → NetworkError Fail Closed）见 Phase 1/3。
    if let Err(e) = network
        .clear_event_connections(EventNetworkIdentity {
            event_id,
            gamebox_cidr: awd_event.gamebox_cidr.clone(),
        })
        .await
    {
        tracing::error!(
            "[NetworkPolicy] conntrack cleanup failed for event {}: {}",
            event_id,
            e
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_rules_differ_for_hardening_and_attack() {
        let gb = Ipv4Cidr::parse("10.0.0.0/16").unwrap();
        let wg = Ipv4Cidr::parse("10.1.0.0/16").unwrap();
        let h = render_rules_for_phase(
            "evt",
            AwdPhase::Hardening,
            &gb,
            &wg,
            "10.0.0.1",
            "10.0.0.2",
            &[],
        );
        let a = render_rules_for_phase(
            "evt",
            AwdPhase::Attack,
            &gb,
            &wg,
            "10.0.0.1",
            "10.0.0.2",
            &[],
        );
        assert_ne!(h.iptables_restore_input, a.iptables_restore_input);
        assert!(
            h.description.to_lowercase().contains("harden")
                || h.iptables_restore_input.contains("FCTF-H-")
        );
        assert!(
            a.iptables_restore_input.contains("ACCEPT") || !a.iptables_restore_input.is_empty()
        );
    }

    #[test]
    fn pause_rules_are_non_empty() {
        let gb = Ipv4Cidr::parse("10.0.0.0/16").unwrap();
        let wg = Ipv4Cidr::parse("10.1.0.0/16").unwrap();
        let p = render_rules_for_phase(
            "evt",
            AwdPhase::Pause,
            &gb,
            &wg,
            "10.0.0.1",
            "10.0.0.2",
            &[],
        );
        assert!(!p.iptables_restore_input.is_empty());
    }
}
