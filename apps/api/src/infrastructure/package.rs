//! Shared artifact package utilities: safe zip extract, package/spec digests.
//!
//! Used by both Challenge and GameBox import pipelines (plan: share the package
//! foundation — do not re-implement zip-slip guards / semantic digest per domain).
//!
//! Canonical package layout (after extract + root discovery):
//! ```text
//! meta.toml
//! src/Dockerfile
//! src/**            # docker build context only
//! attachment/**     # challenge only; never part of docker context
//! judge/**          # gamebox only; never part of docker context
//! ```

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};

/// Hard limits for artifact package zip (defense-in-depth).
pub const MAX_ARCHIVE_BYTES: u64 = 256 * 1024 * 1024;
pub const MAX_EXTRACTED_BYTES: u64 = 512 * 1024 * 1024;
pub const MAX_FILES: usize = 5000;
pub const MAX_SINGLE_FILE_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_META_TOML_BYTES: u64 = 1024 * 1024;
pub const MAX_PACKAGE_FILE_BYTES: u64 = 1024 * 1024;
/// Judge script size cap (GameBox).
pub const MAX_JUDGE_SCRIPT_BYTES: u64 = 1024 * 1024;

/// Package-processing error with a stable kind for API mapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageError {
    /// Author/package input problem → 4xx (INVALID_PACKAGE / INVALID_PATH / …).
    Validation(String),
    /// Platform internal failure → 5xx.
    Internal(String),
}

impl std::fmt::Display for PackageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PackageError::Validation(m) => write!(f, "{m}"),
            PackageError::Internal(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for PackageError {}

/// Safely extract a zip into `dest_dir` with size / zip-slip / symlink guards.
pub fn extract_package_zip(zip_path: &Path, dest_dir: &Path) -> Result<(), PackageError> {
    let meta = fs::metadata(zip_path)
        .map_err(|e| PackageError::Validation(format!("INVALID_PACKAGE: cannot read zip: {e}")))?;
    if meta.len() > MAX_ARCHIVE_BYTES {
        return Err(PackageError::Validation(format!(
            "PACKAGE_TOO_LARGE: archive exceeds {} bytes",
            MAX_ARCHIVE_BYTES
        )));
    }

    let file = File::open(zip_path)
        .map_err(|e| PackageError::Validation(format!("INVALID_PACKAGE: open zip: {e}")))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| PackageError::Validation(format!("INVALID_PACKAGE: invalid zip: {e}")))?;

    let mut total_extracted: u64 = 0;
    let mut file_count: usize = 0;

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| PackageError::Validation(format!("INVALID_PACKAGE: zip entry: {e}")))?;

        let name = entry.name().to_string();
        if name.is_empty() {
            continue;
        }
        if entry.is_symlink() {
            return Err(PackageError::Validation(
                "INVALID_PATH: symlinks are not allowed in packages".into(),
            ));
        }
        let enclosed = entry.enclosed_name().ok_or_else(|| {
            PackageError::Validation(format!("INVALID_PATH: unsafe zip path: {name}"))
        })?;
        if enclosed.is_absolute() {
            return Err(PackageError::Validation(format!(
                "INVALID_PATH: absolute path rejected: {name}"
            )));
        }
        for comp in enclosed.components() {
            match comp {
                Component::Normal(_) | Component::CurDir => {}
                _ => {
                    return Err(PackageError::Validation(format!(
                        "INVALID_PATH: path traversal rejected: {name}"
                    )));
                }
            }
        }

        let out_path = dest_dir.join(&enclosed);

        if entry.is_dir() {
            fs::create_dir_all(&out_path)
                .map_err(|e| PackageError::Internal(format!("extract mkdir failed: {e}")))?;
            continue;
        }

        file_count += 1;
        if file_count > MAX_FILES {
            return Err(PackageError::Validation(format!(
                "TOO_MANY_FILES: package exceeds {MAX_FILES} files"
            )));
        }

        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| PackageError::Internal(format!("extract mkdir failed: {e}")))?;
        }

        let size = entry.size();
        if size > MAX_SINGLE_FILE_BYTES {
            return Err(PackageError::Validation(format!(
                "PACKAGE_TOO_LARGE: single file exceeds {} bytes: {name}",
                MAX_SINGLE_FILE_BYTES
            )));
        }
        total_extracted = total_extracted.saturating_add(size);
        if total_extracted > MAX_EXTRACTED_BYTES {
            return Err(PackageError::Validation(format!(
                "PACKAGE_TOO_LARGE: extracted size exceeds {} bytes",
                MAX_EXTRACTED_BYTES
            )));
        }

        let mut outfile = File::create(&out_path)
            .map_err(|e| PackageError::Internal(format!("extract create file failed: {e}")))?;
        // Bound actual bytes written (zip size claims can lie).
        let mut buf = Vec::new();
        entry
            .take(MAX_SINGLE_FILE_BYTES + 1)
            .read_to_end(&mut buf)
            .map_err(|e| PackageError::Validation(format!("INVALID_PACKAGE: read entry: {e}")))?;
        if buf.len() as u64 > MAX_SINGLE_FILE_BYTES {
            return Err(PackageError::Validation(format!(
                "PACKAGE_TOO_LARGE: single file exceeds {} bytes: {name}",
                MAX_SINGLE_FILE_BYTES
            )));
        }
        outfile
            .write_all(&buf)
            .map_err(|e| PackageError::Internal(format!("extract write failed: {e}")))?;
    }

    Ok(())
}

/// Discover package root: if `root/meta.toml` exists use root; else if exactly one
/// nested package root containing meta.toml, use that; else error.
pub fn discover_package_root(extract_root: &Path) -> Result<PathBuf, PackageError> {
    let root_meta = extract_root.join("meta.toml");
    if root_meta.is_file() {
        return Ok(extract_root.to_path_buf());
    }

    let mut found = Vec::new();
    find_meta_tomls(extract_root, &mut found)?;

    match found.len() {
        0 => Err(PackageError::Validation(
            "INVALID_PACKAGE: meta.toml not found".into(),
        )),
        1 => Ok(found.remove(0)),
        _ => Err(PackageError::Validation(
            "INVALID_PACKAGE: multiple meta.toml found; provide a single package root".into(),
        )),
    }
}

fn find_meta_tomls(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), PackageError> {
    let entries = fs::read_dir(dir)
        .map_err(|e| PackageError::Internal(format!("read_dir {}: {e}", dir.display())))?;
    for entry in entries {
        let entry = entry.map_err(|e| PackageError::Internal(format!("read_dir entry: {e}")))?;
        let path = entry.path();
        let ft = entry
            .file_type()
            .map_err(|e| PackageError::Internal(format!("file_type: {e}")))?;
        if ft.is_symlink() {
            return Err(PackageError::Validation(
                "INVALID_PATH: symlinks are not allowed in packages".into(),
            ));
        }
        if ft.is_dir() {
            find_meta_tomls(&path, out)?;
        } else if ft.is_file() && entry.file_name() == "meta.toml" {
            if let Some(parent) = path.parent() {
                out.push(parent.to_path_buf());
            }
        }
    }
    Ok(())
}

/// Validate required package layout under `package_root`: meta.toml + src/Dockerfile.
pub fn require_package_layout(package_root: &Path) -> Result<(), PackageError> {
    let meta = package_root.join("meta.toml");
    if !meta.is_file() {
        return Err(PackageError::Validation(
            "INVALID_PACKAGE: meta.toml missing at package root".into(),
        ));
    }
    let meta_len = fs::metadata(&meta).map(|m| m.len()).unwrap_or(0);
    if meta_len > MAX_META_TOML_BYTES {
        return Err(PackageError::Validation(
            "PACKAGE_TOO_LARGE: meta.toml exceeds 1MB".into(),
        ));
    }
    let dockerfile = package_root.join("src").join("Dockerfile");
    if !dockerfile.is_file() {
        return Err(PackageError::Validation(
            "DOCKERFILE_MISSING: src/Dockerfile required at package root".into(),
        ));
    }
    Ok(())
}

/// Read meta.toml text (already size-checked by require_package_layout).
pub fn read_meta_toml(package_root: &Path) -> Result<String, PackageError> {
    fs::read_to_string(package_root.join("meta.toml"))
        .map_err(|e| PackageError::Validation(format!("INVALID_PACKAGE: read meta.toml: {e}")))
}

/// Read an arbitrary package file (e.g. attachment) with path + size guards.
///
/// `relative` must be a relative path that stays under `package_root` after
/// canonicalisation; content limited to `max_bytes`.
pub fn read_package_file(
    package_root: &Path,
    relative: &str,
    max_bytes: u64,
) -> Result<Vec<u8>, PackageError> {
    if relative.is_empty() {
        return Err(PackageError::Validation(format!(
            "INVALID_PATH: empty file path"
        )));
    }
    if relative.starts_with('/') || relative.starts_with('\\') {
        return Err(PackageError::Validation(format!(
            "INVALID_PATH: must be relative: {relative}"
        )));
    }
    if relative.contains("..") {
        return Err(PackageError::Validation(format!(
            "INVALID_PATH: must not contain '..': {relative}"
        )));
    }
    let path = package_root.join(relative);
    let package_canon = fs::canonicalize(package_root)
        .map_err(|e| PackageError::Internal(format!("canonicalize package root: {e}")))?;
    let file_canon = fs::canonicalize(&path).map_err(|_| {
        PackageError::Validation(format!("FILE_NOT_FOUND: {relative} not found in package"))
    })?;
    if !file_canon.starts_with(&package_canon) {
        return Err(PackageError::Validation(format!(
            "INVALID_PATH: escapes package root: {relative}"
        )));
    }
    let meta = fs::metadata(&file_canon)
        .map_err(|e| PackageError::Validation(format!("FILE_NOT_FOUND: {e}")))?;
    if meta.len() > max_bytes {
        return Err(PackageError::Validation(format!(
            "PACKAGE_TOO_LARGE: {relative} exceeds {max_bytes} bytes"
        )));
    }
    fs::read(&file_canon)
        .map_err(|e| PackageError::Validation(format!("FILE_NOT_FOUND: read failed: {e}")))
}

/// Compute package_digest = SHA-256 over canonical file list.
///
/// Includes `meta.toml` plus every tree under each `include_dirs` entry
/// (e.g. `["src", "attachment"]` for challenge, `["src", "judge"]` for gamebox).
/// For each file (sorted by relative path): hash `path\0` + type byte + len + raw bytes.
/// Ignores mtime/owner/permissions. Directories are not hashed as entries.
pub fn compute_package_digest(
    package_root: &Path,
    include_dirs: &[&str],
) -> Result<String, PackageError> {
    let mut files: Vec<(String, PathBuf)> = Vec::new();

    let meta = package_root.join("meta.toml");
    if meta.is_file() {
        files.push(("meta.toml".into(), meta));
    }

    for dir_name in include_dirs {
        let dir = package_root.join(dir_name);
        if dir.is_dir() {
            collect_tree(package_root, &dir, &mut files)?;
        }
    }

    // Dedup + sort by relative path.
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files.dedup_by(|a, b| a.0 == b.0);

    let mut hasher = Sha256::new();
    for (rel, abs) in &files {
        hasher.update(rel.as_bytes());
        hasher.update([0u8]);
        hasher.update(b"f");
        let bytes = fs::read(abs)
            .map_err(|e| PackageError::Validation(format!("INVALID_PACKAGE: read {rel}: {e}")))?;
        hasher.update(&(bytes.len() as u64).to_le_bytes());
        hasher.update(&bytes);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn collect_tree(
    package_root: &Path,
    dir: &Path,
    out: &mut Vec<(String, PathBuf)>,
) -> Result<(), PackageError> {
    let entries = fs::read_dir(dir)
        .map_err(|e| PackageError::Internal(format!("read_dir {}: {e}", dir.display())))?;
    for entry in entries {
        let entry = entry.map_err(|e| PackageError::Internal(format!("read_dir entry: {e}")))?;
        let path = entry.path();
        let ft = entry
            .file_type()
            .map_err(|e| PackageError::Internal(format!("file_type: {e}")))?;
        if ft.is_symlink() {
            return Err(PackageError::Validation(
                "INVALID_PATH: symlinks are not allowed in packages".into(),
            ));
        }
        if ft.is_dir() {
            collect_tree(package_root, &path, out)?;
        } else if ft.is_file() {
            let rel = path
                .strip_prefix(package_root)
                .map_err(|_| {
                    PackageError::Validation("INVALID_PATH: file outside package root".into())
                })?
                .to_string_lossy()
                .replace('\\', "/");
            out.push((rel, path));
        }
    }
    Ok(())
}

/// SHA-256 hex of canonical JSON bytes for any serializable normalized spec.
pub fn compute_spec_digest<T: serde::Serialize>(spec: &T) -> Result<String, PackageError> {
    let bytes = serde_json::to_vec(spec)
        .map_err(|e| PackageError::Internal(format!("serialize normalized spec: {e}")))?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(hex::encode(hasher.finalize()))
}

/// SHA-256 hex of raw bytes (attachment integrity).
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// Bound + sanitize build error messages (strip obvious secret-looking tokens, max ~2KB).
pub fn sanitize_build_error(msg: &str) -> String {
    let mut s = msg.replace('\0', "");
    for needle in ["password=", "PASSWORD=", "Authorization:", "token="] {
        if let Some(idx) = s.find(needle) {
            let end = (idx + needle.len() + 32).min(s.len());
            s.replace_range(idx..end, &format!("{needle}***"));
        }
    }
    const MAX: usize = 2048;
    if s.len() > MAX {
        s.truncate(MAX);
        s.push_str("…(truncated)");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::ZipWriter;
    use zip::write::SimpleFileOptions;

    fn write_minimal_package(root: &Path) {
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("meta.toml"),
            r#"
name = "test"
version = "1.0.0"
author = "a@b.c"
category = "web"
description = "d"
"#,
        )
        .unwrap();
        fs::write(root.join("src/Dockerfile"), "FROM scratch\n").unwrap();
    }

    #[test]
    fn package_digest_stable_across_order() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        write_minimal_package(a.path());
        write_minimal_package(b.path());
        fs::write(a.path().join("src/a.txt"), b"hello").unwrap();
        fs::write(a.path().join("src/b.txt"), b"world").unwrap();
        fs::write(b.path().join("src/b.txt"), b"world").unwrap();
        fs::write(b.path().join("src/a.txt"), b"hello").unwrap();

        let da = compute_package_digest(a.path(), &["src"]).unwrap();
        let db = compute_package_digest(b.path(), &["src"]).unwrap();
        assert_eq!(da, db);
        assert_eq!(da.len(), 64);
    }

    #[test]
    fn package_digest_includes_attachment_when_requested() {
        let a = tempfile::tempdir().unwrap();
        write_minimal_package(a.path());
        let without = compute_package_digest(a.path(), &["src"]).unwrap();
        fs::create_dir_all(a.path().join("attachment")).unwrap();
        fs::write(a.path().join("attachment/src.zip"), b"zip").unwrap();
        let with = compute_package_digest(a.path(), &["src", "attachment"]).unwrap();
        assert_ne!(without, with);
    }

    #[test]
    fn zip_slip_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let zip_path = dir.path().join("evil.zip");
        {
            let f = File::create(&zip_path).unwrap();
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
    fn read_package_file_guards() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("attachment")).unwrap();
        fs::write(root.path().join("attachment/src.zip"), b"abc").unwrap();

        let bytes = read_package_file(root.path(), "attachment/src.zip", 1024).unwrap();
        assert_eq!(bytes, b"abc");

        assert!(read_package_file(root.path(), "../x", 1024).is_err());
        assert!(read_package_file(root.path(), "/abs", 1024).is_err());
        assert!(read_package_file(root.path(), "missing", 1024).is_err());
    }
}
