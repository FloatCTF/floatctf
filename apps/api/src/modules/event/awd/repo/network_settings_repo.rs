//! awd_network_settings：平台网络资源池（singleton，id = 1）。

use crate::entity::awd_network_settings;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ConnectionTrait, DatabaseConnection, EntityTrait,
};

use super::super::AwdError;

/// 字符串 → PG 原生 CIDR/INET 值（sqlx 以 IpNetwork 读写，§20）。
fn parse_ipnet(v: &str) -> Result<ipnetwork::IpNetwork, AwdError> {
    v.parse()
        .map_err(|e| AwdError::Validation(format!("invalid cidr/ip {v}: {e}")))
}

pub const SETTINGS_ID: i16 = 1;

/// 读取平台网络设置（singleton）。缺失时返回 NotFound。
pub async fn get<C: ConnectionTrait + Send>(
    db: &C,
) -> Result<awd_network_settings::Model, AwdError> {
    awd_network_settings::Entity::find_by_id(SETTINGS_ID)
        .one(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound("awd_network_settings singleton".into()))
}

/// 更新平台网络设置。设置变更只影响 future allocations（§31）。
pub async fn update(
    db: &DatabaseConnection,
    patch: NetworkSettingsPatch,
) -> Result<awd_network_settings::Model, AwdError> {
    let current = get(db).await?;
    let mut am: awd_network_settings::ActiveModel = current.clone().into();
    if let Some(v) = patch.gamebox_pool {
        am.gamebox_pool = Set(parse_ipnet(&v)?);
    }
    if let Some(v) = patch.gamebox_event_prefix {
        am.gamebox_event_prefix = Set(v);
    }
    if let Some(v) = patch.gamebox_team_prefix {
        am.gamebox_team_prefix = Set(v);
    }
    if let Some(v) = patch.wireguard_pool {
        am.wireguard_pool = Set(parse_ipnet(&v)?);
    }
    if let Some(v) = patch.wireguard_event_prefix {
        am.wireguard_event_prefix = Set(v);
    }
    if let Some(v) = patch.wireguard_team_prefix {
        am.wireguard_team_prefix = Set(v);
    }
    if let Some(v) = patch.wireguard_port_min {
        am.wireguard_port_min = Set(v);
    }
    if let Some(v) = patch.wireguard_port_max {
        am.wireguard_port_max = Set(v);
    }
    if let Some(v) = patch.wireguard_public_endpoint {
        am.wireguard_public_endpoint = Set(Some(v));
    }
    am.updated_at = Set(chrono::Utc::now().into());
    am.update(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))
}

pub struct NetworkSettingsPatch {
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

impl Default for NetworkSettingsPatch {
    fn default() -> Self {
        Self {
            gamebox_pool: None,
            gamebox_event_prefix: None,
            gamebox_team_prefix: None,
            wireguard_pool: None,
            wireguard_event_prefix: None,
            wireguard_team_prefix: None,
            wireguard_port_min: None,
            wireguard_port_max: None,
            wireguard_public_endpoint: None,
        }
    }
}
