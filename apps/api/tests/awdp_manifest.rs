//! AWDP GameBox 清单（meta.toml [awdp]）校验测试（plan §77）。
//!
//! 纯文件系统测试（不依赖 DB/Docker），风格跟随 challenge_import.rs：
//! 走真实 import 链路的包解析函数（extract_package_zip / discover_package_root /
//! require_package_layout / read_meta_toml / read_awdp_script / read_judge_script）。
//!
//! 覆盖：
//!   - 无 [awdp]：普通 GameBox 合法（awdp 列为空）
//!   - 完整 [awdp]：合法（exploit 脚本可读、normalize 物化字段）
//!   - [awdp] 缺 source_code_dir：拒绝
//!   - [awdp] 缺 exploit_script：拒绝
//!   - traversal / 越界路径：拒绝（exploit 路径 + source 目录 + 包外文件）
//!   - manifest 合法但脚本文件缺失（磁盘缺文件）：读取失败（import 物化阶段 fail）

use std::io::Write;
use std::path::Path;

use zip::ZipWriter;
use zip::write::SimpleFileOptions;

use floatctf::modules::gamebox::package::{
    discover_package_root, extract_package_zip, read_awdp_script, read_judge_script,
    read_meta_toml, require_package_layout,
};

const BASE_META: &str = r#"
name = "manifest-it"
version = "1.0.0"
author = "it@example.com"
category = "web"
description = "manifest integration"

[gamebox]
username = "ctf"
"#;

/// 写入一个可直接解析的包目录（meta.toml + src/Dockerfile + 可选 extras）。
fn write_package(root: &Path, meta_toml: &str, extras: &[(&str, &str)]) {
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("meta.toml"), meta_toml).unwrap();
    std::fs::write(root.join("src/Dockerfile"), "FROM scratch\n").unwrap();
    for (rel, content) in extras {
        let p = root.join(rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(p, content).unwrap();
    }
}

/// 把包目录打成 zip（模拟 admin 上传的 multipart 文件）。
fn zip_package(root: &Path, zip_path: &Path) {
    let f = std::fs::File::create(zip_path).unwrap();
    let mut zw = ZipWriter::new(f);
    let opts = SimpleFileOptions::default();

    fn add_dir(zw: &mut ZipWriter<std::fs::File>, dir: &Path, prefix: &str) {
        for e in std::fs::read_dir(dir).unwrap() {
            let e = e.unwrap();
            let name = e.file_name().to_string_lossy().into_owned();
            let rel = format!("{prefix}/{name}");
            if e.path().is_dir() {
                zw.start_file(format!("{rel}/"), SimpleFileOptions::default())
                    .unwrap();
                add_dir(zw, &e.path(), &rel);
            } else {
                let bytes = std::fs::read(e.path()).unwrap();
                zw.start_file(rel, SimpleFileOptions::default()).unwrap();
                zw.write_all(&bytes).unwrap();
            }
        }
    }

    let meta_bytes = std::fs::read(root.join("meta.toml")).unwrap();
    zw.start_file("meta.toml", opts).unwrap();
    zw.write_all(&meta_bytes).unwrap();
    add_dir(&mut zw, &root.join("src"), "src");
    for rel in ["judge", "awdp"] {
        let d = root.join(rel);
        if d.exists() {
            zw.start_file(format!("{rel}/"), opts).unwrap();
            add_dir(&mut zw, &d, rel);
        }
    }
    zw.finish().unwrap();
}

/// zip → 解压 → discover → layout 校验 → 读取 meta.toml（真实 import 前置步骤）。
fn load_meta_from_zip(zip_path: &Path, extract_dir: &Path) -> String {
    extract_package_zip(zip_path, extract_dir).expect("extract zip");
    let package_root = discover_package_root(extract_dir).expect("discover root");
    require_package_layout(&package_root).expect("layout");
    read_meta_toml(&package_root).expect("read meta.toml")
}

// ────────────────────────────────────────────────────────────────────────────
// §77：no [awdp] —— 普通 GameBox 合法
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn no_awdp_section_is_valid_normal_gamebox() {
    let dir = tempfile::tempdir().unwrap();
    let extract = tempfile::tempdir().unwrap();
    write_package(
        dir.path(),
        BASE_META,
        &[
            ("judge/check.py", "print('ok')"),
            ("src/index.php", "<?php"),
        ],
    );
    let zip_path = dir.path().join("pkg.zip");
    zip_package(dir.path(), &zip_path);

    let meta = load_meta_from_zip(&zip_path, extract.path());
    let parsed = fcmc::GameBoxMeta::parse_and_validate(&meta).expect("no-[awdp] manifest valid");
    assert!(parsed.awdp.is_none(), "[awdp] must be absent");
    assert_eq!(parsed.gamebox.username, "ctf");
    let norm = parsed.normalize().unwrap();
    assert_eq!(norm.exploit_script, None);
    assert_eq!(norm.source_code_dir, None);

    // judge 脚本仍可正常读取（非 awdp 能力不受影响）。
    let _ = read_judge_script(extract.path(), "judge/check.py").unwrap();
}

// ────────────────────────────────────────────────────────────────────────────
// §77：完整 [awdp] —— 合法
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn complete_awdp_section_is_valid() {
    let dir = tempfile::tempdir().unwrap();
    let extract = tempfile::tempdir().unwrap();
    let meta_toml = format!(
        r#"{BASE_META}
[judge]
script = "judge/check.py"

[awdp]
exploit_script = "awdp/exploit.py"
source_code_dir = "/var/www/html"
"#
    );
    write_package(
        dir.path(),
        &meta_toml,
        &[
            ("judge/check.py", "print('ok')"),
            ("awdp/exploit.py", "print('attack')"),
        ],
    );
    let zip_path = dir.path().join("pkg.zip");
    zip_package(dir.path(), &zip_path);

    let meta = load_meta_from_zip(&zip_path, extract.path());
    let parsed = fcmc::GameBoxMeta::parse_and_validate(&meta).expect("complete [awdp] valid");
    let awdp = parsed.awdp.as_ref().unwrap();
    assert_eq!(awdp.exploit_script, "awdp/exploit.py");
    assert_eq!(awdp.source_code_dir, "/var/www/html");
    let norm = parsed.normalize().unwrap();
    assert_eq!(norm.exploit_script.as_deref(), Some("awdp/exploit.py"));
    assert_eq!(norm.source_code_dir.as_deref(), Some("/var/www/html"));

    // 脚本内容按 import 流程可读取。
    let content = read_awdp_script(extract.path(), "awdp/exploit.py").unwrap();
    assert_eq!(content, "print('attack')");
}

// ────────────────────────────────────────────────────────────────────────────
// §77：[awdp] 缺 source_code_dir → fail
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn awdp_missing_source_code_dir_rejected() {
    let meta_toml = format!(
        r#"{BASE_META}
[awdp]
exploit_script = "awdp/exploit.py"
"#
    );
    let err = fcmc::GameBoxMeta::parse_and_validate(&meta_toml).unwrap_err();
    assert!(
        err.to_string().contains("source_code_dir") || err.to_string().contains("missing"),
        "missing source_code_dir must fail: {err}"
    );
}

// ────────────────────────────────────────────────────────────────────────────
// §77：[awdp] 缺 exploit_script → fail
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn awdp_missing_exploit_script_rejected() {
    let meta_toml = format!(
        r#"{BASE_META}
[awdp]
source_code_dir = "/var/www/html"
"#
    );
    let err = fcmc::GameBoxMeta::parse_and_validate(&meta_toml).unwrap_err();
    assert!(
        err.to_string().contains("exploit_script") || err.to_string().contains("missing"),
        "missing exploit_script must fail: {err}"
    );
}

// ────────────────────────────────────────────────────────────────────────────
// §77：traversal / 越界 → fail
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn awdp_traversal_paths_rejected() {
    // exploit 路径带 .. → 拒绝。
    let t1 = format!(
        r#"{BASE_META}
[awdp]
exploit_script = "awdp/../escape.py"
source_code_dir = "/var/www/html"
"#
    );
    let err = fcmc::GameBoxMeta::parse_and_validate(&t1).unwrap_err();
    assert!(
        err.to_string().contains("exploit") || err.to_string().contains("awdp"),
        "traversal exploit path must fail: {err}"
    );

    // exploit 路径前缀错误（scripts/ 而不是 awdp/）→ 拒绝。
    let t2 = format!(
        r#"{BASE_META}
[awdp]
exploit_script = "scripts/exploit.py"
source_code_dir = "/var/www/html"
"#
    );
    assert!(fcmc::GameBoxMeta::parse_and_validate(&t2).is_err());

    // source_code_dir 带 .. → 拒绝。
    let t3 = format!(
        r#"{BASE_META}
[awdp]
exploit_script = "awdp/exploit.py"
source_code_dir = "/var/www/../html"
"#
    );
    let err = fcmc::GameBoxMeta::parse_and_validate(&t3).unwrap_err();
    assert!(
        err.to_string().contains("source_code_dir") || err.to_string().contains("awdp"),
        "traversal source dir must fail: {err}"
    );

    // source_code_dir 相对路径 → 拒绝。
    let t4 = format!(
        r#"{BASE_META}
[awdp]
exploit_script = "awdp/exploit.py"
source_code_dir = "var/www/html"
"#
    );
    assert!(fcmc::GameBoxMeta::parse_and_validate(&t4).is_err());

    // 绝对/越界 exploit 路径在读取层也被拒绝（read_package_file 纵深防御）。
    let dir = tempfile::tempdir().unwrap();
    write_package(dir.path(), BASE_META, &[]);
    let err = read_awdp_script(dir.path(), "awdp/../../etc/passwd").unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("awdp") || msg.contains("INVALID_PATH") || msg.contains(".."),
        "read_awdp_script must reject traversal: {msg}"
    );
}

// ────────────────────────────────────────────────────────────────────────────
// §77：manifest 合法但脚本文件缺失 → import 物化阶段 fail
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn missing_awdp_script_file_on_disk_fails() {
    let dir = tempfile::tempdir().unwrap();
    let meta_toml = format!(
        r#"{BASE_META}
[awdp]
exploit_script = "awdp/exploit.py"
source_code_dir = "/var/www/html"
"#
    );
    // 声明了 awdp/exploit.py，但包内没有该文件。
    write_package(dir.path(), &meta_toml, &[("src/index.php", "<?php")]);

    // manifest 本身解析通过（字段齐全）。
    fcmc::GameBoxMeta::parse_and_validate(&meta_toml).expect("manifest ok");

    // 但 import 物化阶段读取脚本失败 → FILE_NOT_FOUND（[awdp] 缺文件 = fail）。
    let err = read_awdp_script(dir.path(), "awdp/exploit.py").unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("FILE_NOT_FOUND") || msg.contains("not found"),
        "missing script file must fail: {msg}"
    );
}
