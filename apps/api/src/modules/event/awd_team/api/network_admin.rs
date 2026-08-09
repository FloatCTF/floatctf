//! Platform AWD Networking API（§73）：
//! - GET/PATCH /admin/awd/network            平台网络设置（pools / prefix / WG port / endpoint）
//! - GET  /admin/awd/network/health          Host 观测状态（§4.1，只读）
//! - GET  /admin/awd/network/allocations     平台分配账本视图（§7/§66）
//!
//! 全部 Admin only（§72）。平台页不允许切 Docker backend / 关闭 firewalld（§4.1/§44）。

use actix_web::{get, patch, web};
use serde::Deserialize;

use crate::api::{
    AppError, UniResponse, UniResult, extractor::auth::SuperAdminJwtGuard, prelude::*,
};
use crate::modules::event::awd_team::{
    repo::network_settings_repo::NetworkSettingsPatch, service::platform_network_service,
};

#[derive(Debug, Deserialize)]
pub struct PlatformNetworkSettingsUpdateRequest {
    pub gamebox_pool: Option<String>,
    pub gamebox_event_prefix: Option<i16>,
    pub gamebox_team_prefix: Option<i16>,
    pub wireguard_pool: Option<String>,
    pub wireguard_event_prefix: Option<i16>,
    pub wireguard_team_prefix: Option<i16>,
    pub wireguard_port_min: Option<i32>,
    pub wireguard_port_max: Option<i32>,
    pub wireguard_public_endpoint: Option<String>,
}

/// GET /api/admin/awd/network
#[get("/awd/network")]
pub async fn get_platform_network(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
) -> UniResult<serde_json::Value> {
    let settings = platform_network_service::get_settings(ctx.db.get_ref())
        .await
        .map_err(AppError::from)?;

    use crate::modules::event::awd_team::domain::network::{
        Ipv4Cidr, NetworkPool, WireGuardPortRange,
    };

    let gb_pool = NetworkPool::new(
        Ipv4Cidr::parse(&settings.gamebox_pool.to_string())?,
        settings.gamebox_event_prefix as u8,
        settings.gamebox_team_prefix as u8,
    )
    .map_err(AppError::from)?;
    let wg_pool = NetworkPool::new(
        Ipv4Cidr::parse(&settings.wireguard_pool.to_string())?,
        settings.wireguard_event_prefix as u8,
        settings.wireguard_team_prefix as u8,
    )
    .map_err(AppError::from)?;
    let port_range = WireGuardPortRange::new(
        settings.wireguard_port_min as u16,
        settings.wireguard_port_max as u16,
    )
    .map_err(AppError::from)?;

    Ok(UniResponse::ok(Some(serde_json::json!({
        "gamebox_pool": settings.gamebox_pool.to_string(),
        "gamebox_event_prefix": settings.gamebox_event_prefix,
        "gamebox_team_prefix": settings.gamebox_team_prefix,
        "wireguard_pool": settings.wireguard_pool.to_string(),
        "wireguard_event_prefix": settings.wireguard_event_prefix,
        "wireguard_team_prefix": settings.wireguard_team_prefix,
        "wireguard_port_min": settings.wireguard_port_min,
        "wireguard_port_max": settings.wireguard_port_max,
        "wireguard_public_endpoint": settings.wireguard_public_endpoint,
        "updated_at": settings.updated_at.to_rfc3339(),
        // 容量预览（§67）
        "gamebox_event_capacity": gb_pool.event_capacity(),
        "gamebox_team_capacity_per_event": gb_pool.team_capacity_per_event(),
        "gamebox_hosts_per_team": gb_pool.hosts_per_team(),
        "wireguard_event_capacity": wg_pool.event_capacity(),
        "wireguard_team_capacity_per_event": wg_pool.team_capacity_per_event(),
        "wireguard_port_capacity": port_range.capacity(),
    })))
    .into())
}

/// PATCH /api/admin/awd/network
#[patch("/awd/network")]
pub async fn update_platform_network(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    body: web::Json<PlatformNetworkSettingsUpdateRequest>,
) -> UniResult<serde_json::Value> {
    let b = body.into_inner();
    let patch = NetworkSettingsPatch {
        gamebox_pool: b.gamebox_pool,
        gamebox_event_prefix: b.gamebox_event_prefix,
        gamebox_team_prefix: b.gamebox_team_prefix,
        wireguard_pool: b.wireguard_pool,
        wireguard_event_prefix: b.wireguard_event_prefix,
        wireguard_team_prefix: b.wireguard_team_prefix,
        wireguard_port_min: b.wireguard_port_min,
        wireguard_port_max: b.wireguard_port_max,
        wireguard_public_endpoint: b.wireguard_public_endpoint,
    };
    let updated = platform_network_service::update_settings(ctx.db.get_ref(), patch)
        .await
        .map_err(AppError::from)?;

    Ok(UniResponse::ok(Some(serde_json::json!({
        "gamebox_pool": updated.gamebox_pool.to_string(),
        "wireguard_pool": updated.wireguard_pool.to_string(),
        "wireguard_public_endpoint": updated.wireguard_public_endpoint,
        "updated_at": updated.updated_at.to_rfc3339(),
        "note": "现有 Event 分配不受影响，仅在 future allocations 生效（§31/§32）",
    })))
    .into())
}

/// GET /api/admin/awd/network/health（§4.1 Host Status：纯观测）
#[get("/awd/network/health")]
pub async fn get_platform_network_health(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
) -> UniResult<platform_network_service::PlatformHostStatus> {
    let status = platform_network_service::host_status(ctx.db.get_ref())
        .await
        .map_err(AppError::from)?;
    Ok(UniResponse::ok(Some(status)).into())
}

/// GET /api/admin/awd/network/allocations（§7/§66 可见性）
#[get("/awd/network/allocations")]
pub async fn get_platform_network_allocations(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
) -> UniResult<Vec<platform_network_service::PlatformAllocation>> {
    let allocations = platform_network_service::allocations_view(ctx.db.get_ref())
        .await
        .map_err(AppError::from)?;
    Ok(UniResponse::ok(Some(allocations)).into())
}
