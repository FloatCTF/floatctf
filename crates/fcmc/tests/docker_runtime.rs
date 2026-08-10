//! Docker runtime integration tests.
//!
//! These tests require a running Docker daemon and are ignored by default.
//! Run with: cargo test --test docker_runtime -- --ignored

use bollard::Docker;
use fcmc::{
    ChallengeMeta, ContainerRuntime, DockerContainerRuntime, GameBoxMeta, NetworkSpec,
    RecommendedResources,
};
use std::path::Path;

#[tokio::test]
#[ignore = "requires Docker"]
async fn docker_create_and_start_challenge() {
    let content = std::fs::read_to_string(Path::new("tests/fixtures/challenge/meta.toml")).unwrap();
    let cm = ChallengeMeta::from_toml_str(&content).unwrap();
    let docker = Docker::connect_with_local_defaults().unwrap();
    let rt = DockerContainerRuntime::new(docker.clone());

    let flag = "flag{docker-test}";
    let spec = fcmc::ContainerSpec {
        name: "fcmc-test-challenge".into(),
        image: cm.docker.as_ref().unwrap().image_tag.clone(),
        env: vec![format!("{}={}", cm.flag.env_var, flag)],
        labels: Default::default(),
        network_name: None,
        fixed_ip: None,
        port_bindings: vec![fcmc::PortBinding {
            container_port: cm.docker.as_ref().unwrap().port.clone(),
            host_ip: Some("0.0.0.0".into()),
            host_port: None,
        }],
        auto_remove: true,
        resources: fcmc::ResourceLimits::default(),
        network_mode: None,
        healthcheck: None,
    };

    let handle = rt.create_and_start(spec).await.unwrap();
    assert!(!handle.container_id.is_empty());

    let state = rt.inspect_container(&handle.container_id).await.unwrap();
    assert!(state.running);

    rt.stop_and_remove(&handle.container_id, std::time::Duration::from_secs(0))
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "requires Docker"]
async fn docker_create_network_and_container() {
    let docker = Docker::connect_with_local_defaults().unwrap();
    let rt = DockerContainerRuntime::new(docker);

    let net = rt
        .create_network(NetworkSpec {
            name: "fcmc-test-net".into(),
            subnet_cidr: "172.30.0.0/16".into(),
            internal: true,
            bridge_name: Some("fcmc-test-net".into()),
            check_duplicate: true,
        })
        .await
        .unwrap();

    assert!(!net.network_id.is_empty());

    // Cleanup
    rt.remove_network(&net.network_id).await.unwrap();
}

#[tokio::test]
#[ignore = "requires Docker"]
async fn docker_gamebox_create_and_start() {
    let content = std::fs::read_to_string(Path::new("tests/fixtures/gamebox/meta.toml")).unwrap();
    let meta = GameBoxMeta::parse_and_validate(&content).unwrap();
    let docker = Docker::connect_with_local_defaults().unwrap();
    let rt = DockerContainerRuntime::new(docker);

    // Image ref is platform-resolved (not in meta). Integration test uses a placeholder tag.
    let image_ref = fcmc::build_gamebox_image_ref(
        "floatctf",
        &meta.resolved_safe_name().unwrap(),
        &meta.version,
    );
    let res = meta
        .gamebox
        .recommended_resources
        .clone()
        .unwrap_or_else(RecommendedResources::default);

    let spec = fcmc::ContainerSpec {
        name: "fcmc-test-gamebox".into(),
        image: image_ref,
        env: vec![
            format!("CTF_USER={}", meta.gamebox.username),
            "CTF_PASSWORD=testpass".into(),
        ],
        labels: fcmc::awd_labels(
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            1,
            "gamebox",
        ),
        network_name: None,
        fixed_ip: None,
        port_bindings: vec![],
        auto_remove: true,
        resources: fcmc::ResourceLimits {
            cpu_millis: Some(res.cpu_millis),
            memory_bytes: Some(res.memory_bytes),
            pids_limit: Some(res.pids_limit),
            cap_drop: vec![
                "NET_ADMIN".to_string(),
                "NET_RAW".to_string(),
                "SYS_ADMIN".to_string(),
            ],
            privileged: false,
            extra_hosts: vec![],
        },
        network_mode: None,
        healthcheck: None,
    };

    let handle = rt.create_and_start(spec).await.unwrap();
    assert!(!handle.container_id.is_empty());

    let state = rt.inspect_container(&handle.container_id).await.unwrap();
    assert!(state.running);

    rt.stop_and_remove(&handle.container_id, std::time::Duration::from_secs(0))
        .await
        .unwrap();
}
