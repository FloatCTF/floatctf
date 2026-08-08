//! Global firewall desired-state reconcile orchestrator (Phase 1 P1-10).
//!
//! 唯一路径：DB（Desired State）→ `DesiredFirewallState`（全局）→ `FirewallRuntime.reconcile()`。
//! 禁止在任何 service 直接 add/delete 单条 nft rule。

use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter};
use uuid::Uuid;

use crate::entity::{
    awd_events, awd_team_bans, awd_team_networks, sea_orm_active_enums::AwdEventStatus,
};
use crate::modules::event::awd_team::{
    AwdError, AwdResult,
    domain::firewall_state::{DesiredEventPolicy, DesiredFirewallState, DesiredTeamPolicy, IpNet},
    infrastructure::firewall::FirewallRuntime,
};

/// 进入防火墙 desired set 的赛事范围（§5.13b）。
///
/// 判定：**event 仍有 managed runtime/network 资源且未完全归档清理**。
/// 不能只看 Running/Paused —— Deploying（部分资源）/NetworkError（需要隔离保护）等
/// 非终态也必须保持策略。
pub fn in_firewall_desired_set(status: &AwdEventStatus) -> bool {
    matches!(
        status,
        AwdEventStatus::Deploying
            | AwdEventStatus::Deployed
            | AwdEventStatus::Prechecking
            | AwdEventStatus::Verified
            | AwdEventStatus::Running
            | AwdEventStatus::Paused
            | AwdEventStatus::NetworkError
            | AwdEventStatus::StartBlocked
            | AwdEventStatus::DeployFailed
    )
}

/// 由 DB 构建全局 DesiredFirewallState（revision 由调用方/存储层提供）。
pub async fn build_desired_state<C: ConnectionTrait + Send>(
    db: &C,
    revision: u64,
) -> AwdResult<DesiredFirewallState> {
    let events = awd_events::Entity::find()
        .all(db)
        .await
        .map_err(|e| AwdError::Database(format!("load awd_events: {e}")))?;
    // Event Network 是 Desired State 源（§45/§54）；无网络的 event 不进防火墙 desired set
    let event_networks =
        crate::modules::event::awd_team::repo::event_network_repo::list_all(db).await?;

    let mut desired = DesiredFirewallState {
        revision,
        events: Vec::new(),
    };

    for event in events {
        if !in_firewall_desired_set(&event.status) {
            continue;
        }
        let Some(event_network) = event_networks
            .iter()
            .find(|en| en.event_id == event.event_id)
            .cloned()
        else {
            continue;
        };

        // 队伍网络分配
        let networks = awd_team_networks::Entity::find()
            .filter(awd_team_networks::Column::EventId.eq(event.event_id))
            .all(db)
            .await
            .map_err(|e| AwdError::Database(format!("load awd_team_networks: {e}")))?;

        // 活跃 ban
        let bans = awd_team_bans::Entity::find()
            .filter(awd_team_bans::Column::EventId.eq(event.event_id))
            .filter(
                awd_team_bans::Column::Status
                    .eq(crate::entity::sea_orm_active_enums::BanStatus::Active),
            )
            .all(db)
            .await
            .map_err(|e| AwdError::Database(format!("load awd_team_bans: {e}")))?;

        let mut teams = Vec::new();
        for n in networks {
            teams.push(DesiredTeamPolicy {
                team_id: n.team_id,
                wireguard_network: IpNet::parse(&n.wireguard_subnet.to_string())
                    .map_err(|e| AwdError::Validation(format!("wg subnet: {e}")))?,
                gamebox_network: IpNet::parse(&n.gamebox_subnet.to_string())
                    .map_err(|e| AwdError::Validation(format!("gamebox subnet: {e}")))?,
            });
        }

        let flagserver_ip: std::net::Ipv4Addr = event_network
            .flagserver_ip
            .to_string()
            .parse()
            .map_err(|_| {
                AwdError::Validation(format!("flagserver_ip {}", event_network.flagserver_ip))
            })?;
        let judgeserver_ip: std::net::Ipv4Addr = event_network
            .judgeserver_ip
            .to_string()
            .parse()
            .map_err(|_| {
            AwdError::Validation(format!("judgeserver_ip {}", event_network.judgeserver_ip))
        })?;

        desired.events.push(DesiredEventPolicy {
            event_key:
                crate::modules::event::awd_team::infrastructure::firewall::NftObjectName::event_key(
                    &event.event_id,
                )
                .as_str()
                .to_string(),
            event_id: event.event_id,
            phase: event.phase,
            gamebox_network: IpNet::parse(&event_network.gamebox_cidr.to_string())
                .map_err(|e| AwdError::Validation(format!("gamebox_cidr: {e}")))?,
            // 基础设施网络：Event Network 显式固化的 infrastructure_subnet（§25/§45）
            infrastructure_network: IpNet::parse(&event_network.infrastructure_subnet.to_string())
                .map_err(|e| AwdError::Validation(format!("infrastructure_subnet: {e}")))?,
            flagserver_ip,
            judgeserver_ip,
            banned_teams: bans.into_iter().map(|b| b.team_id).collect(),
            teams,
        });
    }

    Ok(desired)
}

/// 基础设施网络：FlagServer 所在 /24 网段（分配约定 infra=.0.x）。
fn infra_network(flagserver_ip: std::net::Ipv4Addr) -> IpNet {
    let octets = flagserver_ip.octets();
    IpNet {
        addr: std::net::Ipv4Addr::new(octets[0], octets[1], octets[2], 0),
        prefix_len: 24,
    }
}

/// 全局 reconcile：DB → Desired → runtime.reconcile。
///
/// 失败 Fail Closed：返回 Err（调用方经 transition_event 置 NetworkError）。
pub async fn reconcile_global(
    db: &sea_orm::DatabaseConnection,
    firewall: &dyn FirewallRuntime,
    revision: u64,
) -> AwdResult<crate::modules::event::awd_team::infrastructure::firewall::FirewallApplyResult> {
    let desired = build_desired_state(db, revision).await?;
    firewall.reconcile(&desired).await
}

/// 空态 reconcile：没有任何赛事需要策略时删除整个 `table inet floatctf_awd`（P4-13 用）。
pub async fn reconcile_empty(
    firewall: &dyn FirewallRuntime,
    revision: u64,
) -> AwdResult<crate::modules::event::awd_team::infrastructure::firewall::FirewallApplyResult> {
    firewall
        .reconcile(&DesiredFirewallState {
            revision,
            events: Vec::new(),
        })
        .await
}

/// 网络策略 revision（settings 表，跨重启持久）。
///
/// 每次 Desired State 变化 → +1；reconcile 成功后 nft table comment 中的
/// observed revision 与它一致（Phase 2 Precheck 比对）。
pub async fn next_network_revision(db: &sea_orm::DatabaseConnection) -> AwdResult<u64> {
    use crate::infrastructure::settings::{get_setting, upsert_setting};
    const KEY: &str = "AWD_NETWORK_REVISION";
    let current: u64 = get_setting(db, KEY)
        .await
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let next = current + 1;
    upsert_setting(db, KEY, &next.to_string())
        .await
        .map_err(|e| AwdError::Database(format!("revision persist: {e}")))?;
    Ok(next)
}

/// 当前策略 revision（读 settings；不存在视为 0 = 尚未 reconcile）。
pub async fn current_network_revision(db: &sea_orm::DatabaseConnection) -> u64 {
    use crate::infrastructure::settings::get_setting;
    get_setting(db, "AWD_NETWORK_REVISION")
        .await
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

/// 阶段切换后的 conntrack 清理（best-effort + 显式错误日志，P0-4 规则）。
/// 主策略（nft reconcile）已成功；这里只清理既有连接防绕过。
pub async fn flush_event_connections(
    network: &dyn crate::modules::event::awd_team::infrastructure::network::AwdNetworkRuntime,
    event_id: Uuid,
    gamebox_cidr: &str,
) {
    use crate::modules::event::awd_team::infrastructure::network::EventNetworkIdentity;
    if let Err(e) = network
        .clear_event_connections(EventNetworkIdentity {
            event_id,
            gamebox_cidr: gamebox_cidr.to_string(),
        })
        .await
    {
        tracing::error!(
            "[Firewall] conntrack cleanup failed for event {}: {}",
            event_id,
            e
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desired_set_covers_non_terminal_runtime_states() {
        assert!(in_firewall_desired_set(&AwdEventStatus::Deploying));
        assert!(in_firewall_desired_set(&AwdEventStatus::Deployed));
        assert!(in_firewall_desired_set(&AwdEventStatus::Prechecking));
        assert!(in_firewall_desired_set(&AwdEventStatus::Verified));
        assert!(in_firewall_desired_set(&AwdEventStatus::Running));
        assert!(in_firewall_desired_set(&AwdEventStatus::Paused));
        assert!(in_firewall_desired_set(&AwdEventStatus::NetworkError));
        assert!(in_firewall_desired_set(&AwdEventStatus::StartBlocked));
        assert!(in_firewall_desired_set(&AwdEventStatus::DeployFailed));
        // 终态/未部署不进入
        assert!(!in_firewall_desired_set(&AwdEventStatus::Draft));
        assert!(!in_firewall_desired_set(&AwdEventStatus::Configuring));
        assert!(!in_firewall_desired_set(&AwdEventStatus::Finished));
        assert!(!in_firewall_desired_set(&AwdEventStatus::Archived));
        assert!(!in_firewall_desired_set(
            &AwdEventStatus::VerificationFailed
        ));
    }

    #[test]
    fn infra_network_is_flagserver_24() {
        let net = infra_network("10.42.0.10".parse().unwrap());
        assert_eq!(net.as_str(), "10.42.0.0/24");
    }
}
