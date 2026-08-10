//! 配置校验逻辑（application/check.rs）测试。
//!
//! 所有用例在临时目录中构造 meta.toml，验证 check_challenge / check_gamebox
//! 对成功、缺失附件、无 Docker、非法资源值、坏 TOML 等场景的判定。

use fcmc::application::check::{CheckLevel, check_challenge, check_gamebox};

fn write_meta(dir: &std::path::Path, content: &str) -> std::path::PathBuf {
    let path = dir.join("meta.toml");
    std::fs::write(&path, content).unwrap();
    path
}

fn has_level(
    result: &fcmc::application::check::CheckResult,
    level: CheckLevel,
    section: &str,
) -> bool {
    result
        .messages
        .iter()
        .any(|m| m.level as u8 == level as u8 && m.section == section)
}

const VALID_CHALLENGE: &str = r#"
name = "test"
version = "1.0.0"
author = "test@example.com"
category = "Web"
description = "desc"

[flag]
type = "dynamic"

[docker]
port = 80
"#;

const VALID_GAMEBOX: &str = r#"
name = "gb"
version = "1.0.0"
author = "test@example.com"
category = "web"
description = "desc"

[gamebox]
username = "ctf"
"#;

/// Valid gamebox package layout: meta + src/Dockerfile (+ optional judge).
fn setup_valid_gamebox_pkg(dir: &std::path::Path, meta: &str) {
    write_meta(dir, meta);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/Dockerfile"), "FROM scratch\n").unwrap();
}

// ─── check_challenge ────────────────────────────────────────────────

#[test]
fn challenge_valid_passes() {
    let tmp = tempfile::TempDir::new().unwrap();
    write_meta(tmp.path(), VALID_CHALLENGE);

    let result = check_challenge(tmp.path()).unwrap();
    assert!(
        result.passed,
        "valid challenge must pass: {:?}",
        result.messages
    );
    assert!(has_level(&result, CheckLevel::Ok, "解析结果"));
    // 未配置附件 → WARN 但不失败
    assert!(has_level(&result, CheckLevel::Warn, "附件检查"));
    // Docker 配置存在 → OK
    assert!(has_level(&result, CheckLevel::Ok, "Docker 检查"));
}

#[test]
fn challenge_with_existing_attachment_passes() {
    let tmp = tempfile::TempDir::new().unwrap();
    std::fs::create_dir_all(tmp.path().join("attachment")).unwrap();
    std::fs::write(tmp.path().join("attachment/src.zip"), b"zip").unwrap();
    let toml = r#"
name = "test"
version = "1.0.0"
author = "test@example.com"
category = "Web"
description = "desc"
attachment = "attachment/src.zip"

[flag]
type = "dynamic"

[docker]
port = 80
"#;
    write_meta(tmp.path(), toml);

    let result = check_challenge(tmp.path()).unwrap();
    assert!(result.passed);
    assert!(has_level(&result, CheckLevel::Ok, "附件检查"));
}

#[test]
fn challenge_missing_attachment_fails() {
    let tmp = tempfile::TempDir::new().unwrap();
    let toml = r#"
name = "test"
version = "1.0.0"
author = "test@example.com"
category = "Web"
description = "desc"
attachment = "attachment/not-exists.zip"

[flag]
type = "dynamic"

[docker]
port = 80
"#;
    write_meta(tmp.path(), toml);

    let result = check_challenge(tmp.path()).unwrap();
    assert!(!result.passed, "missing attachment must fail");
    assert!(has_level(&result, CheckLevel::Err, "附件检查"));
}

#[test]
fn challenge_invalid_toml_fails() {
    let tmp = tempfile::TempDir::new().unwrap();
    write_meta(tmp.path(), "not valid toml {{{");

    let result = check_challenge(tmp.path()).unwrap();
    assert!(!result.passed);
    assert!(has_level(&result, CheckLevel::Err, "解析结果"));
}

#[test]
fn challenge_missing_meta_file_errors() {
    let tmp = tempfile::TempDir::new().unwrap();
    let err = check_challenge(tmp.path()).unwrap_err();
    assert!(err.to_string().contains("meta.toml"));
}

#[test]
fn challenge_docker_port_zero_fails() {
    let tmp = tempfile::TempDir::new().unwrap();
    let toml = r#"
name = "test"
version = "1.0.0"
author = "test@example.com"
category = "Web"
description = "desc"

[flag]
type = "dynamic"

[docker]
port = 0
"#;
    write_meta(tmp.path(), toml);

    let result = check_challenge(tmp.path()).unwrap();
    assert!(!result.passed, "port 0 must fail");
    assert!(has_level(&result, CheckLevel::Err, "解析结果"));
}

#[test]
fn challenge_zero_resource_fails() {
    let tmp = tempfile::TempDir::new().unwrap();
    let toml = r#"
name = "test"
version = "1.0.0"
author = "test@example.com"
category = "Web"
description = "desc"

[flag]
type = "dynamic"

[docker]
port = 80

[docker.recommended_resources]
cpu_millis = 0
memory_bytes = 268435456
pids_limit = 100
"#;
    write_meta(tmp.path(), toml);

    let result = check_challenge(tmp.path()).unwrap();
    assert!(!result.passed, "zero cpu_millis must fail");
    assert!(has_level(&result, CheckLevel::Err, "解析结果"));
}

#[test]
fn challenge_with_resources_reports_ok() {
    let tmp = tempfile::TempDir::new().unwrap();
    let toml = r#"
name = "test"
version = "1.0.0"
author = "test@example.com"
category = "Web"
description = "desc"

[flag]
type = "dynamic"

[docker]
port = 80

[docker.recommended_resources]
cpu_millis = 500
memory_bytes = 268435456
pids_limit = 100
"#;
    write_meta(tmp.path(), toml);

    let result = check_challenge(tmp.path()).unwrap();
    assert!(result.passed);
    assert!(has_level(&result, CheckLevel::Ok, "资源配置"));
}

#[test]
fn challenge_legacy_image_tag_fails() {
    let tmp = tempfile::TempDir::new().unwrap();
    let toml = r#"
name = "test"
version = "1.0.0"
author = "test@example.com"
category = "Web"
description = "desc"
image_tag = "x:v1"

[flag]
type = "dynamic"
"#;
    write_meta(tmp.path(), toml);

    let result = check_challenge(tmp.path()).unwrap();
    assert!(!result.passed, "legacy image_tag must fail");
    assert!(has_level(&result, CheckLevel::Err, "解析结果"));
}

// ─── check_gamebox ──────────────────────────────────────────────────

#[test]
fn gamebox_valid_passes() {
    let tmp = tempfile::TempDir::new().unwrap();
    setup_valid_gamebox_pkg(tmp.path(), VALID_GAMEBOX);

    let result = check_gamebox(tmp.path()).unwrap();
    assert!(
        result.passed,
        "valid gamebox must pass: {:?}",
        result.messages
    );
    assert!(has_level(&result, CheckLevel::Ok, "解析结果"));
    assert!(has_level(&result, CheckLevel::Ok, "Dockerfile"));
}

#[test]
fn gamebox_missing_dockerfile_fails() {
    let tmp = tempfile::TempDir::new().unwrap();
    write_meta(tmp.path(), VALID_GAMEBOX);

    let result = check_gamebox(tmp.path()).unwrap();
    assert!(!result.passed, "missing Dockerfile must fail");
    assert!(has_level(&result, CheckLevel::Err, "Dockerfile"));
}

#[test]
fn gamebox_zero_cpu_fails() {
    let tmp = tempfile::TempDir::new().unwrap();
    let toml = format!(
        "{}\n\n[gamebox.recommended_resources]\ncpu_millis = 0\nmemory_bytes = 1\npids_limit = 1\n",
        VALID_GAMEBOX
    );
    setup_valid_gamebox_pkg(tmp.path(), &toml);

    let result = check_gamebox(tmp.path()).unwrap();
    assert!(!result.passed, "zero cpu_millis must fail");
    assert!(has_level(&result, CheckLevel::Err, "解析结果"));
}

#[test]
fn gamebox_zero_memory_fails() {
    let tmp = tempfile::TempDir::new().unwrap();
    let toml = format!(
        "{}\n\n[gamebox.recommended_resources]\ncpu_millis = 1\nmemory_bytes = 0\npids_limit = 1\n",
        VALID_GAMEBOX
    );
    setup_valid_gamebox_pkg(tmp.path(), &toml);

    let result = check_gamebox(tmp.path()).unwrap();
    assert!(!result.passed, "zero memory_bytes must fail");
}

#[test]
fn gamebox_judge_script_missing_fails() {
    let tmp = tempfile::TempDir::new().unwrap();
    let toml = format!(
        "{}\n\n[judge]\nscript = \"judge/check.py\"\n",
        VALID_GAMEBOX
    );
    setup_valid_gamebox_pkg(tmp.path(), &toml);

    let result = check_gamebox(tmp.path()).unwrap();
    assert!(!result.passed, "missing judge script must fail");
    assert!(has_level(&result, CheckLevel::Err, "Judge"));
}

#[test]
fn gamebox_judge_script_present_passes() {
    let tmp = tempfile::TempDir::new().unwrap();
    let toml = format!(
        "{}\n\n[judge]\nscript = \"judge/check.py\"\n",
        VALID_GAMEBOX
    );
    setup_valid_gamebox_pkg(tmp.path(), &toml);
    std::fs::create_dir_all(tmp.path().join("judge")).unwrap();
    std::fs::write(tmp.path().join("judge/check.py"), "print('ok')\n").unwrap();

    let result = check_gamebox(tmp.path()).unwrap();
    assert!(
        result.passed,
        "gamebox with judge script must pass: {:?}",
        result.messages
    );
    assert!(has_level(&result, CheckLevel::Ok, "Judge"));
}

#[test]
fn gamebox_legacy_image_tag_fails() {
    let tmp = tempfile::TempDir::new().unwrap();
    let toml = r#"
name = "gb"
version = "1.0.0"
author = "a"
category = "web"
description = "d"
[gamebox]
username = "ctf"
image_tag = "x:y"
"#;
    setup_valid_gamebox_pkg(tmp.path(), toml);
    let result = check_gamebox(tmp.path()).unwrap();
    assert!(!result.passed);
}

#[test]
fn gamebox_invalid_toml_fails() {
    let tmp = tempfile::TempDir::new().unwrap();
    write_meta(tmp.path(), "not valid {{{");

    let result = check_gamebox(tmp.path()).unwrap();
    assert!(!result.passed);
}

#[test]
fn gamebox_missing_meta_file_errors() {
    let tmp = tempfile::TempDir::new().unwrap();
    let err = check_gamebox(tmp.path()).unwrap_err();
    assert!(err.to_string().contains("meta.toml"));
}
