//! Pure nftables ruleset renderer (Phase 1 P1-8).
//!
//! `DesiredFirewallState` → `table inet floatctf_awd` 完整规则文本。
//! 纯函数、无 IO、无特权命令 —— 全部渲染逻辑可单测断言。
//!
//! # 设计原则（chore/plans/awd/01-phase1-network-recovery.md）
//!
//! - base chain `policy accept`：FloatCTF 只做 **restrictive DROP**，
//!   不主动 ACCEPT 业务流量（own-team 放行规则是 hardening 隔离的必要精确表达）。
//! - 只用 sets，不生成 per-team ban chains；禁 `nft flush ruleset`。
//! - `inet` family 统一 IPv4/IPv6；IPv6 显式策略占位（AWD v4-only 部署，管理范围默认 DROP）。
//! - 规则规模 O(N)（N = 队伍数），不生成 O(N²) 的 Team A→Team B 规则。

use std::collections::BTreeSet;

use sha2::{Digest, Sha256};

use crate::entity::sea_orm_active_enums::AwdPhase;

use crate::modules::event::awd_team::domain::firewall_state::{
    DesiredEventPolicy, DesiredFirewallState,
};

/// FloatCTF 唯一拥有的 nftables table。
pub const TABLE_NAME: &str = "floatctf_awd";
/// forward hook 上 FloatCTF 的 priority（平台技术常量，P1-1 决定；见 docs/awd-nftables-host-discovery.md）。
pub const FORWARD_PRIORITY: i32 = 1;

/// 短稳定 nft 对象名（P1-5/§5.18）。
///
/// 规则：lowercase + 固定前缀 + 长度有界 + 确定性生成；禁止拼完整 event UUID。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NftObjectName(String);

impl NftObjectName {
    /// `ev_` + event_id SHA-256 前 8 个 hex 字符。
    pub fn event_key(event_id: &uuid::Uuid) -> Self {
        let hash = Sha256::digest(event_id.as_bytes());
        Self(format!("ev_{}", hex::encode(&hash[..4])))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn fmt_set(name: &str, elements: &BTreeSet<String>) -> String {
    let mut out = format!("    set {name} {{\n        type ipv4_addr\n        flags interval");
    if !elements.is_empty() {
        let joined = elements.iter().cloned().collect::<Vec<_>>().join(", ");
        out.push_str(&format!("\n        elements = {{ {joined} }}"));
    }
    out.push_str("\n    }\n");
    out
}

fn fmt_set_v6(name: &str) -> String {
    format!("    set {name} {{\n        type ipv6_addr\n        flags interval\n    }}\n")
}

/// 渲染完整 `table inet floatctf_awd` 规则文本（含 revision 注释，仅 observability）。
pub fn render_table(desired: &DesiredFirewallState) -> String {
    let mut out = String::new();
    out.push_str("table inet ");
    out.push_str(TABLE_NAME);
    out.push_str(" {\n");
    out.push_str(&format!(
        "    comment \"managed-by=floatctf revision={}\"\n",
        desired.revision
    ));

    // ── base chain ──
    out.push_str("    chain awd_forward {\n");
    out.push_str(&format!(
        "        type filter hook forward priority {FORWARD_PRIORITY}; policy accept;\n"
    ));
    out.push_str(
        "        # managed by FloatCTF: restrictive DROP only; no iptables/Docker rules touched\n",
    );
    out.push_str("        ip saddr @banned_players_v4 drop\n");
    out.push_str("        ip6 saddr @banned_players_v6 drop\n");
    for event in &desired.events {
        out.push_str(&format!(
            "        jump event_{}\n",
            NftObjectName::event_key(&event.event_id).as_str()
        ));
    }
    out.push_str("    }\n\n");

    // ── 全局 sets ──
    let mut all_gameboxes: BTreeSet<String> = BTreeSet::new();
    let mut all_players: BTreeSet<String> = BTreeSet::new();
    let mut infra: BTreeSet<String> = BTreeSet::new();
    let mut banned: BTreeSet<String> = BTreeSet::new();
    for event in &desired.events {
        all_gameboxes.insert(event.gamebox_network.as_str());
        for team in &event.teams {
            all_players.insert(team.wireguard_network.as_str());
            if event.banned_teams.contains(&team.team_id) {
                // ban：阻断该队玩家 WG 子网 + GameBox 子网的入站流量（saddr 匹配）
                banned.insert(team.wireguard_network.as_str());
                banned.insert(team.gamebox_network.as_str());
            }
        }
        infra.insert(event.infrastructure_network.as_str());
        infra.insert(event.flagserver_ip.to_string());
        infra.insert(event.judgeserver_ip.to_string());
    }
    out.push_str(&fmt_set("all_gameboxes_v4", &all_gameboxes));
    out.push_str(&fmt_set("infrastructure_v4", &infra));
    out.push_str(&fmt_set("player_wg_v4", &all_players));
    out.push_str(&fmt_set("banned_players_v4", &banned));
    out.push_str(&fmt_set_v6("banned_players_v6"));
    out.push('\n');

    // ── per-event sets + chains ──
    for event in &desired.events {
        let key = NftObjectName::event_key(&event.event_id);
        let mut gb: BTreeSet<String> = BTreeSet::new();
        let mut players: BTreeSet<String> = BTreeSet::new();
        for team in &event.teams {
            gb.insert(team.gamebox_network.as_str());
            players.insert(team.wireguard_network.as_str());
        }
        out.push_str(&fmt_set(&format!("{}_gameboxes_v4", key.as_str()), &gb));
        out.push_str(&fmt_set(&format!("{}_players_v4", key.as_str()), &players));
        // IPv6 显式策略占位：AWD v4-only，不分配 IPv6；空集即“管理范围默认 DROP”
        out.push_str(&fmt_set_v6(&format!("{}_gameboxes_v6", key.as_str())));
        out.push_str(&fmt_set_v6(&format!("{}_players_v6", key.as_str())));
        out.push('\n');
        out.push_str(&render_event_chain(event, &key));
        out.push('\n');
    }

    out.push_str("}\n");
    out
}

/// 渲染单个赛事的 event chain（phase 规则 + 公共 restrictive 规则）。
fn render_event_chain(event: &DesiredEventPolicy, key: &NftObjectName) -> String {
    let k = key.as_str();
    let mut out = String::new();
    out.push_str(&format!("    chain event_{k} {{\n"));

    match event.phase {
        AwdPhase::Hardening => {
            // 1) own-team 放行（精确表达 hardening 隔离；否则跨队阻断无法表达“自己除外”）
            for team in &event.teams {
                out.push_str(&format!(
                    "        # own-team {}: not denied\n",
                    team.team_id
                ));
                out.push_str(&format!(
                    "        ip saddr {} ip daddr {} accept\n",
                    team.wireguard_network.as_str(),
                    team.gamebox_network.as_str()
                ));
            }
            // 2) 其余 player→gamebox（跨队）默认 DROP
            out.push_str(&format!(
                "        ip saddr @{k}_players_v4 ip daddr @{k}_gameboxes_v4 drop\n"
            ));
            // 3) gamebox 全出网阻断（防横向移动 + 公网 + infra）
            out.push_str(&format!("        ip saddr @{k}_gameboxes_v4 drop\n"));
        }
        AwdPhase::Attack => {
            // 攻击阶段：不阻止跨队 player→gamebox
            // gamebox 出网：允许 gamebox↔gamebox（攻击协同）+ flagserver，其余阻断
            out.push_str(&format!(
                "        ip saddr @{k}_gameboxes_v4 ip daddr @{k}_gameboxes_v4 accept\n"
            ));
            out.push_str(&format!(
                "        ip saddr @{k}_gameboxes_v4 ip daddr {} accept\n",
                event.flagserver_ip
            ));
            out.push_str(&format!("        ip saddr @{k}_gameboxes_v4 drop\n"));
        }
        AwdPhase::Pause => {
            // 暂停：阻断比赛网络路径（玩家与 GameBox 双向）
            out.push_str(&format!("        ip saddr @{k}_players_v4 drop\n"));
            out.push_str(&format!("        ip saddr @{k}_gameboxes_v4 drop\n"));
        }
    }

    // ── 公共 restrictive 规则（所有 phase）──
    // 玩家/GameBox → 基础设施（FlagServer/JudgeServer/infra 网段）阻断
    out.push_str(&format!(
        "        ip saddr @{k}_players_v4 ip daddr @infrastructure_v4 drop\n"
    ));
    out.push_str(&format!(
        "        ip saddr @{k}_gameboxes_v4 ip daddr @infrastructure_v4 drop\n"
    ));
    // IPv6 显式策略（v6 占位空集 = 管理范围默认 DROP；若未来分配 v6 地址则注入 set 生效）
    out.push_str(&format!("        ip6 saddr @{k}_players_v6 drop\n"));
    out.push_str(&format!("        ip6 saddr @{k}_gameboxes_v6 drop\n"));

    out.push_str("    }\n");
    out
}

/// 观测到的防火墙状态（reconcile 后从 `nft list table` 解析）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ObservedFirewallState {
    pub table_exists: bool,
    /// 从 table comment 解析出的 revision（`managed-by=floatctf revision=N`）。
    pub observed_revision: Option<u64>,
    /// 观测到的 event chains（`event_<key>`）。
    pub event_chains: Vec<String>,
    pub notes: Vec<String>,
}

/// 从 `nft list table inet floatctf_awd` 输出解析观测状态（P1-9 verify 用）。
pub fn parse_observed_table(output: &str) -> ObservedFirewallState {
    let mut state = ObservedFirewallState::default();
    state.table_exists = output.contains("table inet floatctf_awd");
    for line in output.lines() {
        let line = line.trim();
        if let Some(idx) = line.find("managed-by=floatctf revision=") {
            let rest = &line[idx + "managed-by=floatctf revision=".len()..];
            if let Ok(n) = rest
                .split(|c: char| !c.is_ascii_digit())
                .next()
                .unwrap_or("")
                .parse::<u64>()
            {
                state.observed_revision = Some(n);
            }
        }
        if let Some(rest) = line.strip_prefix("chain event_") {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() {
                // 还原完整 chain 名（event_<key>），与 verify() 的期望一致
                state.event_chains.push(format!("event_{name}"));
            }
        }
    }
    state
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::event::awd_team::domain::firewall_state::{DesiredTeamPolicy, IpNet};
    use uuid::Uuid;

    fn sample_event(phase: AwdPhase, banned: bool) -> DesiredEventPolicy {
        let team_a = Uuid::new_v4();
        let team_b = Uuid::new_v4();
        let mut event = DesiredEventPolicy {
            event_key: "ev_sample".into(),
            event_id: Uuid::new_v4(),
            phase,
            gamebox_network: IpNet::parse("10.42.0.0/16").unwrap(),
            infrastructure_network: IpNet::parse("10.42.0.0/24").unwrap(),
            flagserver_ip: "10.42.0.10".parse().unwrap(),
            judgeserver_ip: "10.42.0.11".parse().unwrap(),
            teams: vec![
                DesiredTeamPolicy {
                    team_id: team_a,
                    wireguard_network: IpNet::parse("172.31.1.0/24").unwrap(),
                    gamebox_network: IpNet::parse("10.42.1.0/24").unwrap(),
                },
                DesiredTeamPolicy {
                    team_id: team_b,
                    wireguard_network: IpNet::parse("172.31.2.0/24").unwrap(),
                    gamebox_network: IpNet::parse("10.42.2.0/24").unwrap(),
                },
            ],
            banned_teams: if banned { vec![team_a] } else { vec![] },
        };
        event.event_key = NftObjectName::event_key(&event.event_id)
            .as_str()
            .to_string();
        event
    }

    #[test]
    fn render_hardening_blocks_cross_team_not_own() {
        let desired = DesiredFirewallState {
            revision: 7,
            events: vec![sample_event(AwdPhase::Hardening, false)],
        };
        let text = render_table(&desired);
        assert!(text.contains("table inet floatctf_awd"));
        assert!(text.contains("managed-by=floatctf revision=7"));
        assert!(text.contains("hook forward priority 1; policy accept;"));
        // own-team accept 先于跨队 drop
        assert!(text.contains("ip saddr 172.31.1.0/24 ip daddr 10.42.1.0/24 accept"));
        assert!(text.contains("ip saddr 172.31.2.0/24 ip daddr 10.42.2.0/24 accept"));
        assert!(text.contains("ip saddr @ev_"));
        // 无 O(N²)：不出现 Team A → Team B 的单条规则
        assert!(!text.contains("172.31.1.0/24 ip daddr 10.42.2.0/24"));
    }

    #[test]
    fn render_attack_opens_cross_team_and_flagserver() {
        let desired = DesiredFirewallState {
            revision: 1,
            events: vec![sample_event(AwdPhase::Attack, false)],
        };
        let text = render_table(&desired);
        assert!(text.contains("ip saddr @ev_"));
        assert!(text.contains("ip daddr 10.42.0.10 accept")); // flagserver
        // attack 无跨队 drop：不存在 players→gameboxes drop
        assert!(!text.contains("_players_v4 ip daddr @ev_"));
    }

    #[test]
    fn render_pause_blocks_all_game_traffic() {
        let desired = DesiredFirewallState {
            revision: 2,
            events: vec![sample_event(AwdPhase::Pause, false)],
        };
        let text = render_table(&desired);
        assert!(text.contains("ip saddr @ev_"));
        assert!(text.contains("drop"));
    }

    #[test]
    fn render_ban_populates_banned_set() {
        let desired = DesiredFirewallState {
            revision: 3,
            events: vec![sample_event(AwdPhase::Attack, true)],
        };
        let text = render_table(&desired);
        // banned set 含被 ban 队伍的 WG + GameBox 子网
        assert!(text.contains("banned_players_v4"));
        assert!(text.contains("172.31.1.0/24"));
        assert!(text.contains("10.42.1.0/24"));
        // 无 per-team ban chains
        assert!(!text.contains("FCTF-B-"));
    }

    #[test]
    fn render_ipv6_explicit_policy_present() {
        let desired = DesiredFirewallState {
            revision: 4,
            events: vec![sample_event(AwdPhase::Hardening, false)],
        };
        let text = render_table(&desired);
        // inet family：显式 IPv6 策略（占位空集）+ banned v6
        assert!(text.contains("_players_v6"));
        assert!(text.contains("_gameboxes_v6"));
        assert!(text.contains("banned_players_v6"));
        assert!(text.contains("ip6 saddr"));
    }

    #[test]
    fn render_multi_event_keeps_both_chains() {
        let e1 = sample_event(AwdPhase::Hardening, false);
        let e2 = sample_event(AwdPhase::Attack, false);
        let desired = DesiredFirewallState {
            revision: 5,
            events: vec![e1.clone(), e2],
        };
        let text = render_table(&desired);
        let k1 = NftObjectName::event_key(&e1.event_id).as_str().to_string();
        assert!(text.contains(&format!("chain event_{k1}")));
        assert!(text.contains(&format!("jump event_{k1}")));
        assert_eq!(text.matches("chain event_").count(), 2);
    }

    #[test]
    fn parse_observed_table_extracts_revision_and_chains() {
        let out = "table inet floatctf_awd {\n    comment \"managed-by=floatctf revision=42\"\n    chain event_ev_ab12cd34 {\n    }\n}\n";
        let state = parse_observed_table(out);
        assert!(state.table_exists);
        assert_eq!(state.observed_revision, Some(42));
        assert_eq!(state.event_chains, vec!["event_ev_ab12cd34"]);
    }

    #[test]
    fn parse_observed_table_empty() {
        let state = parse_observed_table("table inet floatctf_awd {\n}\n");
        assert!(state.table_exists);
        assert_eq!(state.observed_revision, None);
        assert!(state.event_chains.is_empty());
    }

    #[test]
    fn event_key_is_stable_and_bounded() {
        let id = Uuid::new_v4();
        let k1 = NftObjectName::event_key(&id);
        let k2 = NftObjectName::event_key(&id);
        assert_eq!(k1, k2);
        assert!(k1.as_str().len() <= 16);
        assert!(k1.as_str().starts_with("ev_"));
        assert!(
            k1.as_str()
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        );
    }
}
