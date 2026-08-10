//! Generic image build / push / pull / inspect APIs (Docker via bollard).
//!
//! Domain-agnostic: no GameBox/Challenge knowledge. Callers supply the target
//! ref, labels, and registry auth from platform configuration.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use thiserror::Error;

// ---------------------------------------------------------------------------
// Request / result types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ImageBuildRequest {
    /// Host directory used as the Docker build context (tar'd and streamed).
    pub context_dir: PathBuf,
    /// Dockerfile path relative to `context_dir` (default `"Dockerfile"`).
    pub dockerfile: String,
    /// Tag applied to the built image (`name:tag`).
    pub target_ref: String,
    /// Image labels (`io.floatctf.*`, etc.).
    pub labels: HashMap<String, String>,
    /// Hard timeout for the build stream.
    pub timeout: Duration,
}

impl ImageBuildRequest {
    pub fn new(context_dir: impl Into<PathBuf>, target_ref: impl Into<String>) -> Self {
        Self {
            context_dir: context_dir.into(),
            dockerfile: "Dockerfile".into(),
            target_ref: target_ref.into(),
            labels: HashMap::new(),
            timeout: Duration::from_secs(600),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ImageBuildResult {
    /// Authoritative local image id (`sha256:…`), from inspect — never grepped from logs.
    pub image_id: String,
    pub target_ref: String,
}

#[derive(Debug, Clone)]
pub struct ImageInspect {
    pub image_id: String,
    pub repo_tags: Vec<String>,
    pub repo_digests: Vec<String>,
}

/// Registry credentials supplied by the platform (never from meta.toml).
#[derive(Debug, Clone, Default)]
pub struct RegistryAuth {
    pub username: Option<String>,
    pub password: Option<String>,
    pub server_address: Option<String>,
}

#[derive(Debug, Error)]
pub enum ImageError {
    #[error("image build failed: {0}")]
    BuildFailed(String),
    #[error("image build timed out")]
    BuildTimeout,
    #[error("image not found: {0}")]
    ImageNotFound(String),
    #[error("image push failed: {0}")]
    PushFailed(String),
    #[error("image pull failed: {0}")]
    PullFailed(String),
    #[error("image inspect failed: {0}")]
    InspectFailed(String),
    #[error("registry auth failed: {0}")]
    RegistryAuthFailed(String),
    #[error("repo digest unavailable for: {0}")]
    DigestUnavailable(String),
    #[error("image tag failed: {0}")]
    TagFailed(String),
    #[error("image error: {0}")]
    Other(String),
}

// ---------------------------------------------------------------------------
// Image ref helpers
// ---------------------------------------------------------------------------

/// Split an image reference into `(repository, tag_or_none)`.
///
/// Handles:
/// - `nginx:latest`
/// - `floatctf/gameboxes/ttt1:1.0.0`
/// - `registry.example.com:5000/foo/bar:1.0`
/// - `repo@sha256:…` → tag_or_none is `None` (digest form); repository is before `@`
///
/// Tag is taken from the **last path component** only, so host:port is preserved.
pub fn split_image_ref(image_ref: &str) -> (String, Option<String>) {
    if let Some((repo, _digest)) = image_ref.split_once('@') {
        return (repo.to_string(), None);
    }

    let (prefix, name) = match image_ref.rfind('/') {
        Some(i) => (&image_ref[..i], &image_ref[i + 1..]),
        None => ("", image_ref),
    };

    if let Some((n, tag)) = name.split_once(':') {
        let repo = if prefix.is_empty() {
            n.to_string()
        } else {
            format!("{prefix}/{n}")
        };
        (repo, Some(tag.to_string()))
    } else {
        (image_ref.to_string(), None)
    }
}

/// Repository part of an image ref (strip tag or digest).
pub fn image_repository(image_ref: &str) -> String {
    split_image_ref(image_ref).0
}

/// Pick the `RepoDigest` entry matching the repository of `image_ref`.
///
/// `repo_digests` entries look like `registry.example.com/foo/bar@sha256:abc`.
/// Returns the full `repo@sha256:…` string when found.
pub fn pick_repo_digest(repo_digests: &[String], image_ref: &str) -> Option<String> {
    let repo = image_repository(image_ref);
    repo_digests
        .iter()
        .find(|d| d.split('@').next() == Some(repo.as_str()))
        .cloned()
        .or_else(|| {
            // Fallback: some daemons report digests without registry host when
            // the original ref was un-namespaced differently — try suffix match.
            repo_digests
                .iter()
                .find(|d| {
                    d.split('@').next().is_some_and(|r| {
                        r == repo
                            || r.ends_with(&format!("/{repo}"))
                            || repo.ends_with(&format!("/{r}"))
                    })
                })
                .cloned()
        })
}

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

/// Generic image lifecycle (build / tag / push / pull / inspect / remove).
#[async_trait]
pub trait ImageRuntime: Send + Sync {
    async fn build_image(&self, req: ImageBuildRequest) -> Result<ImageBuildResult, ImageError>;
    async fn inspect_image(&self, image_ref: &str) -> Result<ImageInspect, ImageError>;
    async fn tag_image(&self, source: &str, target_ref: &str) -> Result<(), ImageError>;
    /// Push and return the matching repo digest (`repo@sha256:…`) when available.
    async fn push_image(
        &self,
        image_ref: &str,
        auth: Option<&RegistryAuth>,
    ) -> Result<String, ImageError>;
    async fn pull_image(
        &self,
        image_ref: &str,
        auth: Option<&RegistryAuth>,
    ) -> Result<ImageInspect, ImageError>;
    /// Inspect locally; if missing, pull then re-inspect.
    async fn ensure_image(
        &self,
        image_ref: &str,
        auth: Option<&RegistryAuth>,
    ) -> Result<ImageInspect, ImageError>;
    async fn remove_image(&self, image_ref: &str, force: bool) -> Result<(), ImageError>;
}

// ---------------------------------------------------------------------------
// Docker implementation
// ---------------------------------------------------------------------------

use super::docker::DockerContainerRuntime;
use bollard::auth::DockerCredentials;
use futures_util::StreamExt;
use tracing::{error, info};

fn to_credentials(auth: Option<&RegistryAuth>) -> Option<DockerCredentials> {
    auth.map(|a| DockerCredentials {
        username: a.username.clone(),
        password: a.password.clone(),
        serveraddress: a.server_address.clone(),
        ..Default::default()
    })
}

fn map_bollard_inspect(err: bollard::errors::Error, image_ref: &str) -> ImageError {
    match err {
        bollard::errors::Error::DockerResponseServerError {
            status_code: 404, ..
        } => ImageError::ImageNotFound(image_ref.to_string()),
        other => ImageError::InspectFailed(other.to_string()),
    }
}

#[async_trait]
impl ImageRuntime for DockerContainerRuntime {
    async fn build_image(&self, req: ImageBuildRequest) -> Result<ImageBuildResult, ImageError> {
        use bollard::query_parameters::BuildImageOptionsBuilder;
        use bollard::{body_full, secret::BuildInfo};
        use std::fs::File;
        use std::io::Read;
        use tar::Builder;
        use tempfile::NamedTempFile;
        use tokio_util::bytes::Bytes;

        if !req.context_dir.is_dir() {
            return Err(ImageError::BuildFailed(format!(
                "build context is not a directory: {:?}",
                req.context_dir
            )));
        }

        let dockerfile = if req.dockerfile.is_empty() {
            "Dockerfile".to_string()
        } else {
            req.dockerfile.clone()
        };

        let mut builder = BuildImageOptionsBuilder::default()
            .t(&req.target_ref)
            .dockerfile(&dockerfile)
            .rm(true);

        if !req.labels.is_empty() {
            builder = builder.labels(&req.labels);
        }
        let options = builder.build();

        // Tar the context directory.
        let tmp =
            NamedTempFile::new().map_err(|e| ImageError::BuildFailed(format!("temp file: {e}")))?;
        {
            let file = File::create(tmp.path())
                .map_err(|e| ImageError::BuildFailed(format!("create tar: {e}")))?;
            let mut tar_builder = Builder::new(file);
            tar_builder
                .append_dir_all(".", &req.context_dir)
                .map_err(|e| ImageError::BuildFailed(format!("tar context: {e}")))?;
            tar_builder
                .finish()
                .map_err(|e| ImageError::BuildFailed(format!("finish tar: {e}")))?;
        }

        let mut buf = Vec::new();
        File::open(tmp.path())
            .and_then(|mut f| f.read_to_end(&mut buf))
            .map_err(|e| ImageError::BuildFailed(format!("read tar: {e}")))?;

        let body = body_full(Bytes::from(buf));
        let docker = self.inner().clone();
        let target_ref = req.target_ref.clone();
        let timeout = req.timeout;

        let build_future = async {
            let mut build_stream = docker.build_image(options, None, Some(body));
            while let Some(update) = build_stream.next().await {
                let info: BuildInfo = update.map_err(|e| ImageError::BuildFailed(e.to_string()))?;
                if let Some(ref stream_msg) = info.stream {
                    let msg = stream_msg.trim();
                    if !msg.is_empty() {
                        info!(target: "fcmc::image", "{msg}");
                    }
                }
                if let Some(ref err) = info.error {
                    error!(target: "fcmc::image", "build error: {err}");
                    return Err(ImageError::BuildFailed(err.clone()));
                }
            }
            Ok::<(), ImageError>(())
        };

        match tokio::time::timeout(timeout, build_future).await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return Err(e),
            Err(_) => return Err(ImageError::BuildTimeout),
        }

        // Authoritative id via inspect — never parse build logs.
        let inspected = self.inspect_image(&target_ref).await?;
        Ok(ImageBuildResult {
            image_id: inspected.image_id,
            target_ref,
        })
    }

    async fn inspect_image(&self, image_ref: &str) -> Result<ImageInspect, ImageError> {
        let info = self
            .inner()
            .inspect_image(image_ref)
            .await
            .map_err(|e| map_bollard_inspect(e, image_ref))?;

        Ok(ImageInspect {
            image_id: info.id.unwrap_or_default(),
            repo_tags: info.repo_tags.unwrap_or_default(),
            repo_digests: info.repo_digests.unwrap_or_default(),
        })
    }

    async fn tag_image(&self, source: &str, target_ref: &str) -> Result<(), ImageError> {
        use bollard::query_parameters::TagImageOptionsBuilder;

        let (repo, tag) = split_image_ref(target_ref);
        let mut builder = TagImageOptionsBuilder::default().repo(&repo);
        if let Some(ref t) = tag {
            builder = builder.tag(t);
        }
        let options = builder.build();

        self.inner()
            .tag_image(source, Some(options))
            .await
            .map_err(|e| match e {
                bollard::errors::Error::DockerResponseServerError {
                    status_code: 404, ..
                } => ImageError::ImageNotFound(source.to_string()),
                other => ImageError::TagFailed(other.to_string()),
            })
    }

    async fn push_image(
        &self,
        image_ref: &str,
        auth: Option<&RegistryAuth>,
    ) -> Result<String, ImageError> {
        use bollard::query_parameters::PushImageOptionsBuilder;

        let (repo, tag) = split_image_ref(image_ref);
        let mut builder = PushImageOptionsBuilder::default();
        if let Some(ref t) = tag {
            builder = builder.tag(t);
        }
        let options = builder.build();
        let credentials = to_credentials(auth);

        let mut stream = self.inner().push_image(&repo, Some(options), credentials);

        while let Some(update) = stream.next().await {
            match update {
                Ok(info) => {
                    if let Some(err) = info.error {
                        // Auth-ish failures
                        let lower = err.to_lowercase();
                        if lower.contains("unauthorized")
                            || lower.contains("authentication")
                            || lower.contains("denied")
                        {
                            return Err(ImageError::RegistryAuthFailed(err));
                        }
                        return Err(ImageError::PushFailed(err));
                    }
                }
                Err(e) => {
                    let msg = e.to_string();
                    let lower = msg.to_lowercase();
                    if lower.contains("unauthorized")
                        || lower.contains("authentication")
                        || lower.contains("denied")
                    {
                        return Err(ImageError::RegistryAuthFailed(msg));
                    }
                    return Err(ImageError::PushFailed(msg));
                }
            }
        }

        // After push, inspect and extract matching RepoDigest.
        let inspected = self.inspect_image(image_ref).await?;
        pick_repo_digest(&inspected.repo_digests, image_ref)
            .ok_or_else(|| ImageError::DigestUnavailable(image_ref.to_string()))
    }

    async fn pull_image(
        &self,
        image_ref: &str,
        auth: Option<&RegistryAuth>,
    ) -> Result<ImageInspect, ImageError> {
        use bollard::query_parameters::CreateImageOptionsBuilder;

        let (repo, tag) = split_image_ref(image_ref);
        let mut builder = CreateImageOptionsBuilder::default().from_image(&repo);
        if let Some(ref t) = tag {
            builder = builder.tag(t);
        }
        let options = builder.build();
        let credentials = to_credentials(auth);

        let mut stream = self.inner().create_image(Some(options), None, credentials);

        while let Some(update) = stream.next().await {
            match update {
                Ok(info) => {
                    if let Some(err) = info.error {
                        let lower = err.to_lowercase();
                        if lower.contains("unauthorized")
                            || lower.contains("authentication")
                            || lower.contains("denied")
                        {
                            return Err(ImageError::RegistryAuthFailed(err));
                        }
                        return Err(ImageError::PullFailed(err));
                    }
                }
                Err(e) => {
                    let msg = e.to_string();
                    let lower = msg.to_lowercase();
                    if lower.contains("unauthorized")
                        || lower.contains("authentication")
                        || lower.contains("denied")
                    {
                        return Err(ImageError::RegistryAuthFailed(msg));
                    }
                    if lower.contains("not found") || lower.contains("404") {
                        return Err(ImageError::ImageNotFound(image_ref.to_string()));
                    }
                    return Err(ImageError::PullFailed(msg));
                }
            }
        }

        self.inspect_image(image_ref).await
    }

    async fn ensure_image(
        &self,
        image_ref: &str,
        auth: Option<&RegistryAuth>,
    ) -> Result<ImageInspect, ImageError> {
        match self.inspect_image(image_ref).await {
            Ok(info) => Ok(info),
            Err(ImageError::ImageNotFound(_)) => self.pull_image(image_ref, auth).await,
            Err(e) => Err(e),
        }
    }

    async fn remove_image(&self, image_ref: &str, force: bool) -> Result<(), ImageError> {
        use bollard::query_parameters::RemoveImageOptionsBuilder;

        let options = RemoveImageOptionsBuilder::default().force(force).build();
        match self
            .inner()
            .remove_image(image_ref, Some(options), None)
            .await
        {
            Ok(_) => Ok(()),
            Err(bollard::errors::Error::DockerResponseServerError {
                status_code: 404, ..
            }) => Ok(()),
            Err(e) => Err(ImageError::Other(e.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_simple_tag() {
        assert_eq!(
            split_image_ref("nginx:latest"),
            ("nginx".into(), Some("latest".into()))
        );
    }

    #[test]
    fn split_nested_repo() {
        assert_eq!(
            split_image_ref("floatctf/gameboxes/ttt1:1.0.0"),
            ("floatctf/gameboxes/ttt1".into(), Some("1.0.0".into()))
        );
    }

    #[test]
    fn split_registry_with_port() {
        assert_eq!(
            split_image_ref("registry.example.com:5000/foo/bar:1.0"),
            (
                "registry.example.com:5000/foo/bar".into(),
                Some("1.0".into())
            )
        );
    }

    #[test]
    fn split_digest_ref() {
        assert_eq!(
            split_image_ref("floatctf/gameboxes/ttt1@sha256:abc"),
            ("floatctf/gameboxes/ttt1".into(), None)
        );
    }

    #[test]
    fn pick_repo_digest_exact() {
        let digests = vec![
            "other/repo@sha256:111".into(),
            "floatctf/gameboxes/ttt1@sha256:abc".into(),
        ];
        assert_eq!(
            pick_repo_digest(&digests, "floatctf/gameboxes/ttt1:1.0.0").as_deref(),
            Some("floatctf/gameboxes/ttt1@sha256:abc")
        );
    }

    #[test]
    fn pick_repo_digest_missing() {
        let digests = vec!["other/repo@sha256:111".into()];
        assert!(pick_repo_digest(&digests, "floatctf/gameboxes/ttt1:1.0.0").is_none());
    }

    #[test]
    fn pick_repo_digest_with_port_registry() {
        let digests = vec!["registry.example.com:5000/foo/bar@sha256:zzz".into()];
        assert_eq!(
            pick_repo_digest(&digests, "registry.example.com:5000/foo/bar:1.0").as_deref(),
            Some("registry.example.com:5000/foo/bar@sha256:zzz")
        );
    }
}
