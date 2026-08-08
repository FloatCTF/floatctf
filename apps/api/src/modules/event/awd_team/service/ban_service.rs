//! Ban 跨层闭环（Phase 4 P4-5/P4-6/P4-7，nftables banned set 版）。
//!
//! 单入口 `ban_service::ban_team`（取代旧 render_ban_rules / per-team ban chains）：
//!
//! ```text
//! ban:
//!   1. DB：写 awd_team_bans（desired state）
//!   2. WG：host 移除该队 Active peers（DB 保持 Active = suspend，非永久 revoke）
//!   3. Firewall：全局 DesiredFirewallState（banned_teams += team）→ nft reconcile
//!      → banned 子网成为 @banned_players_v4 set element
//!   4. Conntrack：flush 该队相关连接
//!   5. Realtime：publish team.banned
//!
//! unban：反向（DB unbanned → host 恢复 peers → reconcile 移除 set element → publish）
//!
//! 失败模型（§5.3）：DB ban 是 Desired State；WG/firewall/conntrack 逐项 reconcile，
//! 任何失败 → 返回错误（可重跑）；Recovery（P1-11）按 DB bans 重建，
//! 不依赖 nft table 中旧 ban elements 作为事实源。
//! ```

use sea_orm::DatabaseConnection;
use tracing::info;
use uuid::Uuid;

use crate::infrastructure::realtime::EventPublisher;
use crate::modules::event::awd_team::{
    AwdError, AwdResult,
    infrastructure::{firewall::FirewallRuntime, network::AwdNetworkRuntime},
    repo::ban_repo,
    service::{firewall_service, wireguard_service},
    websocket,
};

/// 发起 ban（跨层闭环）。返回 ban id。
pub async fn ban_team(
    db: &DatabaseConnection,
    network: &dyn AwdNetworkRuntime,
    firewall: &dyn FirewallRuntime,
    publisher: &dyn EventPublisher,
    event_id: Uuid,
    team_id: Uuid,
    reason: Option<&str>,
    banned_by: Option<Uuid>,
) -> AwdResult<Uuid> {
    // 1. DB desired ban
    let active_round =
        crate::modules::event::awd_team::repo::round_repo::find_active_round(db, event_id)
            .await
            .map_err(|e| AwdError::Database(e.to_string()))?;
    let ban = ban_repo::create_ban(
        db,
        event_id,
        team_id,
        reason,
        active_round.map(|r| r.id),
        banned_by,
    )
    .await
    .map_err(|e| AwdError::Database(e.to_string()))?;

    // 2. WG host 挂起（DB 保持 Active）
    let removed =
        wireguard_service::suspend_team_peers_from_host(db, network, event_id, team_id).await?;
    info!(
        "[Ban] Team {team_id} banned in event {event_id}: {removed} WG peer(s) suspended on host"
    );

    // 3. Firewall：banned set reconcile（全局，DB 是事实源）
    firewall_service::reconcile_global(
        db,
        firewall,
        firewall_service::next_network_revision(db).await?,
    )
    .await?;

    // 4. Conntrack 清理（该队 WG 子网）
    let team_net = crate::modules::event::awd_team::repo::gamebox_repo::find_team_network(
        db, event_id, team_id,
    )
    .await
    .map_err(|e| AwdError::Database(e.to_string()))?;
    if let Some(net) = team_net {
        use crate::modules::event::awd_team::infrastructure::network::TeamNetworkIdentity;
        if let Err(e) = network
            .clear_team_connections(TeamNetworkIdentity {
                event_id,
                team_id,
                gamebox_subnet: net.gamebox_subnet.clone(),
            })
            .await
        {
            tracing::error!("[Ban] conntrack cleanup failed for team {team_id}: {e}");
        }
    }

    // 5. Realtime（best-effort）
    let _ = publisher
        .publish(websocket::team_banned(event_id, team_id).into_realtime())
        .await;

    Ok(ban.id)
}

/// 解封（反向闭环）。
pub async fn unban_team(
    db: &DatabaseConnection,
    network: &dyn AwdNetworkRuntime,
    firewall: &dyn FirewallRuntime,
    publisher: &dyn EventPublisher,
    event_id: Uuid,
    team_id: Uuid,
    unbanned_by: Option<Uuid>,
) -> AwdResult<()> {
    let ban = ban_repo::find_active_ban(db, event_id, team_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound("No active ban found".into()))?;

    // 1. DB unbanned
    ban_repo::complete_unban(db, ban.id, unbanned_by)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    // 2. WG host 恢复 Active peers（幂等）
    wireguard_service::restore_active_peers_to_host(db, network, event_id).await?;

    // 3. Firewall：banned set 移除该队 subnet（全局 reconcile）
    firewall_service::reconcile_global(
        db,
        firewall,
        firewall_service::next_network_revision(db).await?,
    )
    .await?;

    // 4. Realtime（best-effort）
    let _ = publisher
        .publish(websocket::team_unbanned(event_id, team_id).into_realtime())
        .await;

    info!("[Ban] Team {team_id} unbanned in event {event_id}");
    Ok(())
}

/// P4-7 自动解封：按 ban id 解封（scheduler unban 任务用）。
pub async fn unban_team_by_ban_id(
    db: &DatabaseConnection,
    network: &dyn AwdNetworkRuntime,
    firewall: &dyn FirewallRuntime,
    publisher: &dyn EventPublisher,
    event_id: Uuid,
    ban_id: Uuid,
) -> AwdResult<()> {
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
    let ban = crate::entity::awd_team_bans::Entity::find()
        .filter(crate::entity::awd_team_bans::Column::Id.eq(ban_id))
        .one(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound("Ban not found".into()))?;
    if ban.event_id != event_id {
        return Err(AwdError::Forbidden("Ban belongs to another event".into()));
    }
    unban_team(
        db,
        network,
        firewall,
        publisher,
        event_id,
        ban.team_id,
        None,
    )
    .await
}
