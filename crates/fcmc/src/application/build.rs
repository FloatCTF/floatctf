//! 包构建用例。

use std::path::Path;

use crate::application::package::{has_dockerfile, resolve_content_id};
use crate::metadata::{
    ArtifactKind, CONTENT_IMAGE_NAMESPACE, ChallengeMeta, GameBoxMeta, content_image_ref,
};
use crate::runtime::{DockerContainerRuntime, ImageBuildRequest, ImageRuntime, connect_preferred};
use anyhow::{Context, Result};

/// CLI 未提供 `-t/--tag` 时使用的默认 registry namespace。
/// 与 floatctf-content 的 `IMAGE_NAMESPACE` 一致（`floatctf`）。
pub const DEFAULT_CLI_REGISTRY_PREFIX: &str = CONTENT_IMAGE_NAMESPACE;

/// 构建 Challenge Docker 镜像。
///
/// 镜像 tag **不再**写在 `meta.toml`。解析顺序：
/// 1. **显式 `tag` 参数**（CLI `-t/--tag` 或调用方提供）—— build override；
/// 2. 否则使用 floatctf-content **canonical ref**：
///    `floatctf/{safe_name}:challenge-v{version}`，其中 `safe_name` 由
///    **content id（包目录名）** 派生或取显式值。
///
/// 仅 `src/` 为 build context——排除 `meta.toml` 与 `attachment/`。
///
/// `proxy` accepts `[ip:]port`（缺省 ip 用 `host.docker.internal`）；`None` 时不注入代理。
pub async fn build_challenge(dir: &Path, tag: Option<&str>, proxy: Option<&str>) -> Result<()> {
    let content_id = resolve_content_id(dir)?;
    let meta_path = dir.join("meta.toml");
    let content = std::fs::read_to_string(&meta_path).context("Failed to read meta.toml")?;

    let cfg = ChallengeMeta::parse_and_validate(&content, &content_id)
        .context("Invalid challenge meta.toml")?;

    let image_tag = resolve_image_tag(tag, || {
        let safe = cfg
            .resolved_safe_name(&content_id)
            .context("Cannot derive safe_name for canonical image ref")?;
        Ok(content_image_ref(
            ArtifactKind::Challenge,
            DEFAULT_CLI_REGISTRY_PREFIX,
            &safe,
            &cfg.version,
        ))
    })?;

    let src_dir = dir.join("src");
    require_dockerfile(&src_dir)?;

    let (docker, _) = connect_preferred()
        .await
        .context("Failed to connect to Docker")?;
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
/// 1. **显式 `tag` 参数**（CLI `-t/--tag` 或调用方提供）—— build override；
/// 2. 否则使用 floatctf-content **canonical ref**：
///    `floatctf/{safe_name}:gamebox-v{version}`。
///
/// 平台/API 导入必须始终从平台配置提供显式 tag。
///
/// `proxy` accepts `[ip:]port`（缺省 ip 用 `host.docker.internal`）；`None` 时不注入代理。
pub async fn build_gamebox(dir: &Path, tag: Option<&str>, proxy: Option<&str>) -> Result<()> {
    let content_id = resolve_content_id(dir)?;
    let meta_path = dir.join("meta.toml");
    let content = std::fs::read_to_string(&meta_path).context("Failed to read meta.toml")?;

    let cfg = GameBoxMeta::parse_and_validate(&content, &content_id)
        .context("Invalid gamebox meta.toml")?;

    let image_tag = resolve_image_tag(tag, || {
        let safe = cfg
            .resolved_safe_name(&content_id)
            .context("Cannot derive safe_name for canonical image ref")?;
        Ok(content_image_ref(
            ArtifactKind::GameBox,
            DEFAULT_CLI_REGISTRY_PREFIX,
            &safe,
            &cfg.version,
        ))
    })?;

    let src_dir = dir.join("src");
    require_dockerfile(&src_dir)?;

    let (docker, _) = connect_preferred()
        .await
        .context("Failed to connect to Docker")?;
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

/// 解析最终镜像 tag：
/// 1. 显式 `-t/--tag`（build override，空/空白视为未提供）→ 原样使用；
/// 2. 否则调用 *canonical* 求值 → floatctf-content canonical ref。
fn resolve_image_tag(
    explicit: Option<&str>,
    canonical: impl FnOnce() -> Result<String>,
) -> Result<String> {
    match explicit.map(str::trim).filter(|tag| !tag.is_empty()) {
        Some(tag) => Ok(tag.to_string()),
        None => canonical(),
    }
}

/// 构建前置：`src/Dockerfile` 存在 → container content；否则明确报 static。
fn require_dockerfile(src_dir: &Path) -> Result<()> {
    let package_dir = src_dir.parent().unwrap_or(src_dir);
    if !has_dockerfile(package_dir) {
        anyhow::bail!(
            "content is static: src/Dockerfile not found ({})",
            src_dir.join("Dockerfile").display()
        );
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_tag_overrides_canonical_ref() {
        let tag = resolve_image_tag(Some("myreg/x:test"), || {
            panic!("canonical must not be evaluated when --tag is given")
        })
        .unwrap();
        assert_eq!(tag, "myreg/x:test");

        // 空 / 纯空白 tag 视为未提供
        let canonical = || Ok("floatctf/comment:challenge-v1.0.0".to_string());
        assert_eq!(
            resolve_image_tag(Some("  "), canonical).unwrap(),
            "floatctf/comment:challenge-v1.0.0"
        );
        assert_eq!(
            resolve_image_tag(None, || Ok("floatctf/comment:challenge-v1.0.0".to_string()))
                .unwrap(),
            "floatctf/comment:challenge-v1.0.0"
        );
    }

    #[test]
    fn default_registry_prefix_is_official_namespace() {
        assert_eq!(DEFAULT_CLI_REGISTRY_PREFIX, "floatctf");
    }

    #[test]
    fn default_tags_match_catalog_json() {
        assert_eq!(
            content_image_ref(
                ArtifactKind::Challenge,
                DEFAULT_CLI_REGISTRY_PREFIX,
                "comment",
                "1.0.0"
            ),
            "floatctf/comment:challenge-v1.0.0"
        );
        assert_eq!(
            content_image_ref(
                ArtifactKind::GameBox,
                DEFAULT_CLI_REGISTRY_PREFIX,
                "comment",
                "1.0.0"
            ),
            "floatctf/comment:gamebox-v1.0.0"
        );
    }
}
