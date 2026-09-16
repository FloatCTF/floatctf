//! 包目录 → **content id** 解析。
//!
//! floatctf-content 中 `id` 就是目录名：`challenges/<id>/meta.toml`。FCMC 的
//! `check` / `build` 直接操作目录，因此包目录名即 content id，`safe_name`
//! 缺省从它派生（**绝不**从 `meta.name` 派生）。

use std::path::Path;

use anyhow::{Context, Result};

/// 取包目录名作为 content id。
///
/// `-p .` / 相对路径 / 带尾斜杠的路径都会被规范化后再取最后一段，因此
/// `fcmc check`（缺省 `.`）也能得到真实目录名。
pub fn resolve_content_id(dir: &Path) -> Result<String> {
    if let Some(name) = std::fs::canonicalize(dir)
        .ok()
        .as_ref()
        .and_then(|canonical| canonical.file_name())
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
    {
        return Ok(name.to_string());
    }

    dir.file_name()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty() && *s != ".")
        .map(str::to_string)
        .with_context(|| {
            format!(
                "cannot determine content id from package directory: {}",
                dir.display()
            )
        })
}

/// 官方“是否为容器”判定：`src/Dockerfile` 是否存在。
///
/// **不是**看 `[docker]` 段是否存在（`content.py::has_dockerfile`）。
pub fn has_dockerfile(dir: &Path) -> bool {
    dir.join("src").join("Dockerfile").is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_id_is_directory_name() {
        let tmp = tempfile::tempdir().unwrap();
        let pkg = tmp.path().join("Cirno's perfect math class");
        std::fs::create_dir_all(&pkg).unwrap();

        assert_eq!(
            resolve_content_id(&pkg).unwrap(),
            "Cirno's perfect math class"
        );
        // 相对路径 + `.` / 尾斜杠都能解析
        assert_eq!(
            resolve_content_id(&pkg.join(".")).unwrap(),
            "Cirno's perfect math class"
        );
    }

    #[test]
    fn dockerfile_detection_is_path_based() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(!has_dockerfile(tmp.path()));
        std::fs::create_dir_all(tmp.path().join("src")).unwrap();
        std::fs::write(tmp.path().join("src/Dockerfile"), "FROM scratch\n").unwrap();
        assert!(has_dockerfile(tmp.path()));
    }
}
