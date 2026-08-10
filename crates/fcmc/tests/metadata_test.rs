//! Metadata parsing and validation tests.

use fcmc::{
    ArtifactKind, ChallengeFlagConfig, ChallengeMeta, ChallengeMetaError, GameBoxHealthcheck,
    GameBoxMeta, GameBoxMetaError, NormalizedHealthcheck, build_artifact_image_ref,
    build_gamebox_image_ref, derive_safe_name, pick_repo_digest, split_image_ref,
    validate_judge_path, validate_safe_name, validate_version,
};
use std::path::Path;

// ─── ChallengeMeta Tests (v1 manifest) ──────────────────────────────

#[test]
fn challenge_parse_valid() {
    let content = std::fs::read_to_string(Path::new("tests/fixtures/challenge/meta.toml")).unwrap();
    let meta = ChallengeMeta::parse_and_validate(&content).unwrap();
    assert_eq!(meta.name, "test-challenge");
    assert_eq!(meta.version, "1.0.0");
    assert_eq!(meta.author, "tester@example.com");
    assert_eq!(meta.category, "Web");
    assert!(matches!(meta.flag, ChallengeFlagConfig::Dynamic));
    assert_eq!(meta.resolved_safe_name().unwrap(), "test-challenge");
    let docker = meta.docker.unwrap();
    assert_eq!(docker.port, 80);
    assert!(docker.recommended_resources.is_none());
}

#[test]
fn challenge_parse_no_docker() {
    let content =
        std::fs::read_to_string(Path::new("tests/fixtures/challenge/meta_no_docker.toml")).unwrap();
    let meta = ChallengeMeta::parse_and_validate(&content).unwrap();
    assert!(meta.docker.is_none());
    assert_eq!(meta.category, "Crypto");
}

#[test]
fn challenge_parse_with_attachment() {
    let content = std::fs::read_to_string(Path::new(
        "tests/fixtures/challenge/meta_with_attachment.toml",
    ))
    .unwrap();
    let meta = ChallengeMeta::parse_and_validate(&content).unwrap();
    assert_eq!(meta.attachment.as_deref(), Some("attachment/src.zip"));
}

#[test]
fn challenge_missing_name() {
    let toml = r#"
version = "1.0.0"
author = "test"
category = "Web"
description = "test"

[flag]
type = "dynamic"
"#;
    let result = ChallengeMeta::from_toml_str(toml);
    assert!(result.is_err());
}

#[test]
fn challenge_missing_flag() {
    let toml = r#"
name = "test"
version = "1.0.0"
author = "test"
category = "Web"
description = "test"
"#;
    let result = ChallengeMeta::from_toml_str(toml);
    assert!(result.is_err());
}

#[test]
fn challenge_missing_category() {
    let toml = r#"
name = "test"
version = "1.0.0"
author = "test"
description = "test"

[flag]
type = "dynamic"
"#;
    let err = ChallengeMeta::parse_and_validate(toml).unwrap_err();
    assert!(
        matches!(
            err,
            ChallengeMetaError::Parse(_) | ChallengeMetaError::EmptyCategory
        ),
        "missing category must be rejected: {err}"
    );
}

#[test]
fn challenge_empty_toml() {
    let result = ChallengeMeta::from_toml_str("");
    assert!(result.is_err());
}

#[test]
fn challenge_invalid_toml() {
    let result = ChallengeMeta::from_toml_str("not valid toml {{{");
    assert!(result.is_err());
}

#[test]
fn challenge_docker_parse_all_fields() {
    let toml = r#"
name = "test"
version = "1.0.0"
author = "test"
category = "Web"
description = "test"

[flag]
type = "static"
value = "flag{test}"

[docker]
port = 8080

[docker.recommended_resources]
cpu_millis = 500
memory_bytes = 268435456
pids_limit = 100
"#;
    let meta = ChallengeMeta::parse_and_validate(toml).unwrap();
    assert_eq!(meta.static_flag_value(), Some("flag{test}"));
    let docker = meta.docker.unwrap();
    assert_eq!(docker.port, 8080);
    let res = docker.recommended_resources.unwrap();
    assert_eq!(res.cpu_millis, 500);
    assert_eq!(res.memory_bytes, 268_435_456);
    assert_eq!(res.pids_limit, 100);
}

#[test]
fn challenge_static_flag_rules() {
    let ok = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
description = "d"

[flag]
type = "static"
value = "flag{x}"
"#;
    let meta = ChallengeMeta::parse_and_validate(ok).unwrap();
    assert_eq!(meta.static_flag_value(), Some("flag{x}"));
    assert_eq!(meta.normalize().unwrap().flag_type, "static");

    let missing = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
description = "d"

[flag]
type = "static"
"#;
    let err = ChallengeMeta::parse_and_validate(missing).unwrap_err();
    assert!(matches!(err, ChallengeMetaError::StaticFlagRequired));
}

#[test]
fn challenge_dynamic_rejects_value_field() {
    let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
description = "d"

[flag]
type = "dynamic"
value = "flag{x}"
"#;
    let err = ChallengeMeta::from_toml_str(toml).unwrap_err();
    assert!(matches!(
        err,
        ChallengeMetaError::UnknownField(_) | ChallengeMetaError::Parse(_)
    ));
}

#[test]
fn challenge_legacy_fields_rejected() {
    // top-level legacy fields
    for line in [
        "image_tag = \"x:v1\"",
        "env_var = \"FLAG\"",
        "schema_version = 1",
    ] {
        let toml = format!(
            r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
description = "d"
{line}

[flag]
type = "dynamic"
"#
        );
        let err = ChallengeMeta::from_toml_str(&toml).unwrap_err();
        assert!(
            matches!(
                err,
                ChallengeMetaError::UnknownField(_) | ChallengeMetaError::Parse(_)
            ),
            "legacy field must be rejected: {line}"
        );
    }

    // legacy env_var inside [flag]
    let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
description = "d"

[flag]
type = "dynamic"
env_var = "FLAG"
"#;
    assert!(ChallengeMeta::from_toml_str(toml).is_err());

    // legacy string port
    let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
description = "d"

[flag]
type = "dynamic"

[docker]
port = "80/tcp"
"#;
    assert!(ChallengeMeta::from_toml_str(toml).is_err());

    // port 0 rejected
    let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
description = "d"

[flag]
type = "dynamic"

[docker]
port = 0
"#;
    let err = ChallengeMeta::parse_and_validate(toml).unwrap_err();
    assert!(matches!(err, ChallengeMetaError::InvalidPort(0)));
}

#[test]
fn challenge_safe_name_rules() {
    assert_eq!(
        derive_safe_name("Easy Web 01").as_deref(),
        Some("easy-web-01")
    );
    assert_eq!(derive_safe_name("easy---web").as_deref(), Some("easy-web"));

    let non_ascii = r#"
name = "注入题目"
version = "1.0.0"
author = "a"
category = "web"
description = "d"

[flag]
type = "dynamic"
"#;
    let err = ChallengeMeta::parse_and_validate(non_ascii).unwrap_err();
    assert!(matches!(err, ChallengeMetaError::SafeNameRequired));

    let explicit_ok = r#"
name = "注入题目"
version = "1.0.0"
author = "a"
category = "web"
description = "d"
safe_name = "zhu-ru"

[flag]
type = "dynamic"
"#;
    let meta = ChallengeMeta::parse_and_validate(explicit_ok).unwrap();
    assert_eq!(meta.resolved_safe_name().unwrap(), "zhu-ru");

    let explicit_bad = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
description = "d"
safe_name = "Easy Web"

[flag]
type = "dynamic"
"#;
    let err = ChallengeMeta::parse_and_validate(explicit_bad).unwrap_err();
    assert!(matches!(err, ChallengeMetaError::InvalidSafeName(_)));
}

#[test]
fn challenge_version_rules() {
    let rc = r#"
name = "t"
version = "1.0.0-rc.1"
author = "a"
category = "web"
description = "d"

[flag]
type = "dynamic"
"#;
    ChallengeMeta::parse_and_validate(rc).unwrap();

    let build = r#"
name = "t"
version = "1.0.0+build.1"
author = "a"
category = "web"
description = "d"

[flag]
type = "dynamic"
"#;
    let err = ChallengeMeta::parse_and_validate(build).unwrap_err();
    assert!(matches!(err, ChallengeMetaError::VersionBuildMetadata(_)));

    let bad = r#"
name = "t"
version = "abc"
author = "a"
category = "web"
description = "d"

[flag]
type = "dynamic"
"#;
    let err = ChallengeMeta::parse_and_validate(bad).unwrap_err();
    assert!(matches!(err, ChallengeMetaError::InvalidVersion { .. }));
}

#[test]
fn challenge_attachment_rules() {
    let ok = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
description = "d"
attachment = "attachment/src.zip"

[flag]
type = "dynamic"
"#;
    assert_eq!(
        ChallengeMeta::parse_and_validate(ok)
            .unwrap()
            .attachment
            .as_deref(),
        Some("attachment/src.zip")
    );

    for bad in ["../x", "/x", "src/x"] {
        let toml = format!(
            r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
description = "d"
attachment = "{bad}"

[flag]
type = "dynamic"
"#
        );
        let err = ChallengeMeta::parse_and_validate(&toml).unwrap_err();
        assert!(
            matches!(err, ChallengeMetaError::InvalidAttachmentPath(_, _)),
            "attachment path must be rejected: {bad}"
        );
    }
}

#[test]
fn challenge_normalize_defaults() {
    let toml = r#"
name = "Easy Web 01"
version = "1.0.0"
author = "a"
category = "web"
description = "d"

[flag]
type = "dynamic"

[docker]
port = 80
"#;
    let norm = ChallengeMeta::parse_and_validate(toml)
        .unwrap()
        .normalize()
        .unwrap();
    assert_eq!(norm.safe_name, "easy-web-01");
    assert_eq!(norm.flag_type, "dynamic");
    assert_eq!(norm.container_port, Some(80));
    assert_eq!(norm.recommended_resources.cpu_millis, 500);
    assert_eq!(norm.recommended_resources.memory_bytes, 268_435_456);
    assert_eq!(norm.recommended_resources.pids_limit, 100);
    assert!(norm.attachment.is_none());

    // non-docker challenge still gets the default recommendations
    let no_docker = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
description = "d"

[flag]
type = "dynamic"
"#;
    let norm = ChallengeMeta::parse_and_validate(no_docker)
        .unwrap()
        .normalize()
        .unwrap();
    assert_eq!(norm.container_port, None);
    assert_eq!(norm.recommended_resources.cpu_millis, 500);
}

#[test]
fn challenge_artifact_image_ref() {
    assert_eq!(
        build_artifact_image_ref(
            ArtifactKind::Challenge,
            "registry.example",
            "easy-web",
            "1.0.0"
        ),
        "registry.example/challenges/easy-web:1.0.0"
    );
    assert_eq!(
        build_artifact_image_ref(
            ArtifactKind::GameBox,
            "registry.example",
            "easy-web",
            "1.0.0"
        ),
        "registry.example/gameboxes/easy-web:1.0.0"
    );
}

// ─── GameBoxMeta Tests (§106) ───────────────────────────────────────

#[test]
fn gamebox_parse_valid() {
    let content = std::fs::read_to_string(Path::new("tests/fixtures/gamebox/meta.toml")).unwrap();
    let meta = GameBoxMeta::parse_and_validate(&content).unwrap();
    assert_eq!(meta.name, "test-gamebox");
    assert_eq!(meta.version, "1.0.0");
    assert_eq!(meta.gamebox.username, "ctf");
    assert_eq!(meta.safe_name.as_deref(), Some("test-gamebox"));
    assert_eq!(meta.gamebox.healthchecks.len(), 1);
    let res = meta.gamebox.recommended_resources.as_ref().unwrap();
    assert_eq!(res.cpu_millis, 1000);
    assert_eq!(res.memory_bytes, 536_870_912);
    assert_eq!(res.pids_limit, 100);
}

#[test]
fn gamebox_parse_minimal_omitted_safe_name() {
    let content =
        std::fs::read_to_string(Path::new("tests/fixtures/gamebox/meta_minimal.toml")).unwrap();
    let meta = GameBoxMeta::parse_and_validate(&content).unwrap();
    assert!(meta.safe_name.is_none());
    assert_eq!(meta.resolved_safe_name().unwrap(), "test-gamebox-minimal");
    assert!(meta.gamebox.recommended_resources.is_none());
    assert!(meta.judge.is_none());
}

#[test]
fn gamebox_parse_with_healthchecks() {
    let content = std::fs::read_to_string(Path::new(
        "tests/fixtures/gamebox/meta_with_healthcheck.toml",
    ))
    .unwrap();
    let meta = GameBoxMeta::parse_and_validate(&content).unwrap();
    assert_eq!(meta.gamebox.healthchecks.len(), 2);
    match &meta.gamebox.healthchecks[0] {
        GameBoxHealthcheck::Http {
            port,
            path,
            expected_status,
        } => {
            assert_eq!(*port, 80);
            assert_eq!(path, "/");
            assert_eq!(*expected_status, 200);
        }
        _ => panic!("expected http"),
    }
    match &meta.gamebox.healthchecks[1] {
        GameBoxHealthcheck::Tcp { port } => assert_eq!(*port, 3306),
        _ => panic!("expected tcp"),
    }
}

#[test]
fn gamebox_parse_with_judge() {
    let content =
        std::fs::read_to_string(Path::new("tests/fixtures/gamebox/meta_with_judge.toml")).unwrap();
    let meta = GameBoxMeta::parse_and_validate(&content).unwrap();
    let judge = meta.judge.as_ref().unwrap();
    assert_eq!(judge.script, "judge/check.py");
    // expected_status defaulted on HTTP
    match &meta.gamebox.healthchecks[0] {
        GameBoxHealthcheck::Http {
            expected_status, ..
        } => assert_eq!(*expected_status, 200),
        _ => panic!("expected http"),
    }
}

#[test]
fn gamebox_explicit_valid_safe_name() {
    let toml = r#"
name = "Easy Web"
version = "1.0.0"
author = "a"
category = "web"
description = "d"
safe_name = "easy-web-01"

[gamebox]
username = "u"
"#;
    let meta = GameBoxMeta::parse_and_validate(toml).unwrap();
    assert_eq!(meta.resolved_safe_name().unwrap(), "easy-web-01");
}

#[test]
fn gamebox_invalid_safe_name() {
    let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
description = "d"
safe_name = "Easy Web"

[gamebox]
username = "u"
"#;
    let err = GameBoxMeta::parse_and_validate(toml).unwrap_err();
    assert!(matches!(err, GameBoxMetaError::InvalidSafeName(_)));
}

#[test]
fn gamebox_valid_version_and_prerelease() {
    for v in ["1.0.0", "1.2.3", "2.0.0-rc.1"] {
        let toml = format!(
            r#"
name = "t"
version = "{v}"
author = "a"
category = "web"
description = "d"
[gamebox]
username = "u"
"#
        );
        GameBoxMeta::parse_and_validate(&toml).unwrap();
    }
}

#[test]
fn gamebox_invalid_version() {
    let toml = r#"
name = "t"
version = "not-a-version"
author = "a"
category = "web"
description = "d"
[gamebox]
username = "u"
"#;
    let err = GameBoxMeta::parse_and_validate(toml).unwrap_err();
    assert!(matches!(err, GameBoxMetaError::InvalidVersion { .. }));
}

#[test]
fn gamebox_reject_build_metadata() {
    let toml = r#"
name = "t"
version = "1.0.0+build.1"
author = "a"
category = "web"
description = "d"
[gamebox]
username = "u"
"#;
    let err = GameBoxMeta::parse_and_validate(toml).unwrap_err();
    assert!(matches!(err, GameBoxMetaError::VersionBuildMetadata(_)));
}

#[test]
fn gamebox_http_invalid_path() {
    let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
description = "d"
[gamebox]
username = "u"
[[gamebox.healthchecks]]
type = "http"
port = 80
path = "no-slash"
"#;
    let err = GameBoxMeta::parse_and_validate(toml).unwrap_err();
    assert!(matches!(err, GameBoxMetaError::InvalidHealthcheckPath(_)));
}

#[test]
fn gamebox_http_invalid_port_zero() {
    let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
description = "d"
[gamebox]
username = "u"
[[gamebox.healthchecks]]
type = "http"
port = 0
path = "/"
"#;
    let err = GameBoxMeta::parse_and_validate(toml).unwrap_err();
    assert!(matches!(err, GameBoxMetaError::InvalidHealthcheckPort(0)));
}

#[test]
fn gamebox_tcp_rejects_http_only_fields() {
    let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
description = "d"
[gamebox]
username = "u"
[[gamebox.healthchecks]]
type = "tcp"
port = 3306
path = "/"
"#;
    assert!(GameBoxMeta::from_toml_str(toml).is_err());

    let toml2 = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
description = "d"
[gamebox]
username = "u"
[[gamebox.healthchecks]]
type = "tcp"
port = 3306
expected_status = 200
"#;
    assert!(GameBoxMeta::from_toml_str(toml2).is_err());
}

#[test]
fn gamebox_duplicate_healthchecks() {
    let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
description = "d"
[gamebox]
username = "u"
[[gamebox.healthchecks]]
type = "tcp"
port = 3306
[[gamebox.healthchecks]]
type = "tcp"
port = 3306
"#;
    let err = GameBoxMeta::parse_and_validate(toml).unwrap_err();
    assert!(matches!(err, GameBoxMetaError::DuplicateHealthcheck));
}

#[test]
fn gamebox_missing_judge_ok() {
    let content =
        std::fs::read_to_string(Path::new("tests/fixtures/gamebox/meta_minimal.toml")).unwrap();
    let meta = GameBoxMeta::parse_and_validate(&content).unwrap();
    assert!(meta.judge.is_none());
}

#[test]
fn gamebox_invalid_judge_path() {
    let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
description = "d"
[gamebox]
username = "u"
[judge]
script = "scripts/check.py"
"#;
    let err = GameBoxMeta::parse_and_validate(toml).unwrap_err();
    assert!(matches!(err, GameBoxMetaError::InvalidJudgePath(_, _)));
}

#[test]
fn gamebox_reject_legacy_image_tag() {
    let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
description = "d"
[gamebox]
username = "u"
image_tag = "test:v1"
"#;
    assert!(GameBoxMeta::from_toml_str(toml).is_err());
}

#[test]
fn gamebox_reject_legacy_scoring() {
    let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
description = "d"
[gamebox]
username = "u"
break_points = 100
fix_points = 50
down_points = 200
first_bonus = 20
"#;
    assert!(GameBoxMeta::from_toml_str(toml).is_err());
}

#[test]
fn gamebox_reject_schema_version() {
    let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
description = "d"
schema_version = 1
[gamebox]
username = "u"
"#;
    assert!(GameBoxMeta::from_toml_str(toml).is_err());
}

#[test]
fn gamebox_reject_services() {
    let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
description = "d"
[gamebox]
username = "u"
[[gamebox.services]]
port = 80
"#;
    assert!(GameBoxMeta::from_toml_str(toml).is_err());
}

#[test]
fn gamebox_reject_old_resources_key() {
    let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
description = "d"
[gamebox]
username = "u"
[gamebox.resources]
cpu_millis = 1000
"#;
    assert!(GameBoxMeta::from_toml_str(toml).is_err());
}

#[test]
fn gamebox_missing_gamebox_section() {
    let toml = r#"
name = "test"
version = "1.0.0"
author = "test"
category = "Web"
description = "test"
"#;
    let result = GameBoxMeta::from_toml_str(toml);
    assert!(result.is_err());
}

#[test]
fn gamebox_missing_username() {
    let toml = r#"
name = "test"
version = "1.0.0"
author = "test"
category = "Web"
description = "test"

[gamebox]
"#;
    let result = GameBoxMeta::from_toml_str(toml);
    assert!(result.is_err());
}

#[test]
fn gamebox_missing_version() {
    let toml = r#"
name = "test"
author = "test"
category = "Web"
description = "test"

[gamebox]
username = "ctf"
"#;
    let result = GameBoxMeta::from_toml_str(toml);
    assert!(result.is_err());
}

#[test]
fn gamebox_normalize_stability() {
    let a = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
description = "d"
[gamebox]
username = "u"
[[gamebox.healthchecks]]
type = "tcp"
port = 3306
[[gamebox.healthchecks]]
type = "http"
port = 80
path = "/"
"#;
    let b = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
description = "d"
[gamebox]
username = "u"
[[gamebox.healthchecks]]
type = "http"
port = 80
path = "/"
expected_status = 200
[[gamebox.healthchecks]]
type = "tcp"
port = 3306
"#;
    let na = GameBoxMeta::parse_and_validate(a)
        .unwrap()
        .normalize()
        .unwrap();
    let nb = GameBoxMeta::parse_and_validate(b)
        .unwrap()
        .normalize()
        .unwrap();
    assert_eq!(
        serde_json::to_string(&na).unwrap(),
        serde_json::to_string(&nb).unwrap()
    );
    assert!(matches!(
        &na.healthchecks[0],
        NormalizedHealthcheck::Http {
            port: 80,
            expected_status: 200,
            ..
        }
    ));
}

// ─── safe_name (§107) ───────────────────────────────────────────────

#[test]
fn safe_name_derive_cases() {
    assert_eq!(
        derive_safe_name("Easy Web 01").as_deref(),
        Some("easy-web-01")
    );
    assert_eq!(derive_safe_name("easy---web").as_deref(), Some("easy-web"));
    assert_eq!(derive_safe_name("  Hello  ").as_deref(), Some("hello"));
    assert_eq!(derive_safe_name("foo_bar").as_deref(), Some("foo-bar"));
    // Mixed ASCII + CJK → ASCII slug; pure non-ASCII → None (SAFE_NAME_REQUIRED).
    assert_eq!(derive_safe_name("SQL注入").as_deref(), Some("sql"));
    assert_eq!(derive_safe_name("注入题目"), None);
    assert_eq!(derive_safe_name("!!!"), None);
    assert_eq!(derive_safe_name(""), None);
}

#[test]
fn safe_name_validate() {
    assert!(validate_safe_name("easy-web-01").is_ok());
    assert!(validate_safe_name("a").is_ok());
    assert!(validate_safe_name("9x").is_ok());
    assert!(validate_safe_name("Easy").is_err());
    assert!(validate_safe_name("-bad").is_err());
    assert!(validate_safe_name("has space").is_err());
    assert!(validate_safe_name("").is_err());
}

#[test]
fn safe_name_non_ascii_only_requires_explicit() {
    let toml = r#"
name = "注入题目"
version = "1.0.0"
author = "a"
category = "web"
description = "d"
[gamebox]
username = "u"
"#;
    let err = GameBoxMeta::parse_and_validate(toml).unwrap_err();
    assert!(matches!(err, GameBoxMetaError::SafeNameRequired));
}

// ─── Image ref (§108) ───────────────────────────────────────────────

#[test]
fn image_ref_helper_cases() {
    assert_eq!(
        build_gamebox_image_ref("floatctf", "ttt1", "1.0.0"),
        "floatctf/gameboxes/ttt1:1.0.0"
    );
    assert_eq!(
        build_gamebox_image_ref("registry.example.com", "easy-web", "2.1.0"),
        "registry.example.com/gameboxes/easy-web:2.1.0"
    );
}

#[test]
fn split_and_pick_repo_digest() {
    assert_eq!(
        split_image_ref("registry.example.com:5000/foo/bar:1.0"),
        (
            "registry.example.com:5000/foo/bar".into(),
            Some("1.0".into())
        )
    );
    let digests = vec![
        "other@sha256:1".into(),
        "registry.example.com:5000/foo/bar@sha256:abc".into(),
    ];
    assert_eq!(
        pick_repo_digest(&digests, "registry.example.com:5000/foo/bar:1.0").as_deref(),
        Some("registry.example.com:5000/foo/bar@sha256:abc")
    );
}

// ─── Helpers unit ───────────────────────────────────────────────────

#[test]
fn version_helper() {
    assert!(validate_version("1.0.0").is_ok());
    assert!(validate_version("1.0.0-rc.1").is_ok());
    assert!(matches!(
        validate_version("1.0.0+meta"),
        Err(GameBoxMetaError::VersionBuildMetadata(_))
    ));
}

#[test]
fn judge_path_helper() {
    assert!(validate_judge_path("judge/check.py").is_ok());
    assert!(validate_judge_path("/abs").is_err());
    assert!(validate_judge_path("judge/../x").is_err());
    assert!(validate_judge_path("other/x.py").is_err());
}

// ─── ContainerFilter Tests ──────────────────────────────────────────

#[test]
fn container_filter_empty() {
    let f = fcmc::ContainerFilter::default();
    let map = f.to_bollard_filters();
    assert!(map.is_empty());
}

#[test]
fn container_filter_multiple_labels() {
    let f = fcmc::ContainerFilter::default()
        .with_label("awd.event_id", "abc")
        .with_label("awd.team_id", "def");
    let map = f.to_bollard_filters();
    let labels = map.get("label").unwrap();
    assert_eq!(labels.len(), 2);
    assert!(labels.contains(&"awd.event_id=abc".to_string()));
    assert!(labels.contains(&"awd.team_id=def".to_string()));
}

#[test]
fn container_filter_with_name() {
    let f = fcmc::ContainerFilter::default().with_label("key", "val");
    let map = f.to_bollard_filters();
    assert!(map.contains_key("label"));
}

// ─── NetworkSpec Tests ──────────────────────────────────────────────

#[test]
fn network_spec_fields() {
    let s = fcmc::NetworkSpec {
        name: "n1".into(),
        subnet_cidr: "10.0.0.0/16".into(),
        internal: true,
        bridge_name: Some("br-n1".into()),
        check_duplicate: true,
    };
    assert!(s.internal);
    assert_eq!(s.bridge_name.as_deref(), Some("br-n1"));
    assert_eq!(s.name, "n1");
}

// ─── AWD Labels Tests ───────────────────────────────────────────────

#[test]
fn awd_labels_content() {
    let labels = fcmc::awd_labels(
        uuid::Uuid::nil(),
        uuid::Uuid::nil(),
        uuid::Uuid::nil(),
        uuid::Uuid::nil(),
        0,
        "gamebox",
    );
    assert_eq!(
        labels.get("awd.event_id").unwrap(),
        "00000000-0000-0000-0000-000000000000"
    );
    assert_eq!(labels.get("awd.resource_kind").unwrap(), "gamebox");
    assert_eq!(labels.get("awd.runtime_generation").unwrap(), "0");
    assert_eq!(labels.len(), 6);
}
