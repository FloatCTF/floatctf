//! Docker image build logic.

use std::path::Path;

use anyhow::{Context, Result};
use bollard::Docker;

use crate::metadata::{ChallengeMeta, GameBoxMeta};
use crate::runtime::DockerContainerRuntime;

/// Build a Challenge Docker image.
pub async fn build_challenge(dir: &Path) -> Result<()> {
    let meta_path = dir.join("meta.toml");
    let content = std::fs::read_to_string(&meta_path).context("Failed to read meta.toml")?;

    let cfg: ChallengeMeta = toml::from_str(&content).context("Failed to parse meta.toml")?;

    let image_tag = cfg
        .docker
        .as_ref()
        .context("No docker config found")?
        .image_tag
        .clone();

    let src_dir = dir.join("src");
    if !src_dir.exists() {
        anyhow::bail!("Source directory not found: {:?}", src_dir);
    }

    let docker = Docker::connect_with_defaults().context("Failed to connect to Docker")?;

    let rt = DockerContainerRuntime::new(docker);
    rt.build_image(&image_tag, &src_dir).await?;

    Ok(())
}

/// Build a GameBox Docker image.
pub async fn build_gamebox(dir: &Path) -> Result<()> {
    let meta_path = dir.join("meta.toml");
    let content = std::fs::read_to_string(&meta_path).context("Failed to read meta.toml")?;

    let cfg: GameBoxMeta = toml::from_str(&content).context("Failed to parse meta.toml")?;

    let image_tag = cfg.gamebox.image_tag.clone();

    let src_dir = dir.join("src");
    if !src_dir.exists() {
        anyhow::bail!("Source directory not found: {:?}", src_dir);
    }

    let docker = Docker::connect_with_defaults().context("Failed to connect to Docker")?;

    let rt = DockerContainerRuntime::new(docker);
    rt.build_image(&image_tag, &src_dir).await?;

    Ok(())
}
