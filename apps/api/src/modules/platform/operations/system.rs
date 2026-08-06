use crate::api::prelude::*;
use actix_web::get;
use bollard::Docker;

#[allow(deprecated)]
use bollard::container::ListContainersOptions;
#[allow(deprecated)]
use bollard::image::ListImagesOptions;
use pnet::datalink;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use sysinfo::{Components, Disks, Networks, System};

#[derive(Debug, Serialize, Deserialize)]
pub struct SystemInformation {
    pub name: Option<String>,
    pub kernel_version: Option<String>,
    pub os_version: Option<String>,
    pub host_name: Option<String>,
    pub uptime: u64,
    pub total_memory: u64,
    pub used_memory: u64,
    pub total_swap: u64,
    pub used_swap: u64,
    pub avg_temp: f32,
    pub max_temp: f32,
    pub nb_cpu: usize,
    pub disks_info: Vec<DiskInformation>,
    pub network_interfaces: Vec<NetworkInterfaceInfo>,
    pub docker_info: DockerInformation, // 新增 docker 信息
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DiskInformation {
    pub name: String,
    pub mount_point: String,
    pub file_system: String,
    pub total_space: f64,
    pub available_space: f64,
    pub used_space: f64,
    pub usage_percent: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NetworkInterfaceInfo {
    pub name: String,
    pub ip_addresses: Vec<String>,
    pub received: u64,
    pub transmitted: u64,
    pub recv_rate: u64,
    pub transmit_rate: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DockerImageInfo {
    pub id: String,
    pub repo_tags: Vec<String>,
    pub size: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DockerInformation {
    pub image_count: usize,
    pub images: Vec<DockerImageInfo>,
    pub running_container_count: usize,
    pub total_disk: u64,
}
/// GET /api/admin/monitor
#[get("/monitor")]
pub async fn get_sys_info(_: SuperAdminJwtGuard) -> UniResult<SystemInformation> {
    // ---------- 系统信息 ----------
    let mut sys = System::new_all();
    sys.refresh_all();
    let nb_cpu = sys.cpus().len();
    let total_memory = sys.total_memory();
    let used_memory = sys.used_memory();
    let total_swap = sys.total_swap();
    let used_swap = sys.used_swap();

    // 温度
    let temps: Vec<f32> = Components::new_with_refreshed_list()
        .iter()
        .filter_map(|c| c.temperature())
        .filter(|t| *t > 0.0)
        .collect();
    let avg_temp = if !temps.is_empty() {
        temps.iter().sum::<f32>() / temps.len() as f32
    } else {
        0.0
    };
    let max_temp = temps.iter().cloned().fold(0.0, f32::max);

    // 磁盘
    let mut disks_info = vec![];
    let disks = Disks::new_with_refreshed_list();
    let mut shown_devices = HashSet::new();

    for disk in &disks {
        let device_name = disk.name().to_string_lossy().to_string();
        if shown_devices.contains(&device_name) || device_name.contains("overlay") {
            continue;
        }
        shown_devices.insert(device_name.clone());

        let mount_point = disk.mount_point().to_string_lossy();
        let file_system = disk.file_system().to_string_lossy();
        let total = disk.total_space();
        let available = disk.available_space();
        let used = total - available;
        let usage_percent = if total > 0 {
            (used as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        disks_info.push(DiskInformation {
            name: device_name,
            mount_point: mount_point.to_string(),
            file_system: file_system.to_string(),
            total_space: total as f64 / 1e9,
            available_space: available as f64 / 1e9,
            used_space: used as f64 / 1e9,
            usage_percent,
        });
    }
    disks_info.sort_by(|a, b| a.name.cmp(&b.name));

    // 网络
    let networks = Networks::new_with_refreshed_list();
    let mut first_sample = HashMap::new();
    for (iface, data) in &networks {
        first_sample.insert(iface, (data.total_received(), data.total_transmitted()));
    }
    sys.refresh_all();
    let ip_map = datalink::interfaces()
        .into_iter()
        .map(|iface| {
            let ips = iface
                .ips
                .iter()
                .map(|ip| ip.ip().to_string())
                .collect::<Vec<_>>();
            (iface.name, ips)
        })
        .collect::<HashMap<_, _>>();
    let mut network_interfaces = vec![];
    for (iface, data) in &networks {
        let (recv, transmit) = first_sample.get(iface).unwrap();
        let recv_rate = data.total_received() - *recv;
        let transmit_rate = data.total_transmitted() - *transmit;
        let ip_addresses = ip_map.get(iface).cloned().unwrap_or_default();
        network_interfaces.push(NetworkInterfaceInfo {
            name: iface.to_string(),
            ip_addresses,
            received: data.total_received(),
            transmitted: data.total_transmitted(),
            recv_rate,
            transmit_rate,
        });
    }
    network_interfaces.sort_by(|a, b| a.name.cmp(&b.name));

    // ---------- Docker 信息 ----------
    let docker = Docker::connect_with_local_defaults().unwrap();

    // 镜像
    #[allow(deprecated)]
    let images = docker
        .list_images(Some(ListImagesOptions::<String> {
            all: true,
            ..Default::default()
        }))
        .await
        .unwrap_or_default();
    let images_info: Vec<DockerImageInfo> = images
        .iter()
        .map(|img| DockerImageInfo {
            id: img.id.clone(),
            repo_tags: img.repo_tags.clone(),
            size: img.size as u64,
        })
        .collect();

    // 正在运行的容器数量
    #[allow(deprecated)]
    let running_containers = docker
        .list_containers(Some(ListContainersOptions::<String> {
            all: false, // 只获取运行中的
            ..Default::default()
        }))
        .await
        .unwrap_or_default();

    // 磁盘占用
    let df = docker.df(None).await.unwrap_or_default();
    let total_disk = df.layers_size.unwrap_or(0) as u64;

    let docker_info = DockerInformation {
        image_count: images_info.len(),
        images: images_info,
        running_container_count: running_containers.len(),
        total_disk,
    };
    let uptime = sysinfo::System::uptime();
    // ---------- 返回 ----------
    UniResponse::ok(
        SystemInformation {
            avg_temp,
            max_temp,
            uptime,
            nb_cpu,
            name: System::name(),
            kernel_version: System::kernel_version(),
            os_version: System::os_version(),
            host_name: System::host_name(),
            total_memory,
            used_memory,
            total_swap,
            used_swap,
            disks_info,
            network_interfaces,
            docker_info,
        }
        .into(),
    )
    .into()
}

#[get("/changelog")]
pub async fn get_changelog(_: SuperAdminJwtGuard) -> UniResult<String> {
    let changelog_path =
        std::env::var("SYSTEM_CHANGELOG_PATH").unwrap_or_else(|_| "./CHANGELOG.md".to_string());
    let changelog = std::fs::read_to_string(changelog_path).unwrap_or_default();
    let changelog = format!(
        "floatctf-api: {}\n{}",
        env!("CARGO_PKG_VERSION"),
        &changelog
    );
    UniResponse::ok(changelog.into()).into()
}
