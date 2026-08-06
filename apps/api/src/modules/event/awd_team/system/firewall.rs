//! Firewall rules engine — renders and applies iptables rules from DB state.
//!
//! # Design principles
//!
//! 1. Database is the single source of truth for network policy.
//! 2. Rules are rendered to iptables-restore format (atomic apply).
//! 3. Current AWD chain state is snapshotted before apply (for rollback).
//! 4. Only manage AWD-specific chains (never overwrite Docker's rules).
//! 5. Chain names use short event hash to respect iptables 28-char limit.

use crate::modules::event::awd_team::{
    AwdError, AwdResult,
    domain::Ipv4Cidr,
    system::command::{CommandRunner, firewall_cmd},
};
use sha2::{Digest, Sha256};

/// Short hash for chain naming (first 8 hex chars of SHA-256).
pub fn short_hash(input: &str) -> String {
    let hash = Sha256::digest(input.as_bytes());
    hex::encode(&hash[..4]) // 8 hex chars
}

/// Chain name conventions for AWD.
///
/// ```text
/// FCTF-AWD              — master chain in DOCKER-USER
/// FCTF-A-{event_short}  — allow rules (per-event)
/// FCTF-X-{event_short}  — deny rules (per-event)
/// FCTF-H-{event_short}  — hardening phase rules
/// FCTF-P-{event_short}  — pause phase rules
/// FCTF-B-{event_short}-{team_short} — ban rules (per-team)
/// ```
pub struct ChainNames {
    pub master: String,
    pub allow: String,
    pub deny: String,
    pub hardening: String,
    pub pause: String,
}

impl ChainNames {
    pub fn for_event(event_id: &str) -> Self {
        let h = short_hash(event_id);
        Self {
            master: "FCTF-AWD".to_string(),
            allow: format!("FCTF-A-{}", h),
            deny: format!("FCTF-X-{}", h),
            hardening: format!("FCTF-H-{}", h),
            pause: format!("FCTF-P-{}", h),
        }
    }

    pub fn ban_chain(event_id: &str, team_id: &str) -> String {
        format!("FCTF-B-{}-{}", short_hash(event_id), short_hash(team_id))
    }
}

/// Rendered iptables rules for a specific event phase.
#[derive(Debug, Clone)]
pub struct RenderedRules {
    pub iptables_restore_input: String,
    pub description: String,
}

/// Render hardening phase rules.
///
/// During hardening:
/// - Players can access their own team's GameBoxes
/// - Players cannot access other teams' GameBoxes
/// - GameBoxes cannot communicate with each other
/// - GameBoxes cannot access WireGuard subnet
/// - GameBoxes cannot access FlagServer/JudgeServer
/// - JudgeServer can reach all GameBoxes
/// - Players cannot access FlagServer/JudgeServer
pub fn render_hardening_rules(
    event_id: &str,
    gamebox_cidr: &Ipv4Cidr,
    _wg_cidr: &Ipv4Cidr,
    flagserver_ip: &str,
    judgeserver_ip: &str,
    // (team_id, gamebox_subnet, wireguard_subnet) — players reach GameBoxes from WG subnet.
    team_subnets: &[(String, String, String)],
) -> RenderedRules {
    let chains = ChainNames::for_event(event_id);
    let mut rules = Vec::new();

    // Create chain
    rules.push(format!(":{} -", chains.hardening));

    // Allow established/related
    rules.push(format!(
        "-A {} -m conntrack --ctstate ESTABLISHED,RELATED -j ACCEPT",
        chains.hardening
    ));

    // Allow JudgeServer → all GameBoxes (health checks)
    rules.push(format!(
        "-A {} -s {} -d {} -j ACCEPT",
        chains.hardening,
        judgeserver_ip,
        gamebox_cidr.to_string()
    ));

    // Allow players (team WireGuard /24) → their own team's GameBox subnet.
    // Ban chain can still DROP for banned teams (jump target).
    for (team_id, gamebox_subnet, wg_subnet) in team_subnets {
        let ban_chain = ChainNames::ban_chain(event_id, team_id);
        rules.push(format!(
            "-A {} -s {} -d {} -j {}",
            chains.hardening, wg_subnet, gamebox_subnet, ban_chain
        ));
    }

    // Block GameBox → other GameBoxes (no lateral movement)
    rules.push(format!(
        "-A {} -s {} -d {} -j DROP",
        chains.hardening,
        gamebox_cidr.to_string(),
        gamebox_cidr.to_string()
    ));

    // Block GameBox → WireGuard subnet
    rules.push(format!(
        "-A {} -s {} -d {} -j DROP",
        chains.hardening,
        gamebox_cidr.to_string(),
        _wg_cidr.to_string()
    ));

    // Block GameBox → FlagServer
    rules.push(format!(
        "-A {} -s {} -d {} -j DROP",
        chains.hardening,
        gamebox_cidr.to_string(),
        flagserver_ip
    ));

    // Block GameBox → JudgeServer
    rules.push(format!(
        "-A {} -s {} -d {} -j DROP",
        chains.hardening,
        gamebox_cidr.to_string(),
        judgeserver_ip
    ));

    // Block players → FlagServer/JudgeServer
    rules.push(format!(
        "-A {} -s 0.0.0.0/0 -d {} -j DROP",
        chains.hardening, flagserver_ip
    ));
    rules.push(format!(
        "-A {} -s 0.0.0.0/0 -d {} -j DROP",
        chains.hardening, judgeserver_ip
    ));

    // Block public internet from GameBoxes
    rules.push(format!(
        "-A {} -s {} -d 0.0.0.0/8 -j DROP",
        chains.hardening,
        gamebox_cidr.to_string()
    ));
    rules.push(format!(
        "-A {} -s {} -d 10.0.0.0/8 -j DROP",
        chains.hardening,
        gamebox_cidr.to_string()
    ));
    rules.push(format!(
        "-A {} -s {} -d 172.16.0.0/12 -j DROP",
        chains.hardening,
        gamebox_cidr.to_string()
    ));
    rules.push(format!(
        "-A {} -s {} -d 192.168.0.0/16 -j DROP",
        chains.hardening,
        gamebox_cidr.to_string()
    ));

    // Return to master chain
    rules.push(format!("-A {} -j RETURN", chains.hardening));

    // Wrap in iptables-restore format
    let output = format!("*filter\n{}\nCOMMIT\n", rules.join("\n"));

    RenderedRules {
        iptables_restore_input: output,
        description: format!("Hardening rules for event {}", short_hash(event_id)),
    }
}

/// Render attack phase rules.
///
/// During attack:
/// - All players can reach all GameBoxes (SSH)
/// - GameBoxes can reach all other GameBoxes
/// - GameBoxes can access FlagServer (for flag requests)
/// - GameBoxes CANNOT access JudgeServer
/// - Players CANNOT access FlagServer/JudgeServer
/// - Cross-team WireGuard isolation still applies
pub fn render_attack_rules(
    event_id: &str,
    gamebox_cidr: &Ipv4Cidr,
    wg_cidr: &Ipv4Cidr,
    flagserver_ip: &str,
    judgeserver_ip: &str,
) -> RenderedRules {
    let chains = ChainNames::for_event(event_id);
    let mut rules = Vec::new();
    let wg = wg_cidr.to_string();
    let gb = gamebox_cidr.to_string();

    rules.push(format!(":{} -", chains.allow));

    // Allow established/related
    rules.push(format!(
        "-A {} -m conntrack --ctstate ESTABLISHED,RELATED -j ACCEPT",
        chains.allow
    ));

    // Allow all GameBox ↔ GameBox
    rules.push(format!("-A {} -s {} -d {} -j ACCEPT", chains.allow, gb, gb));

    // Allow GameBox → FlagServer
    rules.push(format!(
        "-A {} -s {} -d {} -j ACCEPT",
        chains.allow, gb, flagserver_ip
    ));

    // Block GameBox → JudgeServer
    rules.push(format!(
        "-A {} -s {} -d {} -j DROP",
        chains.allow, gb, judgeserver_ip
    ));

    // Players on WireGuard subnet → all GameBoxes (attack phase open)
    rules.push(format!("-A {} -s {} -d {} -j ACCEPT", chains.allow, wg, gb));

    // Block players (WG) → FlagServer/JudgeServer
    rules.push(format!(
        "-A {} -s {} -d {} -j DROP",
        chains.allow, wg, flagserver_ip
    ));
    rules.push(format!(
        "-A {} -s {} -d {} -j DROP",
        chains.allow, wg, judgeserver_ip
    ));

    rules.push(format!("-A {} -j RETURN", chains.allow));

    let output = format!("*filter\n{}\nCOMMIT\n", rules.join("\n"));

    RenderedRules {
        iptables_restore_input: output,
        description: format!("Attack rules for event {}", short_hash(event_id)),
    }
}

/// Render pause phase rules — restrict to WireGuard + SSH only.
pub fn render_pause_rules(event_id: &str, gamebox_cidr: &Ipv4Cidr) -> RenderedRules {
    let chains = ChainNames::for_event(event_id);
    let mut rules = Vec::new();

    rules.push(format!(":{} -", chains.pause));

    // Allow established/related
    rules.push(format!(
        "-A {} -m conntrack --ctstate ESTABLISHED,RELATED -j ACCEPT",
        chains.pause
    ));

    // Block ALL new connections from/to GameBoxes
    rules.push(format!(
        "-A {} -s {} -j DROP",
        chains.pause,
        gamebox_cidr.to_string()
    ));
    rules.push(format!(
        "-A {} -d {} -j DROP",
        chains.pause,
        gamebox_cidr.to_string()
    ));

    rules.push(format!("-A {} -j RETURN", chains.pause));

    let output = format!("*filter\n{}\nCOMMIT\n", rules.join("\n"));

    RenderedRules {
        iptables_restore_input: output,
        description: format!("Pause rules for event {}", short_hash(event_id)),
    }
}

/// Render ban rules for a specific team.
pub fn render_ban_rules(event_id: &str, team_id: &str, team_subnet: &str) -> RenderedRules {
    let ban_chain = ChainNames::ban_chain(event_id, team_id);
    let mut rules = Vec::new();

    rules.push(format!(":{} -", ban_chain));

    // Block all traffic from team's GameBox subnet
    rules.push(format!("-A {} -s {}/24 -j DROP", ban_chain, team_subnet));
    rules.push(format!("-A {} -d {}/24 -j DROP", ban_chain, team_subnet));

    rules.push(format!("-A {} -j RETURN", ban_chain));

    let output = format!("*filter\n{}\nCOMMIT\n", rules.join("\n"));

    RenderedRules {
        iptables_restore_input: output,
        description: format!("Ban rules for team {}", short_hash(team_id)),
    }
}

/// Merge multiple per-event rule renders into a single atomic iptables-restore input.
///
/// Prevents one event's restore from wiping another event's FCTF-* chains.
/// Each input should be a complete `*filter ... COMMIT` blob from `render_*`.
pub fn merge_rendered_rules(parts: &[RenderedRules]) -> RenderedRules {
    if parts.is_empty() {
        return RenderedRules {
            iptables_restore_input: "*filter\nCOMMIT\n".to_string(),
            description: "empty multi-event rules".into(),
        };
    }
    if parts.len() == 1 {
        return parts[0].clone();
    }

    let mut body_lines: Vec<String> = Vec::new();
    let mut descs: Vec<String> = Vec::new();
    for p in parts {
        descs.push(p.description.clone());
        for line in p.iptables_restore_input.lines() {
            let t = line.trim();
            if t.is_empty() || t == "*filter" || t == "COMMIT" {
                continue;
            }
            body_lines.push(line.to_string());
        }
    }
    // Deduplicate identical chain/rule lines while preserving order.
    let mut seen = std::collections::HashSet::new();
    body_lines.retain(|l| seen.insert(l.clone()));

    let output = format!("*filter\n{}\nCOMMIT\n", body_lines.join("\n"));
    RenderedRules {
        iptables_restore_input: output,
        description: format!("merged: {}", descs.join(" | ")),
    }
}

/// Apply rendered rules via iptables-restore with backup/rollback support.
pub async fn apply_rules(runner: &dyn CommandRunner, rules: &RenderedRules) -> AwdResult<()> {
    // 1. Save current snapshot
    let snapshot = firewall_cmd::save_snapshot(runner, "FCTF-")
        .await
        .map_err(|e| AwdError::Network(format!("Failed to snapshot rules: {}", e)))?;

    // 2. Apply new rules
    if let Err(e) = firewall_cmd::apply_rules(runner, &rules.iptables_restore_input).await {
        // 3. Rollback on failure
        tracing::error!("Firewall apply failed, rolling back: {}", e);
        if let Err(rb_err) = firewall_cmd::apply_rules(runner, &snapshot).await {
            tracing::error!("Rollback also failed: {}", rb_err);
            return Err(AwdError::Network(format!(
                "Firewall apply failed ({}) AND rollback failed ({})",
                e, rb_err
            )));
        }
        return Err(AwdError::Network(format!(
            "Firewall apply failed, rolled back: {}",
            e
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_short_hash_deterministic() {
        let h1 = short_hash("test-event-id");
        let h2 = short_hash("test-event-id");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_chain_names_length() {
        let chains = ChainNames::for_event("550e8400-e29b-41d4-a716-446655440000");
        assert!(chains.master.len() <= 28, "master chain too long");
        assert!(chains.allow.len() <= 28, "allow chain too long");
        assert!(chains.deny.len() <= 28, "deny chain too long");
    }

    #[test]
    fn test_render_hardening_has_established() {
        let cidr = Ipv4Cidr::parse("10.0.0.0/16").unwrap();
        let wg = Ipv4Cidr::parse("10.1.0.0/16").unwrap();
        let rules = render_hardening_rules("test", &cidr, &wg, "10.0.0.1", "10.0.0.2", &[]);
        assert!(rules.iptables_restore_input.contains("ESTABLISHED,RELATED"));
    }

    #[test]
    fn test_render_attack_allows_gamebox_to_flagserver() {
        let cidr = Ipv4Cidr::parse("10.0.0.0/16").unwrap();
        let wg = Ipv4Cidr::parse("10.1.0.0/16").unwrap();
        let rules = render_attack_rules("test", &cidr, &wg, "10.0.0.1", "10.0.0.2");
        assert!(rules.iptables_restore_input.contains("10.0.0.1"));
        assert!(rules.iptables_restore_input.contains("ACCEPT"));
    }

    #[test]
    fn test_render_pause_drops_all_gamebox() {
        let cidr = Ipv4Cidr::parse("10.0.0.0/16").unwrap();
        let rules = render_pause_rules("test", &cidr);
        assert!(rules.iptables_restore_input.contains("DROP"));
    }

    #[test]
    fn test_ban_chain_name_format() {
        let ban = ChainNames::ban_chain("event1", "team1");
        assert!(ban.starts_with("FCTF-B-"));
        assert!(ban.len() <= 28);
    }

    #[test]
    fn test_merge_rendered_rules_keeps_both_events() {
        let cidr = Ipv4Cidr::parse("10.0.0.0/16").unwrap();
        let wg = Ipv4Cidr::parse("10.1.0.0/16").unwrap();
        let a = render_pause_rules("event-a", &cidr);
        let b = render_attack_rules("event-b", &cidr, &wg, "10.0.0.1", "10.0.0.2");
        let m = merge_rendered_rules(&[a, b]);
        assert!(m.iptables_restore_input.starts_with("*filter"));
        assert!(m.iptables_restore_input.contains("COMMIT"));
        // Both event chain hashes should appear.
        assert!(m.iptables_restore_input.contains(&short_hash("event-a")));
        assert!(m.iptables_restore_input.contains(&short_hash("event-b")));
        assert_eq!(m.iptables_restore_input.matches("COMMIT").count(), 1);
    }
}
