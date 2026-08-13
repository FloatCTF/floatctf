//! AWDP source artifact 集成测试（Docker-gated）。
//!
//! 覆盖：从真实 GameBox 镜像导出 source_code_dir → 重打包 source.tar.gz 的端到端链路；
//! 包结构 = `src/`（源码）+ 根 `patch.sh` 通用模板；临时容器只 create 不 start、
//! 内容正确、根前缀剥除、临时容器移除；RustFS 上传 + digest 存储。
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

/// RustFS（S3 兼容）客户端；连不上时返回 None（测试跳过，不强求）。
async fn rustfs_or_skip() -> Option<aws_sdk_s3::Client> {
    let cfg = floatctf::core::config::StorageConfig {
        endpoint_url: "http://127.0.0.1:9000".into(),
        access_key_id: "rustfsadmin".into(),
        secret_access_key: floatctf::core::Secret::new("rustfsadmin"),
        region: "cn-east-1".into(),
    };
    match floatctf::infrastructure::storage::connect(&cfg).await {
        Ok(c) => Some(c),
        Err(e) => {
            eprintln!("skip: RustFS unreachable ({e})");
            None
        }
    }
}

#[tokio::test]
async fn extract_source_tar_from_real_gamebox_image() {
    let Some(rt) = docker_or_skip().await else {
        return;
    };
    let handle = rt
        .create_container(fcmc::ContainerSpec {
            network_aliases: vec![],
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

    // §78：临时容器只 create 不 start（create_container 是 create-only）。
    let state = rt
        .inspect_container(&handle.container_id)
        .await
        .expect("inspect temp container");
    assert!(
        !state.running,
        "temp container must be created but NOT started (status={})",
        state.status
    );

    rt.remove_container(&handle.container_id, true)
        .await
        .expect("remove temp container");
    // §78：临时容器移除后应不可再 inspect。
    let gone = rt.inspect_container(&handle.container_id).await;
    assert!(gone.is_err(), "temp container must be removed");
}

#[tokio::test]
async fn publish_artifact_stores_key_and_digest() {
    let Some(rt) = docker_or_skip().await else {
        return;
    };
    let Some(rustfs) = rustfs_or_skip().await else {
        return;
    };
    // 复用模块级 helper（create/copy/targz/remove 全链路）。
    let targz = floatctf::modules::gamebox::source_artifact::extract_awdp_source_targz(
        &rt,
        gamebox_image(),
        "awdp-src-itest-pub",
        "/var/www/html",
    )
    .await
    .expect("extract source tar.gz");
    assert!(!targz.is_empty());

    // §78：上传 private RustFS → 返回 (object_key, sha256 digest)，digest 与内容一致。
    let gb_id = uuid::Uuid::new_v4();
    let pkg_digest = "pkg-abc123";
    let (key, digest) = floatctf::modules::gamebox::source_artifact::publish_awdp_source_artifact(
        &rustfs, gb_id, pkg_digest, &targz,
    )
    .await
    .expect("publish source artifact");
    assert_eq!(
        key,
        format!("gameboxes/{gb_id}/awdp/{pkg_digest}/source.tar.gz"),
        "object key scoped by gamebox + package digest"
    );
    assert_eq!(digest.len(), 64, "digest must be sha256 hex");
    let expected = {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(&targz);
        hex::encode(h.finalize())
    };
    assert_eq!(digest, expected, "digest = sha256(source.tar.gz bytes)");

    // 对象确实落盘（head 成功），然后清理。
    let head = rustfs
        .head_object()
        .bucket(floatctf::modules::gamebox::source_artifact::AWDP_SOURCE_BUCKET)
        .key(&key)
        .send()
        .await
        .expect("head uploaded artifact");
    assert!(head.content_length().unwrap_or(0) > 0, "object stored");
    rustfs
        .delete_object()
        .bucket(floatctf::modules::gamebox::source_artifact::AWDP_SOURCE_BUCKET)
        .key(&key)
        .send()
        .await
        .expect("cleanup object");
}

#[tokio::test]
async fn extract_targz_via_service_helper() {
    let Some(rt) = docker_or_skip().await else {
        return;
    };
    // 复用模块级 helper（内部 create/copy/targz/remove 全链路）。
    let targz = floatctf::modules::gamebox::source_artifact::extract_awdp_source_targz(
        &rt,
        gamebox_image(),
        "awdp-src-itest2",
        "/var/www/html",
    )
    .await
    .expect("extract source tar.gz");
    assert!(!targz.is_empty());
    assert_eq!(&targz[..2], &[0x1f, 0x8b], "must be gzip");

    // §78：包结构 = src/（源码，根前缀剥除）+ 根 patch.sh 通用模板。
    let gz = flate2::read::GzDecoder::new(&targz[..]);
    let mut archive = tar::Archive::new(gz);
    let mut names: Vec<String> = Vec::new();
    let mut patch_content: Option<String> = None;
    for entry in archive.entries().expect("valid tar") {
        let mut e = entry.expect("tar entry");
        let name = e.path().unwrap().to_string_lossy().into_owned();
        names.push(name.clone());
        if name == "patch.sh" {
            let mut s = String::new();
            std::io::Read::read_to_string(&mut e, &mut s).unwrap();
            patch_content = Some(s);
        }
    }
    assert!(
        names.iter().any(|n| n == "src/index.php"),
        "expected src/index.php in {names:?}"
    );
    assert!(
        !names.iter().any(|n| n.starts_with("html")),
        "root prefix must be stripped into src/: {names:?}"
    );
    assert!(
        names.contains(&"patch.sh".to_string()),
        "expected root patch.sh template in {names:?}"
    );
    let patch = patch_content.expect("patch.sh content");
    assert!(
        patch.contains("通用 Patch 模板")
            && patch.lines().all(|l| l.trim().is_empty()
                || l.trim_start().starts_with('#')
                || l.trim_start().starts_with("#!/")),
        "patch.sh must be a comment-only generic template: {patch}"
    );
}

#[tokio::test]
async fn missing_source_dir_fails_with_clear_error() {
    let Some(rt) = docker_or_skip().await else {
        return;
    };
    let err = floatctf::modules::gamebox::source_artifact::extract_awdp_source_targz(
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
