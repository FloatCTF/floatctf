//! static / attachment-only Challenge 包的导入前置链路。
//!
//! 背景：floatctf-content 官方 28 道题里有 16 道没有 `src/Dockerfile`
//! （纯附件题 / 静态 flag 题）。它们必须能从 admin 直接导入，而不是被
//! `DOCKERFILE_MISSING` 挡在门外。
//!
//! 纯文件系统测试（不依赖 DB / Docker），走真实 import 前置函数：
//! extract → discover(id) → require_meta_toml → 是否容器 → 读取附件 → 计算 digest。
//! 容器化策略（static 时丢弃 `[docker].port`）由 `import_service` 的单元测试覆盖
//! （`container_policy_drops_port_for_static_content`）。

use std::io::Write;
use std::path::{Path, PathBuf};

use zip::ZipWriter;
use zip::write::SimpleFileOptions;

use floatctf::infrastructure::package::{
    compute_package_digest, discover_package, extract_package_zip, has_dockerfile, read_meta_toml,
    read_package_file, require_meta_toml, require_src_dockerfile,
};

/// floatctf-content@main 官方 fixture 的逐字节副本（见该目录 README）。
/// 集成测试的 CWD 是 crate 目录，因此用绝对路径。
fn official_fixture(content_id: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/fcmc/tests/fixtures/content_contract/challenges")
        .join(content_id)
}

/// 目录 → zip（顶层目录名 = content id，与 admin 上传的真实包形态一致）。
fn zip_dir(source: &Path, content_id: &str, zip_path: &Path) {
    fn add_dir(zw: &mut ZipWriter<std::fs::File>, dir: &Path, prefix: &str) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let rel = format!("{prefix}/{}", entry.file_name().to_string_lossy());
            if entry.path().is_dir() {
                add_dir(zw, &entry.path(), &rel);
            } else {
                let bytes = std::fs::read(entry.path()).unwrap();
                zw.start_file(rel, SimpleFileOptions::default()).unwrap();
                zw.write_all(&bytes).unwrap();
            }
        }
    }

    let f = std::fs::File::create(zip_path).unwrap();
    let mut zw = ZipWriter::new(f);
    add_dir(&mut zw, source, content_id);
    zw.finish().unwrap();
}

/// 已解压的包。两个 `TempDir` 必须活到测试结束，否则包根会随解压目录一起消失。
struct Prepared {
    content_id: String,
    #[allow(dead_code)]
    zip_tmp: tempfile::TempDir,
    #[allow(dead_code)]
    extract: tempfile::TempDir,
    root: PathBuf,
}

impl Prepared {
    /// 从官方 fixture 打包并解压（真实 import 前置步骤）。
    fn from_official(content_id: &str) -> Self {
        let source = official_fixture(content_id);
        assert!(source.is_dir(), "fixture missing: {}", source.display());
        Self::from_dir(&source, content_id)
    }

    /// 从任意包目录打包并解压。
    fn from_dir(source: &Path, content_id: &str) -> Self {
        let zip_tmp = tempfile::tempdir().unwrap();
        let zip_path = zip_tmp.path().join("pkg.zip");
        let staging = tempfile::tempdir().unwrap();
        let staging_pkg = staging.path().join(content_id);
        copy_tree(source, &staging_pkg);
        zip_dir(&staging_pkg, content_id, &zip_path);

        let extract = tempfile::tempdir().unwrap();
        extract_package_zip(&zip_path, extract.path()).unwrap();
        let discovered = discover_package(extract.path()).unwrap();
        assert_eq!(discovered.content_id.as_deref(), Some(content_id));
        Self {
            content_id: content_id.to_string(),
            zip_tmp,
            extract,
            root: discovered.root,
        }
    }

    /// 解压后的 package_root 确实含 meta.toml。
    fn require_meta(&self) {
        require_meta_toml(&self.root).expect("meta.toml must be present at package root");
    }

    fn is_container(&self) -> bool {
        has_dockerfile(&self.root)
    }

    fn meta(&self) -> fcmc::ChallengeMeta {
        fcmc::ChallengeMeta::parse_and_validate(
            &read_meta_toml(&self.root).unwrap(),
            &self.content_id,
        )
        .expect("metadata must follow the FloatCTF Content Contract")
    }
}

fn copy_tree(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).unwrap();
    for entry in std::fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let target = dst.join(entry.file_name());
        if entry.path().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).unwrap();
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// 官方 static 题（无 src/Dockerfile）必须可以通过布局校验
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn official_static_challenge_without_dockerfile_is_accepted() {
    // Android_reverse: 官方 16 道 static 题之一（纯附件 / 静态 flag）。
    let prepared = Prepared::from_official("Android_reverse");

    prepared.require_meta(); // ← 以前这里会因缺少 src/Dockerfile 直接失败
    assert!(!prepared.is_container());
    // 容器校验仍然会把 static 判为"不是容器内容"，但这是能力判定，不是导入错误
    assert!(require_src_dockerfile(&prepared.root).is_err());

    let meta = prepared.meta();
    assert_eq!(
        meta.resolved_safe_name(&prepared.content_id).unwrap(),
        "android_reverse",
        "safe_name 由目录名派生"
    );
    assert_eq!(meta.difficulty, fcmc::Difficulty::Easy);
    assert!(meta.tags.is_empty());

    let normalized = meta.normalize(&prepared.content_id).unwrap();
    assert_eq!(normalized.container_port, None, "static 内容不带容器端口");
    // 该 fixture（官方 safe-names 副本）未声明 [flag]：Content Contract 允许，
    // 因此可以导入；但平台开局需要 flag_type，这类题需管理员补 flag 后才能玩。
    assert_eq!(normalized.flag_type, None);

    let digest = compute_package_digest(&prepared.root, &["src", "attachment"]).unwrap();
    assert_eq!(digest.len(), 64, "src/ 不存在不会让 digest 计算失败");
}

#[test]
fn official_static_flag_challenge_without_attachment_is_importable() {
    // cookie: static flag、无附件、无 Dockerfile。
    let prepared = Prepared::from_official("cookie");
    prepared.require_meta();
    assert!(!prepared.is_container());

    let meta = prepared.meta();
    assert!(meta.attachment.is_none());
    assert_eq!(
        meta.static_flag_value(),
        Some("flag{fixture-flag-must-not-leak}")
    );

    let normalized = meta.normalize(&prepared.content_id).unwrap();
    assert_eq!(normalized.container_port, None);
    assert!(normalized.attachment.is_none());
}

#[test]
fn official_static_with_docker_fixture_is_static_content() {
    // static_with_docker: 声明了 [docker] port，但没有 src/Dockerfile。
    // 平台必须按"static"处理（端口由 import_service 策略丢弃）。
    let prepared = Prepared::from_official("static_with_docker");
    prepared.require_meta();
    assert!(!prepared.is_container());

    let meta = prepared.meta();
    let normalized = meta.normalize(&prepared.content_id).unwrap();
    assert_eq!(
        normalized.container_port,
        Some(8080),
        "metadata 如实保留声明（平台策略另行丢弃）"
    );
    assert_eq!(normalized.flag_type, None, "fixture 未声明 [flag]");
}

#[test]
fn container_challenge_is_still_detected_as_container() {
    // comment: 官方 container 题（有 src/Dockerfile）。
    let prepared = Prepared::from_official("comment");
    prepared.require_meta();
    assert!(prepared.is_container());
    assert!(require_src_dockerfile(&prepared.root).is_ok());
    assert_eq!(
        prepared
            .meta()
            .normalize(&prepared.content_id)
            .unwrap()
            .container_port,
        Some(80)
    );
}

// ────────────────────────────────────────────────────────────────────────────
// static + 附件：附件元数据与 payload 必须可读、可摘要
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn static_attachment_package_exposes_readable_attachment() {
    // 自建包：meta.toml + attachment/，故意没有 src/Dockerfile。
    let staging = tempfile::tempdir().unwrap();
    let pkg = staging.path().join("miku_flag");
    std::fs::create_dir_all(pkg.join("attachment")).unwrap();
    std::fs::write(
        pkg.join("meta.toml"),
        r#"name = "miku_flag"
version = "1.0.0"
author = "dev@floatctf.local"
category = "misc"
difficulty = "easy"
tags = ["misc"]
description = "attachment-only challenge"

attachment = "attachment/mikuflag.png"

[flag]
type = "static"
value = "flag{attachment-only}"
"#,
    )
    .unwrap();
    std::fs::write(
        pkg.join("attachment/mikuflag.png"),
        b"\x89PNG\r\n\x1a\nfake",
    )
    .unwrap();

    let prepared = Prepared::from_dir(&pkg, "miku_flag");
    prepared.require_meta();
    assert!(!prepared.is_container(), "无 src/Dockerfile → static 内容");

    let meta = prepared.meta();
    let relative = meta.attachment.as_deref().expect("attachment declared");
    assert_eq!(relative, "attachment/mikuflag.png");

    // 平台读取附件 payload（随后写入 attachment_size / sha256 并 mirror 给 Caddy）
    let bytes = read_package_file(&prepared.root, relative, 64 * 1024 * 1024).unwrap();
    assert_eq!(bytes, b"\x89PNG\r\n\x1a\nfake");

    // digest 覆盖 attachment/
    let with_attachment = compute_package_digest(&prepared.root, &["src", "attachment"]).unwrap();
    let without_attachment = compute_package_digest(&prepared.root, &["src"]).unwrap();
    assert_ne!(
        with_attachment, without_attachment,
        "attachment/ 必须进入 package_digest"
    );

    let normalized = meta.normalize(&prepared.content_id).unwrap();
    assert_eq!(normalized.container_port, None);
    assert_eq!(
        normalized.attachment.as_deref(),
        Some("attachment/mikuflag.png")
    );
}
