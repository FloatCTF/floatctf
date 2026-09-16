//! Challenge 包导入前置链路：content id（目录名）→ safe_name → canonical image ref。
//!
//! 纯文件系统测试（不依赖 DB / Docker），覆盖 floatctf-content 的 canonical
//! Challenge fixture 在 **平台导入管线** 中也能被接受，并且镜像名与
//! catalog.json 一致。
//!
//! 关键契约（source of truth: floatctf-content/scripts/content.py）：
//! * `id` 是目录名，不是 `meta.name`；
//! * `safe_name` 缺省由 `id` 派生，绝不从 `name` 派生；
//! * canonical image ref = `floatctf/{safe_name}:challenge-v{version}`。

use std::io::Write;
use std::path::Path;

use zip::ZipWriter;
use zip::write::SimpleFileOptions;

use floatctf::infrastructure::package::{
    discover_package, extract_package_zip, read_meta_toml, require_package_layout,
};

/// floatctf-content@main `scripts/tests/fixtures/valid/challenges/comment/meta.toml`
/// （逐字节一致的副本见 crates/fcmc/tests/fixtures/content_contract/）。
const CANONICAL_COMMENT_META: &str = r#"name = "comment"
version = "1.0.0"
author = "fb0sh@outlook.com"
category = "web"
difficulty = "easy"
tags = ["php", "web"]
description = "注释里面有什么？"

[flag]
type = "dynamic"

[docker]
port = 80

[docker.recommended_resources]
cpu_millis = 500
memory_bytes = 268435456
pids_limit = 100
"#;

fn write_canonical_package(root: &Path) {
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("meta.toml"), CANONICAL_COMMENT_META).unwrap();
    std::fs::write(root.join("src/Dockerfile"), "FROM scratch\n").unwrap();
}

/// 把 `<content-id>/` 作为 zip 顶层目录（= admin 上传的真实形态）。
fn zip_with_content_dir(pkg_dir: &Path, content_id: &str, zip_path: &Path) {
    let f = std::fs::File::create(zip_path).unwrap();
    let mut zw = ZipWriter::new(f);
    let opts = SimpleFileOptions::default();

    let meta = std::fs::read(pkg_dir.join("meta.toml")).unwrap();
    zw.start_file(format!("{content_id}/meta.toml"), opts)
        .unwrap();
    zw.write_all(&meta).unwrap();
    let dockerfile = std::fs::read(pkg_dir.join("src/Dockerfile")).unwrap();
    zw.start_file(format!("{content_id}/src/Dockerfile"), opts)
        .unwrap();
    zw.write_all(&dockerfile).unwrap();
    zw.finish().unwrap();
}

/// 把 meta.toml 直接铺在 zip 根（没有内容目录）。
fn zip_without_content_dir(pkg_dir: &Path, zip_path: &Path) {
    let f = std::fs::File::create(zip_path).unwrap();
    let mut zw = ZipWriter::new(f);
    let opts = SimpleFileOptions::default();
    let meta = std::fs::read(pkg_dir.join("meta.toml")).unwrap();
    zw.start_file("meta.toml", opts).unwrap();
    zw.write_all(&meta).unwrap();
    let dockerfile = std::fs::read(pkg_dir.join("src/Dockerfile")).unwrap();
    zw.start_file("src/Dockerfile", opts).unwrap();
    zw.write_all(&dockerfile).unwrap();
    zw.finish().unwrap();
}

#[test]
fn canonical_challenge_import_chain_accepts_fixture() {
    let pkg = tempfile::tempdir().unwrap();
    let package_root = pkg.path().join("comment");
    write_canonical_package(&package_root);

    let zip_dir = tempfile::tempdir().unwrap();
    let zip_path = zip_dir.path().join("comment-1.0.0.zip");
    zip_with_content_dir(&package_root, "comment", &zip_path);

    // extract → discover → layout → read meta（真实 import 前置步骤）
    let extract = tempfile::tempdir().unwrap();
    extract_package_zip(&zip_path, extract.path()).unwrap();
    let discovered = discover_package(extract.path()).unwrap();
    assert_eq!(discovered.content_id.as_deref(), Some("comment"));
    require_package_layout(&discovered.root).unwrap();
    let meta_toml = read_meta_toml(&discovered.root).unwrap();

    let content_id = discovered.content_id.unwrap();
    let meta = fcmc::ChallengeMeta::parse_and_validate(&meta_toml, &content_id)
        .expect("canonical floatctf-content Challenge must be accepted");
    assert_eq!(meta.name, "comment");
    assert_eq!(meta.version, "1.0.0");
    assert_eq!(meta.difficulty, fcmc::Difficulty::Easy);
    assert_eq!(meta.tags, vec!["php".to_string(), "web".to_string()]);

    let safe_name = meta.resolved_safe_name(&content_id).unwrap();
    assert_eq!(safe_name, "comment");
    assert_eq!(
        fcmc::content_image_ref(
            fcmc::ArtifactKind::Challenge,
            "floatctf",
            &safe_name,
            &meta.version
        ),
        "floatctf/comment:challenge-v1.0.0"
    );

    // normalized spec 保留新字段（平台落库用）
    let normalized = meta.normalize(&content_id).unwrap();
    assert_eq!(normalized.difficulty, fcmc::Difficulty::Easy);
    assert_eq!(normalized.tags, vec!["php".to_string(), "web".to_string()]);
    assert_eq!(normalized.recommended_resources.cpu_millis, 500);
}

#[test]
fn safe_name_follows_directory_name_not_meta_name() {
    let pkg = tempfile::tempdir().unwrap();
    let package_root = pkg.path().join("Cirno's perfect math class");
    std::fs::create_dir_all(package_root.join("src")).unwrap();
    std::fs::write(
        package_root.join("meta.toml"),
        r#"name = "琪露诺的完美数学教室"
version = "1.0.0"
author = "dev@floatctf.local"
category = "misc"
difficulty = "easy"
tags = []
description = "display name 与 content id 完全不同"
"#,
    )
    .unwrap();
    std::fs::write(package_root.join("src/Dockerfile"), "FROM scratch\n").unwrap();

    let zip_dir = tempfile::tempdir().unwrap();
    let zip_path = zip_dir.path().join("pkg.zip");
    zip_with_content_dir(&package_root, "Cirno's perfect math class", &zip_path);

    let extract = tempfile::tempdir().unwrap();
    extract_package_zip(&zip_path, extract.path()).unwrap();
    let discovered = discover_package(extract.path()).unwrap();
    let content_id = discovered.content_id.expect("content id from directory");
    assert_eq!(content_id, "Cirno's perfect math class");

    let meta = fcmc::ChallengeMeta::parse_and_validate(
        &read_meta_toml(&discovered.root).unwrap(),
        &content_id,
    )
    .unwrap();
    // safe_name 来自目录名（中文 name 派生出不来）
    assert_eq!(
        meta.resolved_safe_name(&content_id).unwrap(),
        "cirnos-perfect-math-class"
    );
    assert_eq!(
        fcmc::content_image_ref(
            fcmc::ArtifactKind::Challenge,
            "floatctf",
            "cirnos-perfect-math-class",
            "1.0.0"
        ),
        "floatctf/cirnos-perfect-math-class:challenge-v1.0.0"
    );
}

#[test]
fn zip_without_content_directory_requires_explicit_safe_name() {
    let pkg = tempfile::tempdir().unwrap();
    let package_root = pkg.path().join("comment");
    write_canonical_package(&package_root);

    let zip_dir = tempfile::tempdir().unwrap();
    let zip_path = zip_dir.path().join("root-level.zip");
    zip_without_content_dir(&package_root, &zip_path);

    let extract = tempfile::tempdir().unwrap();
    extract_package_zip(&zip_path, extract.path()).unwrap();
    let discovered = discover_package(extract.path()).unwrap();
    // 没有可信的内容目录名 → 绝不拿解压临时目录名当 content id
    assert_eq!(discovered.content_id, None);

    let meta_toml = read_meta_toml(&discovered.root).unwrap();
    let err = fcmc::ChallengeMeta::parse_and_validate(&meta_toml, "")
        .expect_err("cannot derive safe_name without a content id");
    assert!(
        matches!(err, fcmc::ChallengeMetaError::SafeNameRequired),
        "unexpected error: {err}"
    );

    // 显式 safe_name 时，同样可接受（不强求内容目录）
    let with_explicit = meta_toml.replacen("\n[flag]", "\nsafe_name = \"comment\"\n\n[flag]", 1);
    let meta = fcmc::ChallengeMeta::parse_and_validate(&with_explicit, "").unwrap();
    assert_eq!(meta.resolved_safe_name("").unwrap(), "comment");
}
