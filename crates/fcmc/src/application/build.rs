//! 包构建用例。

use std::path::Path;

use anyhow::{Context, Result};
use bollard::Docker;

use crate::metadata::{
    ArtifactKind, ChallengeMeta, GameBoxMeta, build_artifact_image_ref, build_gamebox_image_ref,
};
use crate::runtime::{DockerContainerRuntime, ImageBuildRequest, ImageRuntime};

/// CLI 未提供 `-t/--tag` 时使用的默认 registry 前缀。
/// 平台导入必须从配置传入显式 tag——禁止在 API 代码中写死。
pub const DEFAULT_CLI_REGISTRY_PREFIX: &str = "floatctf";

/// 构建 Challenge Docker 镜像。
///
/// 镜像 tag **不再**写在 `meta.toml`。解析顺序：
/// 1. Explicit `tag` argument (CLI `-t/--tag` or caller-supplied)
/// 2. Constructed via [`build_artifact_image_ref`] with
///    `registry_prefix = "floatctf"` (CLI default only) + resolved `safe_name` + `version`
///
/// 仅 `src/` 为 build context——排除 `meta.toml` 与 `attachment/`。
///
/// `proxy` accepts `[ip:]port`（缺省 ip 用 `host.docker.internal`）；`None` 时不注入代理。
pub async fn build_challenge(dir: &Path, tag: Option<&str>, proxy: Option<&str>) -> Result<()> {
    let meta_path = dir.join("meta.toml");
    let content = std::fs::read_to_string(&meta_path).context("Failed to read meta.toml")?;

    let cfg = ChallengeMeta::parse_and_validate(&content).context("Invalid challenge meta.toml")?;

    let image_tag = match tag {
        Some(t) if !t.trim().is_empty() => t.to_string(),
        _ => {
            let safe = cfg
                .resolved_safe_name()
                .context("Cannot derive safe_name for default image tag")?;
            build_artifact_image_ref(
                ArtifactKind::Challenge,
                DEFAULT_CLI_REGISTRY_PREFIX,
                &safe,
                &cfg.version,
            )
        }
    };

    let src_dir = dir.join("src");
    if !src_dir.exists() {
        anyhow::bail!("Source directory not found: {:?}", src_dir);
    }
    if !src_dir.join("Dockerfile").exists() {
        anyhow::bail!("Dockerfile not found: {:?}", src_dir.join("Dockerfile"));
    }

    let docker = Docker::connect_with_defaults().context("Failed to connect to Docker")?;
    let rt = DockerContainerRuntime::new(docker);

    // Note: only `src/` is the build context — meta.toml / attachment/ are excluded.
    println!("[fcmc] 开始构建挑战镜像");
    println!("  context: {:?}", src_dir);
    println!("  target : {}", image_tag);
    let mut req = ImageBuildRequest::new(&src_dir, image_tag).with_verbose(true);
    if let Some(proxy) = resolve_build_proxy(proxy) {
        println!("  proxy  : {}", proxy);
        req = req.with_proxy(proxy);
    }
    let result = ImageRuntime::build_image(&rt, req)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;

    println!("[fcmc] 构建完成");
    println!("  image_id  : {}", result.image_id);
    println!("  target_ref: {}", result.target_ref);

    tracing::info!(
        target: "fcmc::build",
        image_id = %result.image_id,
        target_ref = %result.target_ref,
        "challenge image built"
    );

    Ok(())
}

/// 构建 GameBox Docker 镜像。
///
/// 镜像 tag **不再**写在 `meta.toml`。解析顺序：
/// 1. Explicit `tag` argument (CLI `-t/--tag` or caller-supplied)
/// 2. Constructed via [`build_gamebox_image_ref`] with
///    `registry_prefix = "floatctf"` (CLI default only) + resolved `safe_name` + `version`
///
/// 平台/API 导入必须始终从平台配置提供显式 tag。
///
/// `proxy` accepts `[ip:]port`（缺省 ip 用 `host.docker.internal`）；`None` 时不注入代理。
pub async fn build_gamebox(dir: &Path, tag: Option<&str>, proxy: Option<&str>) -> Result<()> {
    let meta_path = dir.join("meta.toml");
    let content = std::fs::read_to_string(&meta_path).context("Failed to read meta.toml")?;

    let cfg = GameBoxMeta::parse_and_validate(&content).context("Invalid gamebox meta.toml")?;

    let image_tag = match tag {
        Some(t) if !t.trim().is_empty() => t.to_string(),
        _ => {
            let safe = cfg
                .resolved_safe_name()
                .context("Cannot derive safe_name for default image tag")?;
            build_gamebox_image_ref(DEFAULT_CLI_REGISTRY_PREFIX, &safe, &cfg.version)
        }
    };

    let src_dir = dir.join("src");
    if !src_dir.exists() {
        anyhow::bail!("Source directory not found: {:?}", src_dir);
    }
    if !src_dir.join("Dockerfile").exists() {
        anyhow::bail!("Dockerfile not found: {:?}", src_dir.join("Dockerfile"));
    }

    let docker = Docker::connect_with_defaults().context("Failed to connect to Docker")?;
    let rt = DockerContainerRuntime::new(docker);

    // Note: only `src/` is the build context — `judge/` is intentionally excluded.
    // Use ImageRuntime UFCS so the typed request API is used (inherent build_image
    // is the (&str, &Path) challenge-compat wrapper).
    println!("[fcmc] 开始构建 GameBox 镜像");
    println!("  context: {:?}", src_dir);
    println!("  target : {}", image_tag);
    let mut req = ImageBuildRequest::new(&src_dir, image_tag).with_verbose(true);
    if let Some(proxy) = resolve_build_proxy(proxy) {
        println!("  proxy  : {}", proxy);
        req = req.with_proxy(proxy);
    }
    let result = ImageRuntime::build_image(&rt, req)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;

    println!("[fcmc] 构建完成");
    println!("  image_id  : {}", result.image_id);
    println!("  target_ref: {}", result.target_ref);

    tracing::info!(
        target: "fcmc::build",
        image_id = %result.image_id,
        target_ref = %result.target_ref,
        "gamebox image built"
    );

    Ok(())
}

/// 解析 CLI `--proxy [ip:]port`：未给 ip 时默认
/// `host.docker.internal`. Returns `None` when the flag is absent.
fn resolve_build_proxy(proxy: Option<&str>) -> Option<String> {
    let p = proxy.map(str::trim).filter(|p| !p.is_empty())?;
    Some(if p.contains(':') {
        p.to_string()
    } else {
        format!("host.docker.internal:{p}")
    })
}
