//! 平台级 AWD 网络管理 API。

use actix_web::{get, patch, web};
use serde::{Deserialize, Deserializer};

use crate::api::{
    AppError, UniResponse, UniResult, extractor::auth::SuperAdminJwtGuard, prelude::*,
};
use crate::modules::event::awd::{
    repo::network_settings_repo::NetworkSettingsPatch, service::platform_network_service,
};

/// 区分「字段缺省」与「显式 null」：缺省 → `None`（不修改），
/// 显式 `null` → `Some(None)`（清空，例如 wireguard_public_endpoint）。
fn double_option<'de, D>(deserializer: D) -> Result<Option<Option<String>>, D::Error>
where
    D: Deserializer<'de>,
{
    Option::<String>::deserialize(deserializer).map(Some)
}

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
    #[serde(default, deserialize_with = "double_option")]
    pub wireguard_public_endpoint: Option<Option<String>>,
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

    use crate::modules::event::awd::domain::network::{Ipv4Cidr, NetworkPool, WireGuardPortRange};

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

    // 边界校验（F47）：公共端点必须是 IP 或含点的域名，可带 1..=65535 端口；
    // 空串/null 表示清空。此前任意字符串（实测 "not-a-url"）都会被写入并下发。
    if let Some(Some(raw)) = &b.wireguard_public_endpoint {
        let endpoint = raw.trim();
        if !endpoint.is_empty() && !is_valid_public_endpoint(endpoint) {
            return Err(AppError::BadRequest(
                "WireGuard 公共端点格式无效：应为「IP 或域名[:端口]」，例如 vpn.example.com:51820"
                    .into(),
            ));
        }
    }

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

/// 校验 WireGuard 公共端点：`IP[:port]` 或 `域名[:port]`（域名必须含点，避免
/// 把任意单词当主机名）；端口范围 1..=65535。
fn is_valid_public_endpoint(value: &str) -> bool {
    use std::net::IpAddr;
    if value.parse::<IpAddr>().is_ok() {
        return true;
    }
    let (host, port) = match value.rsplit_once(':') {
        Some((h, p)) => (h, Some(p)),
        None => (value, None),
    };
    if host.parse::<IpAddr>().is_ok() {
        // IPv4:port 形式
    } else {
        let label_ok = !host.is_empty()
            && host.len() <= 253
            && host.split('.').count() >= 2
            && host.split('.').all(|label| {
                !label.is_empty()
                    && label.len() <= 63
                    && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
            });
        if !label_ok {
            return false;
        }
    }
    match port {
        Some(p) => p
            .parse::<u32>()
            .map(|n| (1..=65535).contains(&n))
            .unwrap_or(false),
        None => true,
    }
}

#[cfg(test)]
mod boundary_tests {
    use super::is_valid_public_endpoint;

    #[test]
    fn public_endpoint_boundaries() {
        // 合法：IP、IP:端口、含点域名、域名:端口边界端口
        for ok in [
            "1.2.3.4",
            "1.2.3.4:1",
            "1.2.3.4:65535",
            "vpn.example.com",
            "vpn.example.com:51820",
            "sub.vpn.example.com:51820",
            "2001:db8::1",
        ] {
            assert!(is_valid_public_endpoint(ok), "应接受: {ok}");
        }
        // 非法：空、无点单词、端口 0/65536、非数字端口、空标签、超长标签
        for bad in [
            "",
            "not-a-url",
            "1.2.3.4:0",
            "1.2.3.4:65536",
            "1.2.3.4:abc",
            "1.2.3.4:",
            "vpn..example.com",
            ".example.com",
            "example.com.",
        ] {
            assert!(!is_valid_public_endpoint(bad), "应拒绝: {bad}");
        }
        let long_label = "a".repeat(64);
        assert!(!is_valid_public_endpoint(&format!("{long_label}.com")));
    }
}
