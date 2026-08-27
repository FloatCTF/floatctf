//! AWD 防火墙期望态模型（Phase 1 P1-4，原生 nftables）。

use uuid::Uuid;

use crate::entity::sea_orm_active_enums::AwdPhase;

/// 全局期望防火墙状态：revision + 各赛事策略。
#[derive(Debug, Clone, Default)]
pub struct DesiredFirewallState {
    /// 策略版本：每次 Desired State 变化 +1；reconcile 成功后
    /// `observed_revision = desired_revision`（DB 是事实源，revision 存 DB）。
    pub revision: u64,
    /// 进入防火墙 desired set 的赛事策略（全局重建，Event A 更新不影响 Event B）。
    pub events: Vec<DesiredEventPolicy>,
}

impl DesiredFirewallState {
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn event_keys(&self) -> Vec<&str> {
        self.events.iter().map(|e| e.event_key.as_str()).collect()
    }
}

/// 单个赛事的期望策略。
#[derive(Debug, Clone)]
pub struct DesiredEventPolicy {
    /// 短稳定事件 key（如 `ev_ab12cd34`），用于 nft 对象命名，禁止拼完整 UUID。
    pub event_key: String,
    pub event_id: Uuid,
    /// 当前比赛阶段（Hardening / Attack / Pause）。
    pub phase: AwdPhase,
    /// 赛事 GameBox 网络（如 10.42.0.0/16）。
    pub gamebox_network: IpNet,
    /// 赛事基础设施网络（FlagServer/JudgeServer 所在子网，如 10.42.0.0/24）。
    pub infrastructure_network: IpNet,
    pub flagserver_ip: std::net::Ipv4Addr,
    pub judgeserver_ip: std::net::Ipv4Addr,
    /// 参赛队伍网络分配。
    pub teams: Vec<DesiredTeamPolicy>,
    /// 被 ban 的队伍（WG/GameBox 子网进入 banned set）。
    pub banned_teams: Vec<Uuid>,
    /// 是否处于最终结算期（final round completed, Judge pending, Attack phase）。
    /// 结算期防火墙 = Pause 规则（阻断全部玩家/GameBox 比赛流量），
    /// 但 JudgeServer→GameBox 仍可达（infra 不在 player/gamebox set 中）。
    pub is_final_settlement: bool,
    /// 赛事是否已结束。Finished 事件保持在防火墙 desired set 中，
    /// 渲染为显式 DENY-ALL 规则，确保 fail-closed 网络锁定。
    pub is_finished: bool,
}

impl DesiredEventPolicy {
    pub fn banned_subnets(&self) -> Vec<String> {
        self.teams
            .iter()
            .filter(|t| self.banned_teams.contains(&t.team_id))
            .map(|t| t.wireguard_network.as_str())
            .collect()
    }
}

/// 单个队伍的期望网络分配。
#[derive(Debug, Clone)]
pub struct DesiredTeamPolicy {
    pub team_id: Uuid,
    /// 队伍 WireGuard 玩家子网（如 172.31.1.0/24）。
    pub wireguard_network: IpNet,
    /// 队伍 GameBox 子网（如 10.42.1.0/24）。
    pub gamebox_network: IpNet,
}

/// 网络（CIDR）类型：解析校验 + 确定性字符串表示。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IpNet {
    pub addr: std::net::Ipv4Addr,
    pub prefix_len: u8,
}

impl IpNet {
    pub fn parse(s: &str) -> Result<Self, String> {
        let (ip, prefix) = s
            .split_once('/')
            .ok_or_else(|| format!("invalid CIDR: {s}"))?;
        let addr: std::net::Ipv4Addr = ip
            .parse()
            .map_err(|_| format!("invalid IPv4 in CIDR: {s}"))?;
        let prefix_len: u8 = prefix
            .parse()
            .map_err(|_| format!("invalid prefix in CIDR: {s}"))?;
        if prefix_len > 32 {
            return Err(format!("invalid prefix length in CIDR: {s}"));
        }
        Ok(Self { addr, prefix_len })
    }

    pub fn as_str(&self) -> String {
        format!("{}/{}", self.addr, self.prefix_len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ipnet_parse_roundtrip() {
        let net = IpNet::parse("10.42.1.0/24").unwrap();
        assert_eq!(net.as_str(), "10.42.1.0/24");
        assert_eq!(net.prefix_len, 24);
    }

    #[test]
    fn ipnet_rejects_garbage() {
        assert!(IpNet::parse("10.42.1.0").is_err());
        assert!(IpNet::parse("10.42.1.0/33").is_err());
        assert!(IpNet::parse("not-an-ip/24").is_err());
    }

    #[test]
    fn banned_subnets_derived_from_teams() {
        let team_a = DesiredTeamPolicy {
            team_id: Uuid::new_v4(),
            wireguard_network: IpNet::parse("172.31.1.0/24").unwrap(),
            gamebox_network: IpNet::parse("10.42.1.0/24").unwrap(),
        };
        let team_b = DesiredTeamPolicy {
            team_id: Uuid::new_v4(),
            wireguard_network: IpNet::parse("172.31.2.0/24").unwrap(),
            gamebox_network: IpNet::parse("10.42.2.0/24").unwrap(),
        };
        let event = DesiredEventPolicy {
            event_key: "ev_ab12cd34".into(),
            event_id: Uuid::new_v4(),
            phase: AwdPhase::Attack,
            gamebox_network: IpNet::parse("10.42.0.0/16").unwrap(),
            infrastructure_network: IpNet::parse("10.42.0.0/24").unwrap(),
            flagserver_ip: "10.42.0.10".parse().unwrap(),
            judgeserver_ip: "10.42.0.11".parse().unwrap(),
            teams: vec![team_a.clone(), team_b.clone()],
            banned_teams: vec![team_a.team_id],
            is_final_settlement: false,
            is_finished: false,
        };
        let banned = event.banned_subnets();
        assert_eq!(banned, vec!["172.31.1.0/24"]);
    }
}
