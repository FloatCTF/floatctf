//! Template generation tests.
//!
//! All tests use temporary directories and clean up automatically.

use fcmc::metadata::template;
use fcmc::{ChallengeMeta, GameBoxMeta};

#[test]
fn challenge_template_generates_files() {
    let tmp = tempfile::TempDir::new().unwrap();
    let output = tmp.path().to_str().unwrap();

    template::generate_challenge_template("test-template", output).unwrap();

    let dir = tmp.path().join("test-template");
    assert!(dir.exists());
    assert!(dir.join("meta.toml").exists());
    assert!(dir.join("src").exists());
    assert!(dir.join("src/Dockerfile").exists());
    assert!(dir.join("src/flag").exists());
    assert!(dir.join("src/flag.sh").exists());
    assert!(dir.join("src/entrypoint.sh").exists());
    assert!(dir.join("src/index.php").exists());
    assert!(dir.join("attachment").exists());
}

#[test]
fn challenge_template_meta_is_parseable() {
    let tmp = tempfile::TempDir::new().unwrap();
    let output = tmp.path().to_str().unwrap();

    template::generate_challenge_template("roundtrip-test", output).unwrap();

    let meta_path = tmp.path().join("roundtrip-test").join("meta.toml");
    let content = std::fs::read_to_string(meta_path).unwrap();
    let meta = ChallengeMeta::from_toml_str(&content).unwrap();
    assert_eq!(meta.name, "roundtrip-test");
}

#[test]
fn challenge_template_output_dir_already_exists() {
    let tmp = tempfile::TempDir::new().unwrap();
    let output = tmp.path().to_str().unwrap();

    // Generate twice — second should succeed (create_dir_all is idempotent)
    template::generate_challenge_template("exists", output).unwrap();
    template::generate_challenge_template("exists", output).unwrap();

    assert!(tmp.path().join("exists/meta.toml").exists());
}

#[test]
fn gamebox_template_generates_files() {
    let tmp = tempfile::TempDir::new().unwrap();
    let output = tmp.path().to_str().unwrap();

    template::generate_gamebox_template("gb-template", output).unwrap();

    let dir = tmp.path().join("gb-template");
    assert!(dir.exists());
    assert!(dir.join("meta.toml").exists());
    assert!(dir.join("src").exists());
    assert!(dir.join("src/Dockerfile").exists());
    assert!(dir.join("src/index.php").exists());
    assert!(dir.join("judge/check.py").exists());
}

#[test]
fn gamebox_template_meta_is_parseable() {
    let tmp = tempfile::TempDir::new().unwrap();
    let output = tmp.path().to_str().unwrap();

    template::generate_gamebox_template("gb-roundtrip", output).unwrap();

    let meta_path = tmp.path().join("gb-roundtrip").join("meta.toml");
    let content = std::fs::read_to_string(meta_path).unwrap();
    let meta = GameBoxMeta::parse_and_validate(&content).unwrap();
    assert_eq!(meta.name, "gb-roundtrip");
    assert_eq!(meta.version, "1.0.0");
    assert_eq!(meta.gamebox.username, "floatctf");
    assert!(meta.judge.is_some());
    assert!(!meta.gamebox.healthchecks.is_empty());
    // No legacy fields
    let raw = std::fs::read_to_string(tmp.path().join("gb-roundtrip/meta.toml")).unwrap();
    assert!(!raw.contains("image_tag"));
    assert!(!raw.contains("break_points"));
}

#[test]
fn gamebox_basic_template_generates_files() {
    let tmp = tempfile::TempDir::new().unwrap();
    let output = tmp.path().to_str().unwrap();

    template::generate_gamebox_basic_template("gb-basic", output).unwrap();

    let dir = tmp.path().join("gb-basic");
    assert!(dir.exists());
    assert!(dir.join("meta.toml").exists());
    assert!(dir.join("src").exists());
    assert!(dir.join("src/Dockerfile").exists());
    assert!(dir.join("src/entrypoint.sh").exists());
    assert!(dir.join("judge/check.py").exists());
}

#[test]
fn gamebox_basic_template_meta_is_parseable() {
    let tmp = tempfile::TempDir::new().unwrap();
    let output = tmp.path().to_str().unwrap();

    template::generate_gamebox_basic_template("gb-basic-rt", output).unwrap();

    let meta_path = tmp.path().join("gb-basic-rt").join("meta.toml");
    let content = std::fs::read_to_string(meta_path).unwrap();
    let meta = GameBoxMeta::parse_and_validate(&content).unwrap();
    assert_eq!(meta.name, "awd-base");
    assert_eq!(meta.safe_name.as_deref(), Some("awd-base"));
}

#[test]
fn gamebox_template_output_dir_already_exists() {
    let tmp = tempfile::TempDir::new().unwrap();
    let output = tmp.path().to_str().unwrap();

    template::generate_gamebox_template("gb-exists", output).unwrap();
    template::generate_gamebox_template("gb-exists", output).unwrap();

    assert!(tmp.path().join("gb-exists/meta.toml").exists());
}

#[test]
fn challenge_template_dockerfile_content() {
    let tmp = tempfile::TempDir::new().unwrap();
    let output = tmp.path().to_str().unwrap();

    template::generate_challenge_template("content-test", output).unwrap();

    let dockerfile =
        std::fs::read_to_string(tmp.path().join("content-test/src/Dockerfile")).unwrap();
    assert!(dockerfile.contains("EXPOSE 80"));
    assert!(dockerfile.contains("ENTRYPOINT"));
}

#[test]
fn gamebox_template_dockerfile_content() {
    let tmp = tempfile::TempDir::new().unwrap();
    let output = tmp.path().to_str().unwrap();

    template::generate_gamebox_template("gb-content", output).unwrap();

    let dockerfile = std::fs::read_to_string(tmp.path().join("gb-content/src/Dockerfile")).unwrap();
    assert!(dockerfile.contains("FROM"));
}
