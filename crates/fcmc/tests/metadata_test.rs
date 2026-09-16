//! 元数据解析集成测试（fixture 驱动）。
//!
//! Content Contract 的权威对齐测试在 `tests/content_contract_parity.rs`；
//! 本文件覆盖 FCMC 自身的解析/校验/ normalize 行为与运行时结构类型。

use fcmc::{
    ArtifactKind, ChallengeFlagConfig, ChallengeMeta, ChallengeMetaError, Difficulty,
    GameBoxHealthcheck, GameBoxMeta, GameBoxMetaError, NormalizedHealthcheck, content_image_ref,
    derive_safe_name, pick_repo_digest, split_image_ref, validate_judge_path, validate_safe_name,
    validate_version,
};
use std::path::{Path, PathBuf};

fn fixture(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(relative)
}

fn read(relative: &str) -> String {
    std::fs::read_to_string(fixture(relative)).unwrap()
}

// ─── ChallengeMeta ──────────────────────────────────────────────────

#[test]
fn challenge_parse_valid() {
    let meta = ChallengeMeta::parse_and_validate(
        &read("challenges/test-challenge/meta.toml"),
        "test-challenge",
    )
    .unwrap();
    assert_eq!(meta.name, "test-challenge");
    assert_eq!(meta.version, "1.0.0");
    assert_eq!(meta.author, "tester@example.com");
    assert_eq!(meta.category, "Web");
    assert_eq!(meta.difficulty, Difficulty::Easy);
    assert_eq!(meta.tags, vec!["web".to_string()]);
    assert!(matches!(meta.flag, Some(ChallengeFlagConfig::Dynamic)));
    assert_eq!(
        meta.resolved_safe_name("test-challenge").unwrap(),
        "test-challenge"
    );
    let docker = meta.docker.unwrap();
    assert_eq!(docker.port, Some(80));
    assert!(docker.recommended_resources.is_none());
}

#[test]
fn challenge_parse_no_docker() {
    let meta = ChallengeMeta::parse_and_validate(
        &read("challenges/test-challenge-no-docker/meta.toml"),
        "test-challenge-no-docker",
    )
    .unwrap();
    assert!(meta.docker.is_none());
    assert_eq!(meta.category, "Crypto");
    assert_eq!(meta.difficulty, Difficulty::Unknown);
    assert!(meta.tags.is_empty());
}

#[test]
fn challenge_parse_with_attachment() {
    let meta = ChallengeMeta::parse_and_validate(
        &read("challenges/test-challenge-attachment/meta.toml"),
        "test-challenge-attachment",
    )
    .unwrap();
    assert_eq!(meta.attachment.as_deref(), Some("attachment/src.zip"));
}

#[test]
fn challenge_missing_required_common_fields() {
    for toml in [
        r#"
version = "1.0.0"
author = "test"
category = "Web"
difficulty = "easy"
tags = []
description = "test"
"#,
        r#"
name = "test"
version = "1.0.0"
author = "test"
category = "Web"
tags = []
description = "test"
"#,
        r#"
name = "test"
version = "1.0.0"
author = "test"
category = "Web"
difficulty = "easy"
description = "test"
"#,
    ] {
        assert!(
            ChallengeMeta::from_toml_str(toml).is_err(),
            "missing required field must be rejected:\n{toml}"
        );
    }
}

#[test]
fn challenge_empty_toml_and_invalid_toml() {
    assert!(ChallengeMeta::from_toml_str("").is_err());
    assert!(ChallengeMeta::from_toml_str("not valid toml {{{").is_err());
}

#[test]
fn challenge_docker_parse_all_fields() {
    let toml = r#"
name = "test"
version = "1.0.0"
author = "test"
category = "Web"
difficulty = "easy"
tags = []
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
    let meta = ChallengeMeta::parse_and_validate(toml, "test").unwrap();
    assert_eq!(meta.static_flag_value(), Some("flag{test}"));
    let docker = meta.docker.unwrap();
    assert_eq!(docker.port, Some(8080));
    let res = docker.recommended_resources.unwrap();
    assert_eq!(res.cpu_millis, Some(500));
    assert_eq!(res.memory_bytes, Some(268_435_456));
    assert_eq!(res.pids_limit, Some(100));
}

#[test]
fn challenge_static_flag_rules() {
    let ok = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
difficulty = "easy"
tags = []
description = "d"

[flag]
type = "static"
value = "flag{x}"
"#;
    let meta = ChallengeMeta::parse_and_validate(ok, "t").unwrap();
    assert_eq!(meta.static_flag_value(), Some("flag{x}"));
    assert_eq!(
        meta.normalize("t").unwrap().flag_type.as_deref(),
        Some("static")
    );

    let missing = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
difficulty = "easy"
tags = []
description = "d"

[flag]
type = "static"
"#;
    let err = ChallengeMeta::parse_and_validate(missing, "t").unwrap_err();
    assert!(matches!(err, ChallengeMetaError::StaticFlagRequired));
}

#[test]
fn challenge_missing_flag_is_valid_and_injects_nothing() {
    let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
difficulty = "easy"
tags = []
description = "d"
"#;
    let meta = ChallengeMeta::parse_and_validate(toml, "t").unwrap();
    assert!(meta.flag.is_none());
    assert!(meta.flag_type().is_none());
    assert!(meta.static_flag_value().is_none());
    assert!(meta.normalize("t").unwrap().flag_type.is_none());
}

#[test]
fn challenge_dynamic_rejects_value_field() {
    let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
difficulty = "easy"
tags = []
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
fn challenge_flag_section_is_strict_but_top_level_is_not() {
    // FCMC 拥有的 [flag] 段严格
    let legacy_flag = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
difficulty = "easy"
tags = []
description = "d"

[flag]
type = "dynamic"
env_var = "FLAG"
"#;
    assert!(ChallengeMeta::from_toml_str(legacy_flag).is_err());

    // 顶层无关扩展字段被忽略（floatctf-content 亦然）
    let extension = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
difficulty = "easy"
tags = []
description = "d"
points = 100
image_tag = "legacy-but-ignored"
"#;
    ChallengeMeta::parse_and_validate(extension, "t").unwrap();

    // 字符串端口 / 0 端口仍然非法
    let string_port = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
difficulty = "easy"
tags = []
description = "d"

[docker]
port = "80/tcp"
"#;
    assert!(ChallengeMeta::from_toml_str(string_port).is_err());

    let zero_port = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
difficulty = "easy"
tags = []
description = "d"

[docker]
port = 0
"#;
    let err = ChallengeMeta::parse_and_validate(zero_port, "t").unwrap_err();
    assert!(matches!(err, ChallengeMetaError::InvalidPort(0)));
}

#[test]
fn challenge_safe_name_rules() {
    assert_eq!(
        derive_safe_name("Easy Web 01").as_deref(),
        Some("easy-web-01")
    );

    // content id 无法派生 → SafeNameRequired
    let non_ascii = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
difficulty = "easy"
tags = []
description = "d"
"#;
    let err = ChallengeMeta::parse_and_validate(non_ascii, "注入题目").unwrap_err();
    assert!(matches!(err, ChallengeMetaError::SafeNameRequired));

    let explicit_ok = r#"
name = "注入题目"
version = "1.0.0"
author = "a"
category = "web"
difficulty = "easy"
tags = []
description = "d"
safe_name = "zhu-ru"
"#;
    let meta = ChallengeMeta::parse_and_validate(explicit_ok, "题目").unwrap();
    assert_eq!(meta.resolved_safe_name("题目").unwrap(), "zhu-ru");

    let explicit_bad = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
difficulty = "easy"
tags = []
description = "d"
safe_name = "Easy Web"
"#;
    let err = ChallengeMeta::parse_and_validate(explicit_bad, "t").unwrap_err();
    assert!(matches!(err, ChallengeMetaError::InvalidSafeName(_)));
}

#[test]
fn challenge_version_rules() {
    for v in ["1.0.0", "12.34.56", "01.0.0"] {
        let toml = format!(
            r#"
name = "t"
version = "{v}"
author = "a"
category = "web"
difficulty = "easy"
tags = []
description = "d"
"#
        );
        ChallengeMeta::parse_and_validate(&toml, "t").unwrap();
    }

    for v in ["1.0.0-rc.1", "1.0.0+build.1", "abc", "1.0"] {
        let toml = format!(
            r#"
name = "t"
version = "{v}"
author = "a"
category = "web"
difficulty = "easy"
tags = []
description = "d"
"#
        );
        let err = ChallengeMeta::parse_and_validate(&toml, "t").unwrap_err();
        assert!(
            matches!(err, ChallengeMetaError::InvalidVersion(_)),
            "{v} must be rejected: {err}"
        );
    }
}

#[test]
fn challenge_attachment_rules() {
    let ok = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
difficulty = "easy"
tags = []
description = "d"
attachment = "attachment/src.zip"
"#;
    assert_eq!(
        ChallengeMeta::parse_and_validate(ok, "t")
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
difficulty = "easy"
tags = []
description = "d"
attachment = "{bad}"
"#
        );
        let err = ChallengeMeta::parse_and_validate(&toml, "t").unwrap_err();
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
difficulty = "easy"
tags = ["web"]
description = "d"

[flag]
type = "dynamic"

[docker]
port = 80
"#;
    let norm = ChallengeMeta::parse_and_validate(toml, "Easy Web 01")
        .unwrap()
        .normalize("Easy Web 01")
        .unwrap();
    assert_eq!(norm.safe_name, "easy-web-01");
    assert_eq!(norm.flag_type.as_deref(), Some("dynamic"));
    assert_eq!(norm.container_port, Some(80));
    assert_eq!(norm.recommended_resources.cpu_millis, 500);
    assert_eq!(norm.recommended_resources.memory_bytes, 268_435_456);
    assert_eq!(norm.recommended_resources.pids_limit, 100);
    assert!(norm.attachment.is_none());
    assert_eq!(norm.difficulty, Difficulty::Easy);
    assert_eq!(norm.tags, vec!["web".to_string()]);

    // 无 docker 的题目同样得到 Challenge 默认资源
    let no_docker = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
difficulty = "easy"
tags = []
description = "d"
"#;
    let norm = ChallengeMeta::parse_and_validate(no_docker, "t")
        .unwrap()
        .normalize("t")
        .unwrap();
    assert_eq!(norm.container_port, None);
    assert_eq!(norm.recommended_resources.cpu_millis, 500);
}

// ─── GameBoxMeta ────────────────────────────────────────────────────

#[test]
fn gamebox_parse_valid() {
    let meta =
        GameBoxMeta::parse_and_validate(&read("gameboxes/test-gamebox/meta.toml"), "test-gamebox")
            .unwrap();
    assert_eq!(meta.name, "test-gamebox");
    assert_eq!(meta.version, "1.0.0");
    assert_eq!(meta.safe_name.as_deref(), Some("test-gamebox"));
    assert_eq!(meta.difficulty, Difficulty::Easy);
    let gamebox = meta.gamebox.as_ref().unwrap();
    assert_eq!(gamebox.username, "ctf");
    assert_eq!(gamebox.healthchecks.len(), 1);
    let res = meta
        .docker
        .as_ref()
        .unwrap()
        .materialize_resources(fcmc::RecommendedResources::GAMEBOX_DEFAULTS);
    assert_eq!(res.cpu_millis, 1000);
    assert_eq!(res.memory_bytes, 536_870_912);
    assert_eq!(res.pids_limit, 100);
}

#[test]
fn gamebox_parse_minimal_omitted_safe_name() {
    let meta = GameBoxMeta::parse_and_validate(
        &read("gameboxes/test-gamebox-minimal/meta.toml"),
        "test-gamebox-minimal",
    )
    .unwrap();
    assert!(meta.safe_name.is_none());
    assert_eq!(
        meta.resolved_safe_name("test-gamebox-minimal").unwrap(),
        "test-gamebox-minimal"
    );
    assert!(meta.docker.is_none());
    assert!(meta.judge.is_none());
}

#[test]
fn gamebox_parse_with_healthchecks() {
    let meta = GameBoxMeta::parse_and_validate(
        &read("gameboxes/test-gamebox-hc/meta.toml"),
        "test-gamebox-hc",
    )
    .unwrap();
    let healthchecks = &meta.gamebox.as_ref().unwrap().healthchecks;
    assert_eq!(healthchecks.len(), 2);
    match &healthchecks[0] {
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
    match &healthchecks[1] {
        GameBoxHealthcheck::Tcp { port } => assert_eq!(*port, 3306),
        _ => panic!("expected tcp"),
    }
}

#[test]
fn gamebox_parse_with_judge() {
    let meta = GameBoxMeta::parse_and_validate(
        &read("gameboxes/test-gamebox-judge/meta.toml"),
        "test-gamebox-judge",
    )
    .unwrap();
    let judge = meta.judge.as_ref().unwrap();
    assert_eq!(judge.script, "judge/check.py");
    match &meta.gamebox.as_ref().unwrap().healthchecks[0] {
        GameBoxHealthcheck::Http {
            expected_status, ..
        } => assert_eq!(*expected_status, 200),
        _ => panic!("expected http"),
    }
}

#[test]
fn gamebox_parse_with_awdp() {
    let meta = GameBoxMeta::parse_and_validate(
        &read("gameboxes/test-gamebox-awdp/meta.toml"),
        "test-gamebox-awdp",
    )
    .unwrap();
    let awdp = meta.awdp.as_ref().unwrap();
    assert_eq!(awdp.exploit_script, "awdp/exploit.py");
    assert_eq!(awdp.source_code_dir.as_str(), "/var/www/html");
    let norm = meta.normalize("test-gamebox-awdp").unwrap();
    assert_eq!(norm.exploit_script.as_deref(), Some("awdp/exploit.py"));
    assert_eq!(norm.source_code_dir.as_deref(), Some("/var/www/html"));
    assert_eq!(norm.username.as_deref(), Some("ctf"));
}

#[test]
fn gamebox_reject_awdp_outside_dir() {
    let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
difficulty = "easy"
tags = []
description = "d"

[gamebox]
username = "u"

[awdp]
source_code_dir = "/var/www/html"
exploit_script = "scripts/x.py"
"#;
    let err = GameBoxMeta::parse_and_validate(toml, "t").unwrap_err();
    assert!(matches!(err, GameBoxMetaError::InvalidExploitPath(_, _)));
}

#[test]
fn gamebox_reject_awdp_missing_source_code_dir() {
    let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
difficulty = "easy"
tags = []
description = "d"

[gamebox]
username = "u"

[awdp]
exploit_script = "awdp/exploit.py"
"#;
    let err = GameBoxMeta::parse_and_validate(toml, "t").unwrap_err();
    assert!(matches!(err, GameBoxMetaError::Parse(_)));
}

#[test]
fn gamebox_reject_bad_source_code_dir() {
    let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
difficulty = "easy"
tags = []
description = "d"

[gamebox]
username = "u"

[awdp]
exploit_script = "awdp/exploit.py"
source_code_dir = "var/www/html"
"#;
    let err = GameBoxMeta::parse_and_validate(toml, "t").unwrap_err();
    assert!(matches!(err, GameBoxMetaError::InvalidSourceCodeDir(_, _)));
}

#[test]
fn gamebox_explicit_valid_safe_name() {
    let toml = r#"
name = "Easy Web"
version = "1.0.0"
author = "a"
category = "web"
difficulty = "easy"
tags = []
description = "d"
safe_name = "easy-web-01"

[gamebox]
username = "u"
"#;
    let meta = GameBoxMeta::parse_and_validate(toml, "t").unwrap();
    assert_eq!(meta.resolved_safe_name("t").unwrap(), "easy-web-01");
}

#[test]
fn gamebox_invalid_safe_name() {
    let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
difficulty = "easy"
tags = []
description = "d"
safe_name = "Easy Web"

[gamebox]
username = "u"
"#;
    let err = GameBoxMeta::parse_and_validate(toml, "t").unwrap_err();
    assert!(matches!(err, GameBoxMetaError::InvalidSafeName(_)));
}

#[test]
fn gamebox_version_rules() {
    for v in ["1.0.0", "1.2.3", "01.0.0"] {
        let toml = format!(
            r#"
name = "t"
version = "{v}"
author = "a"
category = "web"
difficulty = "easy"
tags = []
description = "d"
"#
        );
        GameBoxMeta::parse_and_validate(&toml, "t").unwrap();
    }

    for v in ["2.0.0-rc.1", "1.0.0+build.1", "not-a-version"] {
        let toml = format!(
            r#"
name = "t"
version = "{v}"
author = "a"
category = "web"
difficulty = "easy"
tags = []
description = "d"
"#
        );
        let err = GameBoxMeta::parse_and_validate(&toml, "t").unwrap_err();
        assert!(matches!(err, GameBoxMetaError::InvalidVersion(_)));
    }
}

#[test]
fn gamebox_http_healthcheck_rules() {
    let bad_path = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
difficulty = "easy"
tags = []
description = "d"
[gamebox]
username = "u"
[[gamebox.healthchecks]]
type = "http"
port = 80
path = "no-slash"
"#;
    let err = GameBoxMeta::parse_and_validate(bad_path, "t").unwrap_err();
    assert!(matches!(err, GameBoxMetaError::InvalidHealthcheckPath(_)));

    let zero_port = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
difficulty = "easy"
tags = []
description = "d"
[gamebox]
username = "u"
[[gamebox.healthchecks]]
type = "http"
port = 0
path = "/"
"#;
    let err = GameBoxMeta::parse_and_validate(zero_port, "t").unwrap_err();
    assert!(matches!(err, GameBoxMetaError::InvalidHealthcheckPort(0)));
}

#[test]
fn gamebox_tcp_rejects_http_only_fields() {
    let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
difficulty = "easy"
tags = []
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
difficulty = "easy"
tags = []
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
difficulty = "easy"
tags = []
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
    let err = GameBoxMeta::parse_and_validate(toml, "t").unwrap_err();
    assert!(matches!(err, GameBoxMetaError::DuplicateHealthcheck));
}

#[test]
fn gamebox_missing_judge_ok() {
    let meta = GameBoxMeta::parse_and_validate(
        &read("gameboxes/test-gamebox-minimal/meta.toml"),
        "test-gamebox-minimal",
    )
    .unwrap();
    assert!(meta.judge.is_none());
}

#[test]
fn gamebox_invalid_judge_path() {
    let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
difficulty = "easy"
tags = []
description = "d"
[gamebox]
username = "u"
[judge]
script = "scripts/check.py"
"#;
    let err = GameBoxMeta::parse_and_validate(toml, "t").unwrap_err();
    assert!(matches!(err, GameBoxMetaError::InvalidJudgePath(_, _)));
}

#[test]
fn gamebox_fcmc_owned_sections_stay_strict() {
    // [gamebox] 内 legacy 字段
    for extra in [
        "break_points = 100\nfix_points = 50\ndown_points = 200\nfirst_bonus = 20",
        "image_tag = \"test:v1\"",
    ] {
        let toml = format!(
            r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
difficulty = "easy"
tags = []
description = "d"

[gamebox]
username = "u"
{extra}
"#
        );
        assert!(
            GameBoxMeta::from_toml_str(&toml).is_err(),
            "[gamebox] must reject: {extra}"
        );
    }

    // [gamebox.services] / 旧 resources 键
    for block in [
        "[[gamebox.services]]\nport = 80\n",
        "[gamebox.resources]\ncpu_millis = 1000\n",
        "[gamebox.recommended_resources]\ncpu_millis = 1000\n",
    ] {
        let toml = format!(
            r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
difficulty = "easy"
tags = []
description = "d"

[gamebox]
username = "u"
{block}
"#
        );
        assert!(
            GameBoxMeta::from_toml_str(&toml).is_err(),
            "must reject block: {block}"
        );
    }

    // 顶层 schema_version 现在被忽略（官方 validator 亦然）
    let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
difficulty = "easy"
tags = []
description = "d"
schema_version = 1
"#;
    GameBoxMeta::parse_and_validate(toml, "t").unwrap();
}

#[test]
fn gamebox_missing_username_when_section_present() {
    let toml = r#"
name = "test"
version = "1.0.0"
author = "test"
category = "Web"
difficulty = "easy"
tags = []
description = "test"

[gamebox]
"#;
    assert!(GameBoxMeta::from_toml_str(toml).is_err());
}

#[test]
fn gamebox_missing_version() {
    let toml = r#"
name = "test"
author = "test"
category = "Web"
difficulty = "easy"
tags = []
description = "test"

[gamebox]
username = "ctf"
"#;
    assert!(GameBoxMeta::from_toml_str(toml).is_err());
}

#[test]
fn gamebox_normalize_stability() {
    let a = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
difficulty = "easy"
tags = []
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
difficulty = "easy"
tags = []
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
    let na = GameBoxMeta::parse_and_validate(a, "t")
        .unwrap()
        .normalize("t")
        .unwrap();
    let nb = GameBoxMeta::parse_and_validate(b, "t")
        .unwrap()
        .normalize("t")
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

// ─── safe_name / version helpers ────────────────────────────────────

#[test]
fn safe_name_derive_cases() {
    assert_eq!(
        derive_safe_name("Easy Web 01").as_deref(),
        Some("easy-web-01")
    );
    assert_eq!(derive_safe_name("easy---web").as_deref(), Some("easy-web"));
    assert_eq!(derive_safe_name("  Hello  ").as_deref(), Some("hello"));
    assert_eq!(derive_safe_name("Foo_Bar").as_deref(), Some("foo_bar"));
    assert_eq!(derive_safe_name("foo__bar").as_deref(), Some("foo-bar"));
    // 非 ASCII 被丢弃；纯非 ASCII → None
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
    assert!(validate_safe_name("foo.bar").is_ok());
    assert!(validate_safe_name("android_reverse").is_ok());
    // 显式 safe_name 必须小写（"Android_reverse" 只能作为目录名被派生）
    assert!(validate_safe_name("Android_reverse").is_err());
    assert!(validate_safe_name("Easy").is_err());
    assert!(validate_safe_name("-bad").is_err());
    assert!(validate_safe_name("has space").is_err());
    assert!(validate_safe_name("foo..bar").is_err());
    assert!(validate_safe_name("").is_err());
}

#[test]
fn version_helper() {
    assert!(validate_version("1.0.0").is_ok());
    assert!(validate_version("01.0.0").is_ok());
    assert!(validate_version("1.0.0-rc.1").is_err());
    assert!(validate_version("1.0.0+meta").is_err());
    assert!(validate_version("1.0").is_err());
}

#[test]
fn judge_path_helper() {
    assert!(validate_judge_path("judge/check.py").is_ok());
    assert!(validate_judge_path("/abs").is_err());
    assert!(validate_judge_path("judge/../x").is_err());
    assert!(validate_judge_path("other/x.py").is_err());
}

#[test]
fn canonical_image_ref_helper_cases() {
    assert_eq!(
        content_image_ref(ArtifactKind::Challenge, "floatctf", "ttt1", "1.0.0"),
        "floatctf/ttt1:challenge-v1.0.0"
    );
    assert_eq!(
        content_image_ref(
            ArtifactKind::GameBox,
            "registry.example.com",
            "easy-web",
            "2.1.0"
        ),
        "registry.example.com/easy-web:gamebox-v2.1.0"
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

// ─── ContainerFilter / NetworkSpec / labels ─────────────────────────

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
