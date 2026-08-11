//! AWDP source artifact 集成测试（Docker-gated）。
//!
//! 覆盖：从真实 GameBox 镜像导出 source_code_dir → tar → zip 的端到端链路。
//! 依赖本地镜像 `floatctf/gameboxes/test-gg:1.0.2`（test_g 示例包构建产物，
//! source_code_dir=/var/www/html）。无该镜像时跳过。

use bollard::Docker;
use fcmc::{ContainerRuntime, DockerContainerRuntime, ImageRuntime};

fn gamebox_image() -> &'static str {
    "floatctf/gameboxes/test-gg:1.0.2"
}

async fn docker_or_skip() -> Option<DockerContainerRuntime> {
    let docker = Docker::connect_with_local_defaults().ok()?;
    let rt = DockerContainerRuntime::new(docker);
    if ImageRuntime::inspect_image(&rt, gamebox_image())
        .await
        .is_err()
    {
        eprintln!("skip: image {} not present", gamebox_image());
        return None;
    }
    Some(rt)
}

#[tokio::test]
async fn extract_source_zip_from_real_gamebox_image() {
    let Some(rt) = docker_or_skip().await else {
        return;
    };
    let handle = rt
        .create_container(fcmc::ContainerSpec {
            name: "awdp-src-itest".into(),
            image: gamebox_image().into(),
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
        .expect("create temp container");

    let tar = rt
        .copy_from_container(&handle.container_id, "/var/www/html")
        .await
        .expect("copy /var/www/html");
    assert!(!tar.is_empty());
    assert!(tar.len() >= 262 && &tar[257..262] == b"ustar");

    rt.remove_container(&handle.container_id, true)
        .await
        .expect("remove temp container");
}

#[tokio::test]
async fn extract_zip_via_service_helper() {
    let Some(rt) = docker_or_skip().await else {
        return;
    };
    // 复用模块级 helper（内部 create/copy/zip/remove 全链路）。
    let zip = floatctf::modules::gamebox::source_artifact::extract_awdp_source_zip(
        &rt,
        gamebox_image(),
        "awdp-src-itest2",
        "/var/www/html",
    )
    .await
    .expect("extract source zip");
    assert!(!zip.is_empty());

    let mut reader = zip::ZipArchive::new(std::io::Cursor::new(zip)).expect("valid zip");
    let names: Vec<String> = (0..reader.len())
        .map(|i| reader.by_index(i).unwrap().name().to_string())
        .collect();
    assert!(
        names.contains(&"index.php".to_string()),
        "expected index.php in {names:?}"
    );
    assert!(
        !names.iter().any(|n| n.starts_with("html")),
        "root prefix must be stripped: {names:?}"
    );
}

#[tokio::test]
async fn missing_source_dir_fails_with_clear_error() {
    let Some(rt) = docker_or_skip().await else {
        return;
    };
    let err = floatctf::modules::gamebox::source_artifact::extract_awdp_source_zip(
        &rt,
        gamebox_image(),
        "awdp-src-itest3",
        "/definitely-not-in-image",
    )
    .await
    .expect_err("missing dir must fail");
    assert!(
        err.to_string().contains("SOURCE_EXTRACT"),
        "unexpected error: {err}"
    );
}
