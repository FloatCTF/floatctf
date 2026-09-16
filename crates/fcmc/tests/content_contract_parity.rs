//! `content_contract_parity` — FCMC 与 floatctf-content 的 Content Contract 一致性。
//!
//! 权威实现：`FloatCTF/floatctf-content@main`
//! （`scripts/content.py` + `scripts/tests/test_content.py` + `catalog.json`）。
//! 本测试直接使用该仓库的 fixture 副本（见
//! `tests/fixtures/content_contract/README.md`）。
//!
//! 覆盖：
//! 1. canonical Challenge（container）能被 FCMC 接受
//! 2. canonical static Challenge 能被 FCMC 接受
//! 3. canonical GameBox（**无** `[gamebox]` 段）能以 GameBox 模式被接受
//! 4. `safe_name` 逐例与 Python 一致
//! 5. `version` 与 Python 一致
//! 6. image ref 与 `catalog.json` 一致
//! 7. GameBox 的 AWD 扩展（judge / awdp / healthchecks / username）仍然可用
//! 8. `[docker.recommended_resources]` 迁移（含 partial 与 static 忽略）

use std::path::{Path, PathBuf};

use fcmc::application::check::{CheckLevel, check_challenge, check_gamebox};
use fcmc::application::package::has_dockerfile;
use fcmc::metadata::identity::{
    derive_safe_name, is_valid_safe_name, is_valid_version, resolve_safe_name,
};
use fcmc::{
    ArtifactKind, CONTENT_IMAGE_NAMESPACE, ChallengeFlagConfig, ChallengeMeta, Difficulty,
    GameBoxHealthcheck, GameBoxMeta, RecommendedResources, content_image_ref,
};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/content_contract")
}

fn local_fixture(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(relative)
}

fn read(dir: &Path) -> String {
    std::fs::read_to_string(dir.join("meta.toml")).expect("meta.toml")
}

/// content id = 目录名（floatctf-content 的唯一身份来源）。
fn content_id(dir: &Path) -> String {
    dir.file_name().unwrap().to_str().unwrap().to_string()
}

fn challenge_dir(id: &str) -> PathBuf {
    fixtures().join("challenges").join(id)
}

fn gamebox_dir(id: &str) -> PathBuf {
    fixtures().join("gameboxes").join(id)
}

fn level_present(
    result: &fcmc::application::check::CheckResult,
    level: CheckLevel,
    section: &str,
) -> bool {
    result
        .messages
        .iter()
        .any(|m| m.level as u8 == level as u8 && m.section == section)
}

// ---------------------------------------------------------------------------
// 1. canonical Challenge（container）
// ---------------------------------------------------------------------------

#[test]
fn canonical_container_challenge_is_accepted() {
    let dir = challenge_dir("comment");
    let id = content_id(&dir);
    assert_eq!(id, "comment");

    let meta = ChallengeMeta::parse_and_validate(&read(&dir), &id).expect("canonical challenge");
    assert_eq!(meta.name, "comment");
    assert_eq!(meta.version, "1.0.0");
    assert_eq!(meta.author, "fb0sh@outlook.com");
    assert_eq!(meta.category, "web");
    assert_eq!(meta.difficulty, Difficulty::Easy);
    assert_eq!(meta.tags, vec!["php".to_string(), "web".to_string()]);
    assert_eq!(meta.description, "注释里面有什么？");
    assert!(matches!(meta.flag, Some(ChallengeFlagConfig::Dynamic)));

    let safe_name = meta.resolved_safe_name(&id).unwrap();
    assert_eq!(safe_name, "comment");
    assert_eq!(
        content_image_ref(
            ArtifactKind::Challenge,
            CONTENT_IMAGE_NAMESPACE,
            &safe_name,
            "1.0.0"
        ),
        "floatctf/comment:challenge-v1.0.0"
    );

    let docker = meta.docker.as_ref().unwrap();
    assert_eq!(docker.port, Some(80));
    let res = docker.materialize_resources(RecommendedResources::CHALLENGE_DEFAULTS);
    assert_eq!(res.cpu_millis, 500);
    assert_eq!(res.memory_bytes, 268_435_456);
    assert_eq!(res.pids_limit, 100);

    // FCMC check 接受（container content：fixture 目录含 src/Dockerfile）
    assert!(has_dockerfile(&dir));
    let result = check_challenge(&dir).unwrap();
    assert!(
        result.passed,
        "canonical challenge must pass: {:?}",
        result.messages
    );
    assert!(level_present(&result, CheckLevel::Ok, "Content Contract"));
    assert!(level_present(&result, CheckLevel::Ok, "Dockerfile"));
}

// ---------------------------------------------------------------------------
// 2. canonical static Challenge
// ---------------------------------------------------------------------------

#[test]
fn canonical_static_challenge_is_accepted() {
    let dir = challenge_dir("cookie");
    let id = content_id(&dir);
    assert_eq!(id, "cookie");

    let meta = ChallengeMeta::parse_and_validate(&read(&dir), &id).expect("static challenge");
    assert_eq!(meta.difficulty, Difficulty::Unknown);
    assert!(meta.tags.is_empty());
    assert!(meta.docker.is_none());
    assert_eq!(
        meta.static_flag_value(),
        Some("flag{fixture-flag-must-not-leak}")
    );

    // static / attachment-only：没有 src/Dockerfile
    assert!(!has_dockerfile(&dir));
    let result = check_challenge(&dir).unwrap();
    assert!(
        result.passed,
        "static challenge metadata is valid; static-ness is a WARN not a contract error: {:?}",
        result.messages
    );
    assert!(level_present(&result, CheckLevel::Warn, "Dockerfile"));

    // 静态 flag 明文绝不进入 normalized spec
    let norm = meta.normalize(&id).unwrap();
    let spec_json = serde_json::to_string(&norm).unwrap();
    assert!(!spec_json.contains("fixture-flag-must-not-leak"));
    assert_eq!(norm.flag_type.as_deref(), Some("static"));
}

// ---------------------------------------------------------------------------
// 3. canonical GameBox（无 [gamebox] 段）
// ---------------------------------------------------------------------------

#[test]
fn canonical_minimal_gamebox_without_gamebox_section_is_accepted() {
    let dir = gamebox_dir("comment");
    let id = content_id(&dir);
    assert_eq!(id, "comment");

    // 以 GameBox 模式解析
    let meta = GameBoxMeta::parse_and_validate(&read(&dir), &id).expect("canonical gamebox");
    assert!(meta.gamebox.is_none(), "canonical GameBox has no [gamebox]");
    assert_eq!(meta.difficulty, Difficulty::Medium);
    assert_eq!(meta.tags, vec!["box".to_string()]);
    assert_eq!(meta.description, "GameBox fixture");
    assert_eq!(meta.docker.as_ref().unwrap().port, Some(8080));

    let safe_name = meta.resolved_safe_name(&id).unwrap();
    assert_eq!(safe_name, "comment");
    assert_eq!(
        content_image_ref(
            ArtifactKind::GameBox,
            CONTENT_IMAGE_NAMESPACE,
            &safe_name,
            "1.0.0"
        ),
        "floatctf/comment:gamebox-v1.0.0"
    );

    // 缺 [gamebox] 只影响运行时操作，不做 metadata 判决
    assert!(meta.require_gamebox_section().is_err());
    let result = check_gamebox(&dir).unwrap();
    assert!(
        result.passed,
        "canonical gamebox must pass: {:?}",
        result.messages
    );
    assert!(level_present(&result, CheckLevel::Warn, "gamebox"));

    // normalized spec 不丢字段，username 为 None
    let norm = meta.normalize(&id).unwrap();
    assert!(norm.username.is_none());
    assert!(norm.healthchecks.is_empty());
    assert_eq!(norm.difficulty, Difficulty::Medium);
    assert_eq!(
        norm.recommended_resources,
        RecommendedResources::GAMEBOX_DEFAULTS
    );
}

// ---------------------------------------------------------------------------
// 4. safe_name parity（逐例对齐 content.py::derive_safe_name）
// ---------------------------------------------------------------------------

#[test]
fn safe_name_parity_with_python() {
    // 来自 scripts/tests/test_content.py::SafeNameTests
    for (input, expected) in [
        ("comment", Some("comment")),
        ("Android_reverse", Some("android_reverse")),
        ("FloatCTF-qidong", Some("floatctf-qidong")),
        (
            "Cirno's perfect math class",
            Some("cirnos-perfect-math-class"),
        ),
        ("Cirno\u{2019}s book", Some("cirnos-book")),
        ("komachi's book", Some("komachis-book")),
        ("orin's pack", Some("orins-pack")),
        ("Flag_in_the_model", Some("flag_in_the_model")),
        ("foo bar", Some("foo-bar")),
        ("foo   bar", Some("foo-bar")),
        ("foo__bar", Some("foo-bar")),
        ("--Foo..Bar--", Some("foo-bar")),
        ("foo.bar", Some("foo.bar")),
        ("题目", None),
    ] {
        assert_eq!(
            derive_safe_name(input).as_deref(),
            expected,
            "derive_safe_name({input:?}) must match Python"
        );
    }

    // 派生结果必须全部匹配官方 SAFE_NAME_PATTERN
    for id in [
        "comment",
        "Android_reverse",
        "FloatCTF-qidong",
        "Cirno's perfect math class",
        "komachi's book",
        "orin's pack",
        "Flag_in_the_model",
    ] {
        let derived = derive_safe_name(id).expect("derivable");
        assert!(is_valid_safe_name(&derived), "{id} → {derived}");
    }

    // 显式值优先 + trim；空/空白不回退派生
    assert_eq!(
        resolve_safe_name("Cirno's perfect math class", Some("custom-name")).unwrap(),
        "custom-name"
    );
    assert_eq!(
        resolve_safe_name("题目", Some("challenge-001")).unwrap(),
        "challenge-001"
    );
    assert_eq!(
        resolve_safe_name("comment", Some(" custom-name ")).unwrap(),
        "custom-name"
    );
    assert!(resolve_safe_name("comment", Some("")).is_err());
    assert!(resolve_safe_name("comment", Some("   ")).is_err());
    assert!(resolve_safe_name("comment", Some("Foo Bar")).is_err());
    assert!(resolve_safe_name("题目", None).is_err());
}

#[test]
fn safe_name_parity_against_official_fixtures() {
    // scripts/tests/fixtures/safe-names + valid
    for (dir, expected) in [
        (challenge_dir("Android_reverse"), "android_reverse"),
        (challenge_dir("FloatCTF-qidong"), "floatctf-qidong"),
        (
            challenge_dir("Cirno's perfect math class"),
            "cirnos-perfect-math-class",
        ),
        (challenge_dir("题目"), "challenge-001"),
        (gamebox_dir("Android_reverse"), "android_reverse"),
        (gamebox_dir("comment"), "comment"),
    ] {
        let id = content_id(&dir);
        let raw = read(&dir);
        let safe = if dir.starts_with(fixtures().join("gameboxes")) {
            GameBoxMeta::parse_and_validate(&raw, &id)
                .unwrap()
                .resolved_safe_name(&id)
                .unwrap()
        } else {
            ChallengeMeta::parse_and_validate(&raw, &id)
                .unwrap()
                .resolved_safe_name(&id)
                .unwrap()
        };
        assert_eq!(safe, expected, "safe_name for content id {id:?}");
    }
}

// ---------------------------------------------------------------------------
// 5. version parity
// ---------------------------------------------------------------------------

#[test]
fn version_parity_with_python() {
    // content.py: VERSION_PATTERN = ^\d+\.\d+\.\d+$
    for ok in ["1.0.0", "0.0.1", "12.34.56", "01.0.0"] {
        assert!(
            is_valid_version(ok),
            "{ok} must be accepted (Python parity)"
        );
    }
    for bad in [
        "1.0",
        "v1.0.0",
        "1.0.0-rc.1",
        "1.0.0+build",
        "1.0.0.0",
        "",
        "abc",
    ] {
        assert!(!is_valid_version(bad), "{bad} must be rejected");
    }
}

// ---------------------------------------------------------------------------
// 6. image ref parity（与 catalog.json 一致）
// ---------------------------------------------------------------------------

#[test]
fn image_ref_parity_with_catalog() {
    for (kind, safe_name, version, expected) in [
        (
            ArtifactKind::Challenge,
            "comment",
            "1.0.0",
            "floatctf/comment:challenge-v1.0.0",
        ),
        (
            ArtifactKind::Challenge,
            "android_reverse",
            "1.0.0",
            "floatctf/android_reverse:challenge-v1.0.0",
        ),
        (
            ArtifactKind::Challenge,
            "floatctf-qidong",
            "1.0.0",
            "floatctf/floatctf-qidong:challenge-v1.0.0",
        ),
        (
            ArtifactKind::Challenge,
            "challenge-001",
            "2.0.0",
            "floatctf/challenge-001:challenge-v2.0.0",
        ),
        (
            ArtifactKind::GameBox,
            "comment",
            "1.0.0",
            "floatctf/comment:gamebox-v1.0.0",
        ),
    ] {
        assert_eq!(
            content_image_ref(kind, CONTENT_IMAGE_NAMESPACE, safe_name, version),
            expected
        );
    }

    // Challenge 与 GameBox 可以共用 safe_name（tag 不同）
    let challenge = challenge_dir("Android_reverse");
    let gamebox = gamebox_dir("Android_reverse");
    let (cid, gid) = (content_id(&challenge), content_id(&gamebox));
    let c = ChallengeMeta::parse_and_validate(&read(&challenge), &cid).unwrap();
    let g = GameBoxMeta::parse_and_validate(&read(&gamebox), &gid).unwrap();
    let cs = c.resolved_safe_name(&cid).unwrap();
    let gs = g.resolved_safe_name(&gid).unwrap();
    assert_eq!(cs, gs);
    assert_ne!(
        content_image_ref(
            ArtifactKind::Challenge,
            CONTENT_IMAGE_NAMESPACE,
            &cs,
            &c.version
        ),
        content_image_ref(
            ArtifactKind::GameBox,
            CONTENT_IMAGE_NAMESPACE,
            &gs,
            &g.version
        )
    );
}

// ---------------------------------------------------------------------------
// 7. GameBox AWD 运行时扩展仍然可用
// ---------------------------------------------------------------------------

#[test]
fn gamebox_awd_extensions_still_work() {
    let dir = gamebox_dir("Android_reverse");
    let id = content_id(&dir);
    let raw = read(&dir);
    assert!(
        GameBoxMeta::parse_and_validate(&raw, &id).is_ok(),
        "canonical gamebox fixture without [gamebox] must parse"
    );
}

#[test]
fn gamebox_operational_extensions_judge_awdp_healthchecks_username() {
    let dir = local_fixture("gameboxes/test-gamebox-awdp");
    let id = content_id(&dir);

    let meta = GameBoxMeta::parse_and_validate(&read(&dir), &id).unwrap();
    let gamebox = meta.require_gamebox_section().expect("[gamebox] present");
    assert_eq!(gamebox.username, "ctf");
    assert_eq!(gamebox.healthchecks.len(), 1);
    assert!(matches!(
        gamebox.healthchecks[0],
        GameBoxHealthcheck::Http { port: 80, .. }
    ));

    let judge = meta.judge.as_ref().expect("[judge] preserved");
    assert_eq!(judge.script, "judge/check.py");

    let awdp = meta.awdp.as_ref().expect("[awdp] preserved");
    assert_eq!(awdp.exploit_script, "awdp/exploit.py");
    assert_eq!(awdp.source_code_dir, "/var/www/html");

    let norm = meta.normalize(&id).unwrap();
    assert_eq!(norm.username.as_deref(), Some("ctf"));
    assert_eq!(norm.judge_script.as_deref(), Some("judge/check.py"));
    assert_eq!(norm.exploit_script.as_deref(), Some("awdp/exploit.py"));
    assert_eq!(norm.source_code_dir.as_deref(), Some("/var/www/html"));
}

// ---------------------------------------------------------------------------
// 8. [docker.recommended_resources] 迁移
// ---------------------------------------------------------------------------

#[test]
fn gamebox_resources_come_from_docker_section_and_allow_partial() {
    let dir = local_fixture("gameboxes/test-gamebox-hc");
    let id = content_id(&dir);
    let raw = read(&dir);

    // fixture 里只有 [docker.recommended_resources]，没有 [gamebox.recommended_resources]
    assert!(raw.contains("[docker.recommended_resources]"));
    assert!(!raw.contains("[gamebox.recommended_resources]"));

    let norm = GameBoxMeta::parse_and_validate(&raw, &id)
        .unwrap()
        .normalize(&id)
        .unwrap();
    assert_eq!(
        norm.recommended_resources,
        RecommendedResources::GAMEBOX_DEFAULTS
    );

    // partial：只写 cpu_millis 也合法，其余用默认值物化
    let partial = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "misc"
difficulty = "easy"
tags = []
description = "d"

[docker]
port = 8080

[docker.recommended_resources]
cpu_millis = 250
"#;
    let meta = GameBoxMeta::parse_and_validate(partial, "t").unwrap();
    let norm = meta.normalize("t").unwrap();
    assert_eq!(norm.recommended_resources.cpu_millis, 250);
    assert_eq!(norm.recommended_resources.memory_bytes, 536_870_912);
    assert_eq!(norm.recommended_resources.pids_limit, 100);

    // 旧的 canonical 位置（[gamebox.recommended_resources]）被删除
    let legacy = format!("{partial}\n[gamebox.recommended_resources]\ncpu_millis = 1\n");
    assert!(GameBoxMeta::from_toml_str(&legacy).is_err());
}

#[test]
fn static_content_with_docker_table_remains_static() {
    let dir = challenge_dir("static_with_docker");
    let id = content_id(&dir);
    let raw = read(&dir);
    assert!(raw.contains("[docker]"), "fixture declares [docker]");

    // metadata 合法，[docker] 被解析……
    let meta = ChallengeMeta::parse_and_validate(&raw, &id).unwrap();
    assert_eq!(meta.docker.as_ref().unwrap().port, Some(8080));
    // ……但是否为容器只看 src/Dockerfile
    assert!(!has_dockerfile(&dir));
    let result = check_challenge(&dir).unwrap();
    assert!(result.passed, "{:?}", result.messages);
    assert!(level_present(&result, CheckLevel::Warn, "Dockerfile"));
}

// ---------------------------------------------------------------------------
// 9. 全量：官方 fixture 目录在各自模式下都能通过 contract 校验
// ---------------------------------------------------------------------------

#[test]
fn all_official_fixtures_parse_in_their_mode() {
    let mut checked = 0;

    for (kind, base) in [
        (ArtifactKind::Challenge, fixtures().join("challenges")),
        (ArtifactKind::GameBox, fixtures().join("gameboxes")),
    ] {
        for entry in std::fs::read_dir(&base).unwrap() {
            let entry = entry.unwrap();
            let dir = entry.path();
            if !dir.is_dir() {
                continue;
            }
            let id = content_id(&dir);
            let raw = read(&dir);

            let safe_name = match kind {
                ArtifactKind::Challenge => {
                    let meta = ChallengeMeta::parse_and_validate(&raw, &id)
                        .unwrap_or_else(|e| panic!("{} must be valid: {e}", dir.display()));
                    meta.resolved_safe_name(&id).unwrap()
                }
                ArtifactKind::GameBox => {
                    let meta = GameBoxMeta::parse_and_validate(&raw, &id)
                        .unwrap_or_else(|e| panic!("{} must be valid: {e}", dir.display()));
                    meta.resolved_safe_name(&id).unwrap()
                }
            };

            // safe_name 必须始终是合法 Docker repository 名
            assert!(is_valid_safe_name(&safe_name), "{id} → {safe_name}");
            checked += 1;
        }
    }

    // challenges: 7（comment / cookie / Android_reverse / Cirno's /
    // FloatCTF-qidong / static_with_docker / 题目）+ gameboxes: 2 = 9
    assert_eq!(checked, 9, "expected the full official fixture set");
}
