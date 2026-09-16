//! check 用例集成测试。
//!
//! 重点覆盖两层检查的边界：
//! * Content Contract 问题 → ERR（metadata contract invalid）
//! * operational 问题（附件缺失 / 缺 Dockerfile / 缺 [gamebox]）→ 明确的操作级信息

use fcmc::application::check::{CheckLevel, CheckResult, check_challenge, check_gamebox};

fn write_meta(dir: &std::path::Path, content: &str) {
    std::fs::write(dir.join("meta.toml"), content).unwrap();
}

fn has_level(result: &CheckResult, level: CheckLevel, section: &str) -> bool {
    result
        .messages
        .iter()
        .any(|m| m.level as u8 == level as u8 && m.section == section)
}

fn message_containing(result: &CheckResult, needle: &str) -> bool {
    result.messages.iter().any(|m| m.message.contains(needle))
}

/// 合法 Challenge 目录（目录名 = content id）。返回 TempDir，调用方需要持有它。
fn challenge_pkg(meta: &str) -> tempfile::TempDir {
    let tmp = tempfile::TempDir::new().unwrap();
    write_meta(tmp.path(), meta);
    tmp
}

fn gamebox_pkg(meta: &str, dockerfile: bool) -> tempfile::TempDir {
    let tmp = tempfile::TempDir::new().unwrap();
    write_meta(tmp.path(), meta);
    if dockerfile {
        std::fs::create_dir_all(tmp.path().join("src")).unwrap();
        std::fs::write(tmp.path().join("src/Dockerfile"), "FROM scratch\n").unwrap();
    }
    tmp
}

const VALID_CHALLENGE: &str = r#"
name = "test"
version = "1.0.0"
author = "test@example.com"
category = "Web"
difficulty = "easy"
tags = ["web"]
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
difficulty = "easy"
tags = []
description = "desc"

[gamebox]
username = "ctf"
"#;

// ─── check_challenge ────────────────────────────────────────────────

#[test]
fn challenge_valid_passes() {
    let tmp = challenge_pkg(VALID_CHALLENGE);
    let result = check_challenge(tmp.path()).unwrap();
    assert!(
        result.passed,
        "valid challenge must pass: {:?}",
        result.messages
    );
    assert!(has_level(&result, CheckLevel::Ok, "Content Contract"));
    // 未配置附件 → WARN 但不失败
    assert!(has_level(&result, CheckLevel::Warn, "附件检查"));
    // Docker 配置存在 → OK
    assert!(has_level(&result, CheckLevel::Ok, "Docker 检查"));
}

/// 在顶层（第一个 table header 之前）插入内容，避免意外落入 `[docker]` 表。
fn challenge_with_top_level(extra: &str) -> String {
    let marker = "\n[flag]";
    VALID_CHALLENGE.replacen(marker, &format!("\n{extra}{marker}"), 1)
}

#[test]
fn challenge_with_existing_attachment_passes() {
    let tmp = challenge_pkg(&challenge_with_top_level(
        "attachment = \"attachment/src.zip\"",
    ));
    std::fs::create_dir_all(tmp.path().join("attachment")).unwrap();
    std::fs::write(tmp.path().join("attachment/src.zip"), b"zip").unwrap();

    let result = check_challenge(tmp.path()).unwrap();
    assert!(result.passed, "{:?}", result.messages);
    assert!(has_level(&result, CheckLevel::Ok, "附件检查"));
}

#[test]
fn challenge_missing_attachment_fails() {
    let tmp = challenge_pkg(&challenge_with_top_level(
        "attachment = \"attachment/not-exists.zip\"",
    ));
    let result = check_challenge(tmp.path()).unwrap();
    assert!(!result.passed, "missing attachment must fail");
    assert!(has_level(&result, CheckLevel::Err, "附件检查"));
}

#[test]
fn challenge_invalid_toml_fails_as_contract_error() {
    let tmp = challenge_pkg("not valid toml {{{");
    let result = check_challenge(tmp.path()).unwrap();
    assert!(!result.passed);
    assert!(has_level(&result, CheckLevel::Err, "Content Contract"));
    assert!(message_containing(&result, "metadata contract invalid"));
}

#[test]
fn challenge_missing_meta_file_errors() {
    let tmp = tempfile::TempDir::new().unwrap();
    let err = check_challenge(tmp.path()).unwrap_err();
    assert!(err.to_string().contains("meta.toml"));
}

#[test]
fn challenge_missing_difficulty_is_contract_error() {
    let tmp = challenge_pkg(
        r#"
name = "test"
version = "1.0.0"
author = "test@example.com"
category = "Web"
tags = []
description = "desc"
"#,
    );
    let result = check_challenge(tmp.path()).unwrap();
    assert!(!result.passed);
    assert!(has_level(&result, CheckLevel::Err, "Content Contract"));
}

#[test]
fn challenge_underrivable_safe_name_is_contract_error() {
    // 目录名 "题目" 无法派生 safe_name
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("题目");
    std::fs::create_dir_all(&dir).unwrap();
    write_meta(&dir, VALID_CHALLENGE);

    let result = check_challenge(&dir).unwrap();
    assert!(!result.passed);
    assert!(has_level(&result, CheckLevel::Err, "Content Contract"));
}

#[test]
fn challenge_docker_port_zero_fails() {
    let tmp = challenge_pkg(&VALID_CHALLENGE.replace("port = 80", "port = 0"));
    let result = check_challenge(tmp.path()).unwrap();
    assert!(!result.passed, "port 0 must fail");
    assert!(has_level(&result, CheckLevel::Err, "Content Contract"));
}

#[test]
fn challenge_zero_resource_fails() {
    let tmp = challenge_pkg(&format!(
        "{VALID_CHALLENGE}\n[docker.recommended_resources]\ncpu_millis = 0\n"
    ));
    let result = check_challenge(tmp.path()).unwrap();
    assert!(!result.passed, "zero cpu_millis must fail");
    assert!(has_level(&result, CheckLevel::Err, "Content Contract"));
}

#[test]
fn challenge_with_resources_reports_ok() {
    let tmp = challenge_pkg(&format!(
        "{VALID_CHALLENGE}\n[docker.recommended_resources]\ncpu_millis = 500\nmemory_bytes = 268435456\npids_limit = 100\n"
    ));
    let result = check_challenge(tmp.path()).unwrap();
    assert!(result.passed, "{:?}", result.messages);
    assert!(has_level(&result, CheckLevel::Ok, "资源配置"));
}

#[test]
fn challenge_without_flag_is_valid_and_reports_warn() {
    let tmp = challenge_pkg(
        r#"
name = "static-ish"
version = "1.0.0"
author = "a"
category = "misc"
difficulty = "unknown"
tags = []
description = "no flag declared"
"#,
    );
    let result = check_challenge(tmp.path()).unwrap();
    assert!(result.passed, "{:?}", result.messages);
    assert!(has_level(&result, CheckLevel::Warn, "flag"));
    // 无 src/Dockerfile → static 提示（WARN，不是 contract 错误）
    assert!(has_level(&result, CheckLevel::Warn, "Dockerfile"));
    assert!(message_containing(&result, "content is static"));
}

#[test]
fn challenge_legacy_image_tag_is_ignored_not_fatal() {
    // 官方 validator 忽略未知顶层字段，FCMC 不再因此判 metadata 不合法
    let tmp = challenge_pkg(&challenge_with_top_level("image_tag = \"x:v1\""));
    let result = check_challenge(tmp.path()).unwrap();
    assert!(result.passed, "{:?}", result.messages);
}

// ─── check_gamebox ──────────────────────────────────────────────────

#[test]
fn gamebox_valid_passes() {
    let tmp = gamebox_pkg(VALID_GAMEBOX, true);
    let result = check_gamebox(tmp.path()).unwrap();
    assert!(
        result.passed,
        "valid gamebox must pass: {:?}",
        result.messages
    );
    assert!(has_level(&result, CheckLevel::Ok, "Content Contract"));
    assert!(has_level(&result, CheckLevel::Ok, "Dockerfile"));
}

#[test]
fn gamebox_without_gamebox_section_is_valid_but_runtime_incomplete() {
    let tmp = gamebox_pkg(
        r#"
name = "comment"
version = "1.0.0"
author = "dev@floatctf.local"
category = "misc"
difficulty = "medium"
tags = ["box"]
description = "GameBox fixture"

[docker]
port = 8080
"#,
        true,
    );
    let result = check_gamebox(tmp.path()).unwrap();
    assert!(
        result.passed,
        "canonical gamebox without [gamebox] must pass: {:?}",
        result.messages
    );
    assert!(has_level(&result, CheckLevel::Ok, "Content Contract"));
    assert!(has_level(&result, CheckLevel::Warn, "gamebox"));
}

#[test]
fn gamebox_missing_dockerfile_fails_operationally() {
    let tmp = gamebox_pkg(VALID_GAMEBOX, false);
    let result = check_gamebox(tmp.path()).unwrap();
    assert!(!result.passed, "missing Dockerfile must fail");
    assert!(has_level(&result, CheckLevel::Err, "Dockerfile"));
    assert!(message_containing(&result, "content is static"));
}

#[test]
fn gamebox_zero_cpu_fails() {
    let tmp = gamebox_pkg(
        &format!("{VALID_GAMEBOX}\n[docker.recommended_resources]\ncpu_millis = 0\n"),
        true,
    );
    let result = check_gamebox(tmp.path()).unwrap();
    assert!(!result.passed, "zero cpu_millis must fail");
    assert!(has_level(&result, CheckLevel::Err, "Content Contract"));
}

#[test]
fn gamebox_zero_memory_fails() {
    let tmp = gamebox_pkg(
        &format!("{VALID_GAMEBOX}\n[docker.recommended_resources]\nmemory_bytes = 0\n"),
        true,
    );
    let result = check_gamebox(tmp.path()).unwrap();
    assert!(!result.passed, "zero memory_bytes must fail");
}

#[test]
fn gamebox_judge_script_missing_fails() {
    let tmp = gamebox_pkg(
        &format!("{VALID_GAMEBOX}\n[judge]\nscript = \"judge/check.py\"\n"),
        true,
    );
    let result = check_gamebox(tmp.path()).unwrap();
    assert!(!result.passed, "missing judge script must fail");
    assert!(has_level(&result, CheckLevel::Err, "Judge"));
}

#[test]
fn gamebox_judge_script_present_passes() {
    let tmp = gamebox_pkg(
        &format!("{VALID_GAMEBOX}\n[judge]\nscript = \"judge/check.py\"\n"),
        true,
    );
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

/// 在顶层（第一个 table header 之前）插入内容。
fn gamebox_with_top_level(extra: &str) -> String {
    let marker = "\n[gamebox]";
    VALID_GAMEBOX.replacen(marker, &format!("\n{extra}{marker}"), 1)
}

#[test]
fn gamebox_legacy_image_tag_is_ignored_not_fatal() {
    let tmp = gamebox_pkg(&gamebox_with_top_level("image_tag = \"x:y\""), true);
    let result = check_gamebox(tmp.path()).unwrap();
    assert!(result.passed, "{:?}", result.messages);
}

#[test]
fn gamebox_legacy_resources_key_is_rejected() {
    let tmp = gamebox_pkg(
        &format!("{VALID_GAMEBOX}\n[gamebox.recommended_resources]\ncpu_millis = 1000\n"),
        true,
    );
    let result = check_gamebox(tmp.path()).unwrap();
    assert!(
        !result.passed,
        "[gamebox.recommended_resources] must be rejected"
    );
    assert!(has_level(&result, CheckLevel::Err, "Content Contract"));
}

#[test]
fn gamebox_invalid_toml_fails() {
    let tmp = gamebox_pkg("not valid {{{", true);
    let result = check_gamebox(tmp.path()).unwrap();
    assert!(!result.passed);
    assert!(has_level(&result, CheckLevel::Err, "Content Contract"));
}

#[test]
fn gamebox_missing_meta_file_errors() {
    let tmp = tempfile::TempDir::new().unwrap();
    let err = check_gamebox(tmp.path()).unwrap_err();
    assert!(err.to_string().contains("meta.toml"));
}
