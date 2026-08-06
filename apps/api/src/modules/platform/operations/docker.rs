use bollard::query_parameters::{
    ListImagesOptionsBuilder, ListNetworksOptionsBuilder, RemoveContainerOptions,
    StartContainerOptions, StopContainerOptions,
};

use serde::{Deserialize, Serialize};

use crate::api::prelude::*;

// ─────────────────────────────────────────────────────────────────────────────
// Container Types
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct FloatDockerContainer {
    pub id: String,
    pub name: String,
    pub status: String,
    pub image: String,
    pub ports: String,
    pub created: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ImageInfo {
    pub id: String,
    pub repo_tags: Vec<String>,
    pub size: i64,
    pub created: i64,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct NetworkInfo {
    pub id: String,
    pub name: String,
    pub driver: String,
    pub scope: String,
    pub ipam_driver: String,
    pub subnet: Option<String>,
    pub gateway: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Container API Endpoints
// ─────────────────────────────────────────────────────────────────────────────

/// GET /api/admin/docker/containers
#[get("/containers")]
pub async fn get_containers(
    _user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    query_params: Query<QueryParams>,
) -> UniResult<Vec<FloatDockerContainer>> {
    let query_params = query_params.0;

    use bollard::query_parameters::ListContainersOptionsBuilder;
    let list_opts = ListContainersOptionsBuilder::default().all(true).build();
    let all_containers: Vec<_> = ctx
        .docker
        .list_containers(Some(list_opts))
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    let result: Vec<FloatDockerContainer> = all_containers
        .into_iter()
        .map(|container| {
            let ports = container
                .ports
                .as_ref()
                .map(|p| {
                    p.iter()
                        .map(|port| match port.public_port {
                            Some(p) => format!(
                                "{}:{}",
                                port.ip.as_ref().unwrap_or(&"0.0.0.0".to_string()),
                                p
                            ),
                            None => format!("{}", port.private_port),
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();

            FloatDockerContainer {
                id: container.id.unwrap_or_default(),
                name: container
                    .names
                    .as_ref()
                    .and_then(|n| n.first())
                    .map(|s| s.trim_start_matches('/').to_string())
                    .unwrap_or_default(),
                status: container
                    .state
                    .map(|s| format!("{:?}", s))
                    .unwrap_or_else(|| "unknown".to_string()),
                image: container.image.unwrap_or_default(),
                ports,
                created: container.created.unwrap_or(0) as i64,
            }
        })
        .collect();

    let total_items = result.len();
    let offset = query_params.offset.unwrap_or(0) as usize;
    let limit = query_params.limit.unwrap_or(50) as usize;

    let items: Vec<FloatDockerContainer> = result.into_iter().skip(offset).take(limit).collect();

    let mut meta = query_params;
    meta.total = Some(total_items);

    UniResponse::ok_meta(items.into(), meta.into()).into()
}

/// POST /api/admin/docker/containers/{container_id}/stop
#[post("/containers/{container_id}/stop")]
pub async fn stop_container(
    user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    container_id: Path<String>,
) -> UniResult<()> {
    let user = user.into_inner();
    let container_id = container_id.into_inner();
    let options = StopContainerOptions {
        t: Some(0),
        ..Default::default()
    };
    ctx.docker
        .stop_container(&container_id, Some(options))
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    ctx.log
        .add_log(
            "INFO",
            "DOCKER",
            "STOP_CONTAINER",
            format!("{} 停止容器: {}", user.username, container_id).as_str(),
            json!({"container_id": container_id}),
            None,
            user.id.into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok_none().into()
}

/// POST /api/admin/docker/containers/{container_id}/start
#[post("/containers/{container_id}/start")]
pub async fn start_container(
    user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    container_id: Path<String>,
) -> UniResult<()> {
    let user = user.into_inner();
    let container_id = container_id.into_inner();
    ctx.docker
        .start_container(&container_id, None::<StartContainerOptions>)
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    ctx.log
        .add_log(
            "INFO",
            "DOCKER",
            "START_CONTAINER",
            format!("{} 启动容器: {}", user.username, container_id).as_str(),
            json!({"container_id": container_id}),
            None,
            user.id.into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok_none().into()
}

/// DELETE /api/admin/docker/containers/{container_id}
#[delete("/containers/{container_id}")]
pub async fn delete_container(
    user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    container_id: Path<String>,
) -> UniResult<()> {
    let user = user.into_inner();
    let container_id = container_id.into_inner();
    let options = RemoveContainerOptions {
        v: true,
        force: true,
        link: false,
    };
    ctx.docker
        .remove_container(&container_id, Some(options))
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    ctx.log
        .add_log(
            "INFO",
            "DOCKER",
            "DELETE_CONTAINER",
            format!("{} 删除容器: {}", user.username, container_id).as_str(),
            json!({"container_id": container_id}),
            None,
            user.id.into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok_none().into()
}

// ─────────────────────────────────────────────────────────────────────────────
// Image API Endpoints
// ─────────────────────────────────────────────────────────────────────────────

/// GET /api/admin/docker/images
#[get("/images")]
pub async fn get_images(
    _user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    query_params: Query<QueryParams>,
) -> UniResult<Vec<ImageInfo>> {
    let query_params = query_params.0;

    let images = ctx
        .docker
        .list_images(Some(ListImagesOptionsBuilder::default().build()))
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    let result: Vec<ImageInfo> = images
        .into_iter()
        .map(|img| ImageInfo {
            id: img.id,
            repo_tags: img.repo_tags,
            size: img.size as i64,
            created: img.created,
        })
        .collect();

    let total_items = result.len();
    let offset = query_params.offset.unwrap_or(0) as usize;
    let limit = query_params.limit.unwrap_or(50) as usize;

    let items: Vec<ImageInfo> = result.into_iter().skip(offset).take(limit).collect();

    let mut meta = query_params;
    meta.total = Some(total_items);

    UniResponse::ok_meta(items.into(), meta.into()).into()
}

/// DELETE /api/admin/docker/images/{image_id}
#[delete("/images/{image_id}")]
pub async fn delete_image(
    user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    image_id: Path<String>,
) -> UniResult<()> {
    let user = user.into_inner();
    let image_id = image_id.into_inner();
    use bollard::auth::DockerCredentials;
    use bollard::query_parameters::{
        RemoveContainerOptions, RemoveImageOptions, StartContainerOptions, StopContainerOptions,
    };
    ctx.docker
        .remove_image(
            &image_id,
            None::<RemoveImageOptions>,
            None::<DockerCredentials>,
        )
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    ctx.log
        .add_log(
            "INFO",
            "DOCKER",
            "DELETE_IMAGE",
            format!("{} 删除镜像: {}", user.username, image_id).as_str(),
            json!({"image_id": image_id}),
            None,
            user.id.into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok_none().into()
}

// ─────────────────────────────────────────────────────────────────────────────
// Network API Endpoints
// ─────────────────────────────────────────────────────────────────────────────

/// GET /api/admin/docker/networks
#[get("/networks")]
pub async fn get_networks(
    _user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    query_params: Query<QueryParams>,
) -> UniResult<Vec<NetworkInfo>> {
    let query_params = query_params.0;

    let networks = ctx
        .docker
        .list_networks(Some(ListNetworksOptionsBuilder::default().build()))
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    let result: Vec<NetworkInfo> = networks
        .into_iter()
        .map(|net| {
            let (subnet, gateway) = net
                .ipam
                .as_ref()
                .and_then(|ipam| {
                    ipam.config.as_ref().and_then(|configs| {
                        configs
                            .first()
                            .map(|config| (config.subnet.clone(), config.gateway.clone()))
                    })
                })
                .unwrap_or((None, None));

            NetworkInfo {
                id: net.id.unwrap_or_default(),
                name: net.name.unwrap_or_default(),
                driver: net.driver.unwrap_or_default(),
                scope: net.scope.unwrap_or_default(),
                ipam_driver: net
                    .ipam
                    .as_ref()
                    .and_then(|ipam| ipam.driver.clone())
                    .unwrap_or_default(),
                subnet,
                gateway,
            }
        })
        .collect();

    let total_items = result.len();
    let offset = query_params.offset.unwrap_or(0) as usize;
    let limit = query_params.limit.unwrap_or(50) as usize;

    let items: Vec<NetworkInfo> = result.into_iter().skip(offset).take(limit).collect();

    let mut meta = query_params;
    meta.total = Some(total_items);

    UniResponse::ok_meta(items.into(), meta.into()).into()
}

/// POST /api/admin/docker/networks
#[post("/networks")]
pub async fn create_network(
    user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    body: Json<CreateNetworkRequest>,
) -> UniResult<NetworkInfo> {
    let user = user.into_inner();
    let body = body.into_inner();

    #[allow(deprecated)]
    let config = bollard::network::CreateNetworkOptions {
        name: body.name.clone(),
        driver: body.driver.unwrap_or_else(|| "bridge".to_string()),
        ipam: bollard::secret::Ipam {
            config: Some(vec![bollard::secret::IpamConfig {
                subnet: Some(body.subnet),
                gateway: Some(body.gateway),
                ..Default::default()
            }]),
            ..Default::default()
        },
        ..Default::default()
    };

    ctx.docker
        .create_network(config)
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    ctx.log
        .add_log(
            "INFO",
            "DOCKER",
            "CREATE_NETWORK",
            format!("{} 创建网络: {}", user.username, body.name).as_str(),
            json!({"name": body.name}),
            None,
            user.id.into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok(
        NetworkInfo {
            name: body.name,
            ..Default::default()
        }
        .into(),
    )
    .into()
}

/// DELETE /api/admin/docker/networks/{network_id}
#[delete("/networks/{network_id}")]
pub async fn delete_network(
    user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    network_id: Path<String>,
) -> UniResult<()> {
    let user = user.into_inner();
    let network_id = network_id.into_inner();
    ctx.docker
        .remove_network(&network_id)
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    ctx.log
        .add_log(
            "INFO",
            "DOCKER",
            "DELETE_NETWORK",
            format!("{} 删除网络: {}", user.username, network_id).as_str(),
            json!({"network_id": network_id}),
            None,
            user.id.into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok_none().into()
}

#[derive(Debug, Deserialize)]
pub struct CreateNetworkRequest {
    pub name: String,
    pub subnet: String,
    pub gateway: String,
    pub driver: Option<String>,
}
