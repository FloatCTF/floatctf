//! GameBox 包解析、摘要与规范化。

use std::path::{Path, PathBuf};

use crate::infrastructure::package;
use crate::modules::gamebox::{GameboxError, GameboxResult};

/// GameBox 包 zip 硬限制（纵深防御）。
pub const MAX_ARCHIVE_BYTES: u64 = package::MAX_ARCHIVE_BYTES;
pub const MAX_EXTRACTED_BYTES: u64 = package::MAX_EXTRACTED_BYTES;
pub const MAX_FILES: usize = package::MAX_FILES;
pub const MAX_SINGLE_FILE_BYTES: u64 = package::MAX_SINGLE_FILE_BYTES;
pub const MAX_META_TOML_BYTES: u64 = package::MAX_META_TOML_BYTES;
pub const MAX_JUDGE_SCRIPT_BYTES: u64 = package::MAX_JUDGE_SCRIPT_BYTES;
pub const MAX_AWDP_SCRIPT_BYTES: u64 = package::MAX_JUDGE_SCRIPT_BYTES;

fn map(e: package::PackageError) -> GameboxError {
    match e {
        package::PackageError::Validation(m) => GameboxError::Validation(m),
        package::PackageError::Internal(m) => GameboxError::Internal(m),
    }
}

/// 安全解压 zip 到 `dest_dir`（大小 / zip-slip / 符号链接防护）。
pub fn extract_package_zip(zip_path: &Path, dest_dir: &Path) -> GameboxResult<()> {
    package::extract_package_zip(zip_path, dest_dir).map_err(map)
}

/// 定位包根：根目录 meta.toml，或恰好一个嵌套 meta.toml。
pub fn discover_package_root(extract_root: &Path) -> GameboxResult<PathBuf> {
    package::discover_package_root(extract_root).map_err(map)
}

/// 校验 `package_root` 下的必需包布局。
pub fn require_package_layout(package_root: &Path) -> GameboxResult<()> {
    package::require_package_layout(package_root).map_err(map)
}

/// 读取 meta.toml 文本（大小已由 require_package_layout 校验）。
pub fn read_meta_toml(package_root: &Path) -> GameboxResult<String> {
    package::read_meta_toml(package_root).map_err(map)
}

/// 若存在则读取裁判脚本；强制路径在包内且受大小限制。
pub fn read_judge_script(package_root: &Path, relative: &str) -> GameboxResult<String> {
    fcmc::validate_judge_path(relative)
        .map_err(|e| GameboxError::Validation(format!("INVALID_JUDGE_PATH: {e}")))?;
    let bytes =
        package::read_package_file(package_root, relative, MAX_JUDGE_SCRIPT_BYTES).map_err(map)?;
    String::from_utf8(bytes)
        .map_err(|e| GameboxError::Validation(format!("JUDGE_SCRIPT_NOT_UTF8: {e}")))
}

/// 读取 AWD-P 攻击脚本；强制路径在包内且受大小限制。
pub fn read_awdp_script(package_root: &Path, relative: &str) -> GameboxResult<String> {
    fcmc::validate_awdp_path(relative)
        .map_err(|e| GameboxError::Validation(format!("INVALID_AWDP_PATH: {e}")))?;
    let bytes =
        package::read_package_file(package_root, relative, MAX_AWDP_SCRIPT_BYTES).map_err(map)?;
    String::from_utf8(bytes)
        .map_err(|e| GameboxError::Validation(format!("AWDP_SCRIPT_NOT_UTF8: {e}")))
}

/// 计算 `package_digest` = 规范文件列表（meta.toml + src/** + judge/**）的 SHA-256。
pub fn compute_package_digest(package_root: &Path) -> GameboxResult<String> {
    package::compute_package_digest(package_root, &["src", "judge"]).map_err(map)
}

/// `NormalizedGameBoxSpec` 规范 JSON 字节的 SHA-256 十六进制摘要。
pub fn compute_spec_digest(spec: &fcmc::NormalizedGameBoxSpec) -> GameboxResult<String> {
    package::compute_spec_digest(spec).map_err(map)
}

/// 截断并清洗构建错误信息（去掉明显疑似密钥的 token，最大约 2KB）。
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
