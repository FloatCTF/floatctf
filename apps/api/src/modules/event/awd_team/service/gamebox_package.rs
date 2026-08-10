//! GameBox package helpers — thin delegation to shared `infrastructure::package`.
//!
//! Canonical package layout (after extract + root discovery):
//! ```text
//! meta.toml
//! src/Dockerfile
//! src/**            # docker build context only
//! judge/**          # optional; never part of docker context
//! ```

use std::path::{Path, PathBuf};

use crate::infrastructure::package;
use crate::modules::event::awd_team::{AwdError, AwdResult};

/// Hard limits for GameBox package zip (defense-in-depth).
pub const MAX_ARCHIVE_BYTES: u64 = package::MAX_ARCHIVE_BYTES;
pub const MAX_EXTRACTED_BYTES: u64 = package::MAX_EXTRACTED_BYTES;
pub const MAX_FILES: usize = package::MAX_FILES;
pub const MAX_SINGLE_FILE_BYTES: u64 = package::MAX_SINGLE_FILE_BYTES;
pub const MAX_META_TOML_BYTES: u64 = package::MAX_META_TOML_BYTES;
pub const MAX_JUDGE_SCRIPT_BYTES: u64 = package::MAX_JUDGE_SCRIPT_BYTES;

fn awd_map(e: package::PackageError) -> AwdError {
    match e {
        package::PackageError::Validation(m) => AwdError::Validation(m),
        package::PackageError::Internal(m) => AwdError::Internal(m),
    }
}

/// Safely extract a zip into `dest_dir` with size / zip-slip / symlink guards.
pub fn extract_package_zip(zip_path: &Path, dest_dir: &Path) -> AwdResult<()> {
    package::extract_package_zip(zip_path, dest_dir).map_err(awd_map)
}

/// Discover package root: root meta.toml or exactly one nested meta.toml.
pub fn discover_package_root(extract_root: &Path) -> AwdResult<PathBuf> {
    package::discover_package_root(extract_root).map_err(awd_map)
}

/// Validate required package layout under `package_root`.
pub fn require_package_layout(package_root: &Path) -> AwdResult<()> {
    package::require_package_layout(package_root).map_err(awd_map)
}

/// Read meta.toml text (already size-checked by require_package_layout).
pub fn read_meta_toml(package_root: &Path) -> AwdResult<String> {
    package::read_meta_toml(package_root).map_err(awd_map)
}

/// Read judge script if present; enforces path under package and size limit.
pub fn read_judge_script(package_root: &Path, relative: &str) -> AwdResult<String> {
    fcmc::validate_judge_path(relative)
        .map_err(|e| AwdError::Validation(format!("INVALID_JUDGE_PATH: {e}")))?;
    let bytes = package::read_package_file(package_root, relative, MAX_JUDGE_SCRIPT_BYTES)
        .map_err(awd_map)?;
    String::from_utf8(bytes)
        .map_err(|e| AwdError::Validation(format!("JUDGE_SCRIPT_NOT_UTF8: {e}")))
}

/// Compute package_digest = SHA-256 over canonical file list (meta.toml + src/** + judge/**).
pub fn compute_package_digest(package_root: &Path) -> AwdResult<String> {
    package::compute_package_digest(package_root, &["src", "judge"]).map_err(awd_map)
}

/// SHA-256 hex of canonical JSON bytes for a NormalizedGameBoxSpec.
pub fn compute_spec_digest(spec: &fcmc::NormalizedGameBoxSpec) -> AwdResult<String> {
    package::compute_spec_digest(spec).map_err(awd_map)
}

/// Bound + sanitize build error messages (strip obvious secret-looking tokens, max ~2KB).
pub fn sanitize_build_error(msg: &str) -> String {
    package::sanitize_build_error(msg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::ZipWriter;
    use zip::write::SimpleFileOptions;

    fn write_minimal_package(root: &Path) {
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("meta.toml"),
            r#"
name = "TTT1"
version = "1.0.0"
author = "a@b.c"
category = "web"
description = "d"
safe_name = "ttt1"

[gamebox]
username = "ctf"
"#,
        )
        .unwrap();
        std::fs::write(root.join("src/Dockerfile"), "FROM scratch\n").unwrap();
    }

    #[test]
    fn package_digest_stable_across_order() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        write_minimal_package(a.path());
        write_minimal_package(b.path());
        std::fs::write(a.path().join("src/a.txt"), b"hello").unwrap();
        std::fs::write(a.path().join("src/b.txt"), b"world").unwrap();
        std::fs::write(b.path().join("src/b.txt"), b"world").unwrap();
        std::fs::write(b.path().join("src/a.txt"), b"hello").unwrap();

        let da = compute_package_digest(a.path()).unwrap();
        let db = compute_package_digest(b.path()).unwrap();
        assert_eq!(da, db);
        assert_eq!(da.len(), 64);
    }

    #[test]
    fn zip_slip_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let zip_path = dir.path().join("evil.zip");
        {
            let f = std::fs::File::create(&zip_path).unwrap();
            let mut zw = ZipWriter::new(f);
            zw.start_file("../escape.toml", SimpleFileOptions::default())
                .unwrap();
            zw.write_all(b"x").unwrap();
            zw.finish().unwrap();
        }
        let dest = tempfile::tempdir().unwrap();
        let err = extract_package_zip(&zip_path, dest.path()).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("INVALID_PATH") || msg.contains("unsafe") || msg.contains("traversal"),
            "unexpected: {msg}"
        );
    }

    #[test]
    fn image_ref_helper_matches_fcmc() {
        assert_eq!(
            fcmc::build_gamebox_image_ref("floatctf", "ttt1", "1.0.0"),
            "floatctf/gameboxes/ttt1:1.0.0"
        );
    }

    #[test]
    fn manifest_rejects_legacy_fields() {
        let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
description = "d"
image_tag = "x"

[gamebox]
username = "u"
"#;
        assert!(fcmc::GameBoxMeta::from_toml_str(toml).is_err());
    }
}
