//! Metadata parsing and validation tests.

use fcmc::{ChallengeMeta, GameBoxMeta};
use std::path::Path;

// ─── ChallengeMeta Tests ────────────────────────────────────────────

#[test]
fn challenge_parse_valid() {
    let content = std::fs::read_to_string(Path::new("tests/fixtures/challenge/meta.toml")).unwrap();
    let meta = ChallengeMeta::from_toml_str(&content).unwrap();
    assert_eq!(meta.name, "test-challenge");
    assert_eq!(meta.author, "tester@example.com");
    assert_eq!(meta.category, "Web");
    assert_eq!(meta.flag.value, "flag{test-flag-12345}");
    assert_eq!(meta.flag.env_var, "FLAG");
    let docker = meta.docker.unwrap();
    assert_eq!(docker.image_tag, "test/challenge:v1");
    assert_eq!(docker.port, "80/tcp");
}

#[test]
fn challenge_parse_no_docker() {
    let content =
        std::fs::read_to_string(Path::new("tests/fixtures/challenge/meta_no_docker.toml")).unwrap();
    let meta = ChallengeMeta::from_toml_str(&content).unwrap();
    assert!(meta.docker.is_none());
    assert_eq!(meta.category, "Crypto");
}

#[test]
fn challenge_parse_with_attachment() {
    let content = std::fs::read_to_string(Path::new(
        "tests/fixtures/challenge/meta_with_attachment.toml",
    ))
    .unwrap();
    let meta = ChallengeMeta::from_toml_str(&content).unwrap();
    assert_eq!(meta.attachment.as_deref(), Some("attachment/src.zip"));
}

#[test]
fn challenge_missing_name() {
    let toml = r#"
author = "test"
category = "Web"
description = "test"

[flag]
value = "flag{test}"
env_var = "FLAG"
"#;
    let result = ChallengeMeta::from_toml_str(toml);
    assert!(result.is_err());
}

#[test]
fn challenge_missing_flag() {
    let toml = r#"
name = "test"
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
author = "test"
description = "test"

[flag]
value = "flag{test}"
env_var = "FLAG"
"#;
    let result = ChallengeMeta::from_toml_str(toml);
    assert!(result.is_err());
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
author = "test"
category = "Web"
description = "test"

[flag]
value = "flag{test}"
env_var = "FLAG"

[docker]
image_tag = "myimage:latest"
port = "8080/tcp"
is_nc = true
"#;
    let meta = ChallengeMeta::from_toml_str(toml).unwrap();
    let docker = meta.docker.unwrap();
    assert_eq!(docker.image_tag, "myimage:latest");
    assert_eq!(docker.port, "8080/tcp");
    assert_eq!(docker.is_nc, Some(true));
}

// ─── GameBoxMeta Tests ──────────────────────────────────────────────

#[test]
fn gamebox_parse_valid() {
    let content = std::fs::read_to_string(Path::new("tests/fixtures/gamebox/meta.toml")).unwrap();
    let meta = GameBoxMeta::from_toml_str(&content).unwrap();
    assert_eq!(meta.name, "test-gamebox");
    assert_eq!(meta.gamebox.username, "ctf");
    assert_eq!(meta.gamebox.image_tag, "test/gamebox:v1");
    assert_eq!(meta.gamebox.break_points, 100);
    assert_eq!(meta.gamebox.fix_points, 100);
    assert_eq!(meta.gamebox.down_points, 200);
    assert_eq!(meta.gamebox.first_bonus, 20);
}

#[test]
fn gamebox_parse_with_healthcheck() {
    let content = std::fs::read_to_string(Path::new(
        "tests/fixtures/gamebox/meta_with_healthcheck.toml",
    ))
    .unwrap();
    let meta = GameBoxMeta::from_toml_str(&content).unwrap();
    let hc = meta.gamebox.healthcheck.unwrap();
    assert_eq!(hc.test, vec!["CMD-SHELL", "pgrep sshd"]);
    assert_eq!(hc.interval_secs, 30);
    assert_eq!(hc.timeout_secs, 10);
    assert_eq!(hc.retries, 3);
    assert_eq!(hc.start_period_secs, 60);
}

#[test]
fn gamebox_parse_with_judge() {
    let content =
        std::fs::read_to_string(Path::new("tests/fixtures/gamebox/meta_with_judge.toml")).unwrap();
    let meta = GameBoxMeta::from_toml_str(&content).unwrap();
    let judge = meta.gamebox.judge.unwrap();
    assert_eq!(judge.script_name, "check.py");
    assert_eq!(judge.script_content, "print('ok')");
    assert_eq!(
        judge.args_json.as_deref(),
        Some(r#"{"target": "{target_ip}"}"#)
    );
    assert_eq!(judge.timeout_secs, Some(15));
    assert_eq!(judge.retry_interval_secs, Some(5));
}

#[test]
fn gamebox_default_resource_limits() {
    let content =
        std::fs::read_to_string(Path::new("tests/fixtures/gamebox/meta_minimal.toml")).unwrap();
    let meta = GameBoxMeta::from_toml_str(&content).unwrap();
    assert_eq!(meta.gamebox.resources.cpu_millis, 1000);
    assert_eq!(meta.gamebox.resources.memory_bytes, 536_870_912);
    assert_eq!(meta.gamebox.resources.pids_limit, 100);
}

#[test]
fn gamebox_default_score_values() {
    let content =
        std::fs::read_to_string(Path::new("tests/fixtures/gamebox/meta_minimal.toml")).unwrap();
    let meta = GameBoxMeta::from_toml_str(&content).unwrap();
    assert_eq!(meta.gamebox.break_points, 100);
    assert_eq!(meta.gamebox.fix_points, 100);
    assert_eq!(meta.gamebox.down_points, 100);
    assert_eq!(meta.gamebox.first_bonus, 20);
}

#[test]
fn gamebox_missing_gamebox_section() {
    let toml = r#"
name = "test"
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
author = "test"
category = "Web"
description = "test"

[gamebox]
image_tag = "test:v1"
"#;
    let result = GameBoxMeta::from_toml_str(toml);
    assert!(result.is_err());
}

#[test]
fn gamebox_missing_image_tag() {
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
fn gamebox_reject_fractional_score() {
    let toml = r#"
name = "test"
author = "test"
category = "Web"
description = "test"

[gamebox]
username = "ctf"
image_tag = "test:v1"
break_points = 100.5
"#;
    let result = GameBoxMeta::from_toml_str(toml);
    assert!(result.is_err(), "fractional scores must be rejected");
}

#[test]
fn gamebox_serialize_uses_new_field_names() {
    let config = fcmc::GameBoxConfig {
        username: "ctf".into(),
        image_tag: "test:v1".into(),
        break_points: 100,
        fix_points: 50,
        down_points: 200,
        first_bonus: 20,
        resources: fcmc::ResourceConfig::default(),
        healthcheck: None,
        judge: None,
    };
    let json = serde_json::to_string(&config).unwrap();
    assert!(json.contains("\"break_points\""));
    assert!(!json.contains("\"break_point\""));
    assert!(json.contains("\"first_bonus\""));
    assert!(!json.contains("\"first_bouns\""));
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
    // Note: ContainerFilter doesn't have a with_name method in current code,
    // but we test the label filter works
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
    assert_eq!(labels.len(), 5);
}
