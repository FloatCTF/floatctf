//! Docker 运行时集成测试。

use bollard::Docker;
use fcmc::{
    ArtifactKind, ChallengeMeta, ContainerRuntime, DockerContainerRuntime, GameBoxMeta,
    NetworkSpec, RecommendedResources, build_artifact_image_ref,
};
use std::path::Path;

#[tokio::test]
#[ignore = "requires Docker"]
async fn docker_create_and_start_challenge() {
    let content = std::fs::read_to_string(Path::new("tests/fixtures/challenge/meta.toml")).unwrap();
    let cm = ChallengeMeta::parse_and_validate(&content).unwrap();
    let docker = Docker::connect_with_local_defaults().unwrap();
    let rt = DockerContainerRuntime::new(docker.clone());

    // Image ref is platform-resolved (not in meta): floatctf/challenges/<safe>:<version>.
    let image_ref = build_artifact_image_ref(
        ArtifactKind::Challenge,
        "floatctf",
        &cm.resolved_safe_name().unwrap(),
        &cm.version,
    );
    let docker_meta = cm.docker.as_ref().unwrap();

    let flag = "flag{docker-test}";
    let spec = fcmc::ContainerSpec {
        name: "fcmc-test-challenge".into(),
        image: image_ref,
        // v1 fixture is dynamic → platform injects FLAG env.
        env: vec![format!("FLAG={flag}")],
        labels: Default::default(),
        network_name: None,
        fixed_ip: None,
        port_bindings: vec![fcmc::PortBinding {
            container_port: format!("{}/tcp", docker_meta.port),
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
            format!("GAMEBOX_USERNAME={}", meta.gamebox.username),
            "GAMEBOX_USERPASS=testpass".into(),
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

fn test_image() -> &'static str {
    "busybox:latest"
}

/// test_g GameBox（php:apache 长驻进程，需 GAMEBOX_USERNAME/PASS）。
fn gamebox_image() -> &'static str {
    "floatctf/gameboxes/test-gg:1.0.2"
}

async fn ensure_test_image() {
    use fcmc::ImageRuntime;
    let docker = Docker::connect_with_local_defaults().unwrap();
    let rt = DockerContainerRuntime::new(docker);
    if ImageRuntime::inspect_image(&rt, test_image())
        .await
        .is_err()
    {
        ImageRuntime::pull_image(&rt, test_image(), None::<fcmc::RegistryAuth>.as_ref())
            .await
            .unwrap();
    }
}

#[tokio::test]
#[ignore = "requires Docker"]
async fn docker_exec_captures_output_and_exit_code() {
    ensure_test_image().await;
    let docker = Docker::connect_with_local_defaults().unwrap();
    let rt = DockerContainerRuntime::new(docker);

    let handle = rt
        .create_and_start(fcmc::ContainerSpec {
            name: "fcmc-test-exec".into(),
            image: gamebox_image().into(),
            env: vec![
                "FLOATCTF_SOURCE_DIR=/tmp".into(),
                "GAMEBOX_USERNAME=ctf".into(),
                "GAMEBOX_USERPASS=testpass".into(),
            ],
            labels: Default::default(),
            network_name: None,
            fixed_ip: None,
            port_bindings: vec![],
            auto_remove: true,
            resources: fcmc::ResourceLimits::default(),
            network_mode: None,
            healthcheck: None,
        })
        .await
        .unwrap();

    // Success path: stdout captured, env injected, exit 0.
    let ok = rt
        .exec(
            &handle.container_id,
            fcmc::ExecOptions {
                cmd: vec![
                    "/bin/sh".into(),
                    "-c".into(),
                    "echo out=$FLOATCTF_SOURCE_DIR".into(),
                ],
                env: vec![],
                workdir: None,
                timeout: std::time::Duration::from_secs(15),
                stdout_limit: 64 * 1024,
                stderr_limit: 64 * 1024,
            },
        )
        .await
        .unwrap();
    assert_eq!(ok.exit_code, Some(0));
    assert!(ok.stdout.contains("out=/tmp"), "stdout={:?}", ok.stdout);
    assert!(!ok.timed_out);

    // Failure path: nonzero exit code + stderr captured.
    let err = rt
        .exec(
            &handle.container_id,
            fcmc::ExecOptions {
                cmd: vec![
                    "/bin/sh".into(),
                    "-c".into(),
                    "echo boom >&2; exit 7".into(),
                ],
                env: vec![],
                workdir: None,
                timeout: std::time::Duration::from_secs(15),
                stdout_limit: 64 * 1024,
                stderr_limit: 64 * 1024,
            },
        )
        .await
        .unwrap();
    assert_eq!(err.exit_code, Some(7));
    assert!(err.stderr.contains("boom"), "stderr={:?}", err.stderr);

    // Missing binary → exit code 127.
    let missing = rt
        .exec(
            &handle.container_id,
            fcmc::ExecOptions {
                cmd: vec!["no-such-binary-xyz".into()],
                env: vec![],
                workdir: None,
                timeout: std::time::Duration::from_secs(15),
                stdout_limit: 64 * 1024,
                stderr_limit: 64 * 1024,
            },
        )
        .await
        .unwrap();
    assert_eq!(missing.exit_code, Some(127));

    rt.stop_and_remove(&handle.container_id, std::time::Duration::from_secs(0))
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "requires Docker"]
async fn docker_copy_from_container_returns_tar() {
    ensure_test_image().await;
    let docker = Docker::connect_with_local_defaults().unwrap();
    let rt = DockerContainerRuntime::new(docker);

    // create only — never started (source extraction pattern).
    let handle = rt
        .create_container(fcmc::ContainerSpec {
            name: "fcmc-test-copy".into(),
            image: test_image().into(),
            env: vec![],
            labels: Default::default(),
            network_name: None,
            fixed_ip: None,
            port_bindings: vec![],
            auto_remove: true,
            resources: fcmc::ResourceLimits::default(),
            network_mode: None,
            healthcheck: None,
        })
        .await
        .unwrap();

    let tar = rt
        .copy_from_container(&handle.container_id, "/etc")
        .await
        .unwrap();
    assert!(!tar.is_empty());
    // tar magic: ustar at offset 257.
    assert!(tar.len() >= 262 && &tar[257..262] == b"ustar");

    // Missing path → empty archive → error.
    let err = rt
        .copy_from_container(&handle.container_id, "/definitely-not-here-xyz")
        .await;
    assert!(err.is_err(), "missing path should error, got {:?}", err);

    rt.remove_container(&handle.container_id, true)
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "requires Docker"]
async fn docker_restart_preserves_writable_layer() {
    ensure_test_image().await;
    let docker = Docker::connect_with_local_defaults().unwrap();
    let rt = DockerContainerRuntime::new(docker);

    let handle = rt
        .create_and_start(fcmc::ContainerSpec {
            name: "fcmc-test-restart".into(),
            image: gamebox_image().into(),
            env: vec![
                "GAMEBOX_USERNAME=ctf".into(),
                "GAMEBOX_USERPASS=testpass".into(),
            ],
            labels: Default::default(),
            network_name: None,
            fixed_ip: None,
            port_bindings: vec![],
            auto_remove: true,
            resources: fcmc::ResourceLimits::default(),
            network_mode: None,
            healthcheck: None,
        })
        .await
        .unwrap();

    // Write a marker into the writable layer.
    let w = rt
        .exec(
            &handle.container_id,
            fcmc::ExecOptions {
                cmd: vec![
                    "/bin/sh".into(),
                    "-c".into(),
                    "echo patched > /tmp/marker".into(),
                ],
                env: vec![],
                workdir: None,
                timeout: std::time::Duration::from_secs(15),
                stdout_limit: 64 * 1024,
                stderr_limit: 64 * 1024,
            },
        )
        .await
        .unwrap();
    assert_eq!(w.exit_code, Some(0));

    // restart same container → marker survives.
    rt.restart_container(&handle.container_id, std::time::Duration::from_secs(5))
        .await
        .unwrap();
    let r = rt
        .exec(
            &handle.container_id,
            fcmc::ExecOptions {
                cmd: vec!["cat".into(), "/tmp/marker".into()],
                env: vec![],
                workdir: None,
                timeout: std::time::Duration::from_secs(15),
                stdout_limit: 64 * 1024,
                stderr_limit: 64 * 1024,
            },
        )
        .await
        .unwrap();
    assert_eq!(r.exit_code, Some(0));
    assert!(r.stdout.contains("patched"), "stdout={:?}", r.stdout);

    rt.stop_and_remove(&handle.container_id, std::time::Duration::from_secs(0))
        .await
        .unwrap();
}
