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
author = "test@example.com"
category = "Web"
description = "desc"

[flag]
value = "flag{test}"
env_var = "FLAG"

[docker]
image_tag = "test/challenge:v1"
port = "80/tcp"
"#;

const VALID_GAMEBOX: &str = r#"
name = "gb"
author = "test@example.com"
category = "Web"
description = "desc"

[gamebox]
username = "ctf"
image_tag = "test/gamebox:v1"
"#;

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
author = "test@example.com"
category = "Web"
description = "desc"
attachment = "attachment/src.zip"

[flag]
value = "flag{test}"
env_var = "FLAG"

[docker]
image_tag = "test/challenge:v1"
port = "80/tcp"
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
author = "test@example.com"
category = "Web"
description = "desc"
attachment = "attachment/not-exists.zip"

[flag]
value = "flag{test}"
env_var = "FLAG"

[docker]
image_tag = "test/challenge:v1"
port = "80/tcp"
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

// ─── check_gamebox ──────────────────────────────────────────────────

#[test]
fn gamebox_valid_passes() {
    let tmp = tempfile::TempDir::new().unwrap();
    write_meta(tmp.path(), VALID_GAMEBOX);

    let result = check_gamebox(tmp.path()).unwrap();
    assert!(
        result.passed,
        "valid gamebox must pass: {:?}",
        result.messages
    );
    assert!(has_level(&result, CheckLevel::Ok, "解析结果"));
}

#[test]
fn gamebox_zero_cpu_fails() {
    let tmp = tempfile::TempDir::new().unwrap();
    let toml = format!("{}\n\n[gamebox.resources]\ncpu_millis = 0\n", VALID_GAMEBOX);
    write_meta(tmp.path(), &toml);

    let result = check_gamebox(tmp.path()).unwrap();
    assert!(!result.passed, "zero cpu_millis must fail");
    assert!(has_level(&result, CheckLevel::Err, "资源配置"));
}

#[test]
fn gamebox_zero_memory_fails() {
    let tmp = tempfile::TempDir::new().unwrap();
    let toml = format!(
        "{}\n\n[gamebox.resources]\nmemory_bytes = 0\n",
        VALID_GAMEBOX
    );
    write_meta(tmp.path(), &toml);

    let result = check_gamebox(tmp.path()).unwrap();
    assert!(!result.passed, "zero memory_bytes must fail");
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
