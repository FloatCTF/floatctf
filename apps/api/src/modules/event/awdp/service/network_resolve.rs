//! 练习 data 网络容器解析助手：source-IP → 运行中实例（/flag 与 /proof 共用）。

use bollard::Docker;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use uuid::Uuid;

use crate::entity::{awdp_instances, event_instances};
use crate::modules::event::awdp::{AwdpError, AwdpResult, domain::judge::PRACTICE_NETWORK_NAME};

/// 按 data 网络当前容器 IP 解析 (instance, ext, run_id)。
///
/// 事实来源 = Docker network inspect（current physical attachment），
/// 不信任客户端声明。未知 IP / 非运行实例 → Forbidden。
pub async fn resolve_instance_by_network_ip(
    db: &DatabaseConnection,
    docker: &Docker,
    source_ip: &str,
) -> AwdpResult<(event_instances::Model, awdp_instances::Model)> {
    let network = docker
        .inspect_network(
            PRACTICE_NETWORK_NAME,
            None::<bollard::network::InspectNetworkOptions<String>>,
        )
        .await
        .map_err(|e| AwdpError::Docker(format!("inspect practice network: {e}")))?;
    let containers = network.containers.unwrap_or_default();
    let hit = containers
        .iter()
        .find(|(_, c)| {
            c.ipv4_address
                .as_deref()
                .map(|v| v.split('/').next().unwrap_or(v))
                == Some(source_ip)
        })
        .map(|(_, c)| c.name.clone().unwrap_or_default())
        .ok_or_else(|| AwdpError::Forbidden("unknown source ip".into()))?;
    let container_name = hit.trim_start_matches('/').to_string();

    let instance = event_instances::Entity::find()
        .filter(event_instances::Column::ContainerName.eq(&container_name))
        .one(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?
        .ok_or_else(|| AwdpError::Forbidden("unknown container".into()))?;
    if instance.runtime_state != "running" {
        return Err(AwdpError::Conflict("instance is not running".into()));
    }
    let ext = awdp_instances::Entity::find_by_id(instance.id)
        .one(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?
        .ok_or_else(|| AwdpError::Forbidden("awdp instance not found".into()))?;
    Ok((instance, ext))
}

/// data 网络 inspect 便捷封装（供实例 IP 等查询复用）。
pub async fn inspect_practice_network(docker: &Docker) -> AwdpResult<bollard::models::Network> {
    docker
        .inspect_network(
            PRACTICE_NETWORK_NAME,
            None::<bollard::network::InspectNetworkOptions<String>>,
        )
        .await
        .map_err(|e| AwdpError::Docker(format!("inspect practice network: {e}")))
}

/// 目标实例当前容器内网 IP（data 网络；运行中才返回）。
pub async fn instance_internal_ip(
    docker: &Docker,
    instance: &event_instances::Model,
) -> AwdpResult<Option<String>> {
    if instance.runtime_state != "running" {
        return Ok(None);
    }
    let network = inspect_practice_network(docker).await?;
    let containers = network.containers.unwrap_or_default();
    for (_, c) in containers.iter() {
        let name = c.name.clone().unwrap_or_default();
        if name.trim_start_matches('/') == instance.container_name {
            return Ok(c
                .ipv4_address
                .as_deref()
                .map(|v| v.split('/').next().unwrap_or(v).to_string()));
        }
    }
    Ok(None)
}

/// 占位：保持类型导入完整（Uuid 用于扩展签名）。
#[allow(dead_code)]
fn _uuid_placeholder(_: Uuid) {}
