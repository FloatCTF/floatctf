//! 元数据模板相关集成测试。

use fcmc::metadata::template;
use fcmc::{ChallengeMeta, GameBoxMeta};

#[test]
fn challenge_template_generates_files() {
    let tmp = tempfile::TempDir::new().unwrap();
    let output = tmp.path().to_str().unwrap();

    template::generate_challenge_template("test-template", output, None, "test-template").unwrap();

    let dir = tmp.path().join("test-template");
    assert!(dir.exists());
    assert!(dir.join("meta.toml").exists());
    assert!(dir.join("src").exists());
    assert!(dir.join("src/Dockerfile").exists());
    assert!(dir.join("src/flag").exists());
    assert!(
        !dir.join("src/flag.sh").exists(),
        "flag.sh must not be scaffolded"
    );
    assert!(dir.join("src/entrypoint.sh").exists());
    assert!(dir.join("src/index.php").exists());
    assert!(dir.join("attachment").exists());
    assert!(
        dir.join("attachment/note.txt").exists(),
        "template must scaffold a sample attachment file (mirrors examples/test-c)"
    );
}

#[test]
fn challenge_template_meta_follows_content_contract_and_roundtrips() {
    let tmp = tempfile::TempDir::new().unwrap();
    let output = tmp.path().to_str().unwrap();

    template::generate_challenge_template("roundtrip-test", output, None, "roundtrip-test")
        .unwrap();

    let meta_path = tmp.path().join("roundtrip-test").join("meta.toml");
    let content = std::fs::read_to_string(&meta_path).unwrap();

    // 官方 Content Contract 标记
    assert!(content.contains("version = \"1.0.0\""));
    assert!(content.contains("difficulty = \"unknown\""));
    assert!(content.contains("tags = []"));
    assert!(content.contains("[flag]"));
    assert!(content.contains("type = \"dynamic\""));
    assert!(content.contains("[docker]"));
    assert!(content.contains("port = 80"));
    // safe_name 注释必须说明“由目录 ID 派生”，不是由 name 派生
    assert!(
        content.contains("目录 ID"),
        "safe_name comment must reference the content id:\n{content}"
    );
    assert!(
        !content.contains("由 name 派生"),
        "stale 'derive from name' comment must be gone"
    );
    assert!(content.contains("attachment = \"attachment/note.txt\""));
    // 没有 legacy 字段
    assert!(!content.contains("image_tag"));
    assert!(!content.contains("env_var"));
    assert!(!content.contains("flag.sh"));

    // 生成的包立刻通过 contract 校验（目录名 = content id）
    let meta = ChallengeMeta::parse_and_validate(&content, "roundtrip-test").unwrap();
    assert_eq!(meta.name, "roundtrip-test");
    assert_eq!(meta.version, "1.0.0");
    assert_eq!(meta.difficulty, fcmc::Difficulty::Unknown);
    assert!(matches!(
        meta.flag,
        Some(fcmc::ChallengeFlagConfig::Dynamic)
    ));
    assert_eq!(
        meta.resolved_safe_name("roundtrip-test").unwrap(),
        "roundtrip-test"
    );
    let docker = meta.docker.unwrap();
    assert_eq!(docker.port, Some(80));
}

#[test]
fn challenge_template_with_explicit_safe_name_writes_it() {
    let tmp = tempfile::TempDir::new().unwrap();
    template::generate_challenge_template(
        "题目",
        tmp.path().to_str().unwrap(),
        Some("challenge-001"),
        "challenge-001",
    )
    .unwrap();

    let content = std::fs::read_to_string(tmp.path().join("题目/meta.toml")).unwrap();
    assert!(content.contains("safe_name = \"challenge-001\""));
    let meta = ChallengeMeta::parse_and_validate(&content, "题目").unwrap();
    assert_eq!(meta.resolved_safe_name("题目").unwrap(), "challenge-001");
}

/// 生成的 entrypoint 必须把 FLAG 写入 flag 文件，并在同一 shell 中
/// `unset` 后再 `exec`——否则应用进程可能经 getenv /
/// `/proc/<pid>/environ` 读到真实 flag。
#[cfg(unix)]
#[test]
fn entrypoint_script_flag_contract() {
    let tmp = tempfile::TempDir::new().unwrap();
    template::generate_challenge_template("envtest", tmp.path().to_str().unwrap(), None, "envtest")
        .unwrap();
    let src = tmp.path().join("envtest/src");

    let script = std::fs::read_to_string(src.join("entrypoint.sh")).unwrap();
    let scoped = script.replace("> /flag", "> ./flag");
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::write(dir.path().join("entrypoint.sh"), scoped).unwrap();
    std::fs::write(dir.path().join("flag"), "flag{dynamic_placeholder}\n").unwrap();

    let out = std::process::Command::new("sh")
        .arg("entrypoint.sh")
        .arg("env")
        .current_dir(dir.path())
        .env("FLAG", "flag{secret-secret}")
        .output()
        .expect("sh must be available (linux dev)");

    assert!(out.status.success(), "entrypoint.sh must exit 0");
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        !stdout.contains("flag{secret-secret}"),
        "FLAG leaked into exec'd process env:\n{stdout}"
    );

    let flag = std::fs::read_to_string(dir.path().join("flag")).unwrap();
    assert_eq!(
        flag, "flag{secret-secret}\n",
        "flag write must have happened in the same shell before exec"
    );
}

/// 生成脚本中的字面契约标记：写入 `/flag`，然后
/// 依次 `unset FLAG`，再 `exec "$@"`——同一 shell，无子 shell 辅助。
#[test]
fn entrypoint_script_contract_markers() {
    let tmp = tempfile::TempDir::new().unwrap();
    template::generate_challenge_template("markers", tmp.path().to_str().unwrap(), None, "markers")
        .unwrap();
    let script = std::fs::read_to_string(tmp.path().join("markers/src/entrypoint.sh")).unwrap();

    let write_pos = script.find("> /flag").expect("script must write to /flag");
    let unset_pos = script.find("unset FLAG").expect("script must unset FLAG");
    let exec_pos = script.find("exec \"$@\"").expect("script must exec $@");
    assert!(write_pos < unset_pos && unset_pos < exec_pos);
    assert!(!script.contains("flag.sh"), "no legacy flag.sh helper");
}

#[test]
fn challenge_template_output_dir_already_exists() {
    let tmp = tempfile::TempDir::new().unwrap();
    let output = tmp.path().to_str().unwrap();

    // Generate twice — second should succeed (create_dir_all is idempotent)
    template::generate_challenge_template("exists", output, None, "exists").unwrap();
    template::generate_challenge_template("exists", output, None, "exists").unwrap();

    assert!(tmp.path().join("exists/meta.toml").exists());
}

#[test]
fn gamebox_template_generates_files() {
    let tmp = tempfile::TempDir::new().unwrap();
    let output = tmp.path().to_str().unwrap();

    template::generate_gamebox_template("gb-template", output, None, "gb-template").unwrap();

    let dir = tmp.path().join("gb-template");
    assert!(dir.exists());
    assert!(dir.join("meta.toml").exists());
    assert!(dir.join("src").exists());
    assert!(dir.join("src/Dockerfile").exists());
    assert!(dir.join("src/index.php").exists());
    // AWDP 运行时契约：/flag.php 按 FLAG env 返回 flag（平台 Judge/Break 读取）
    assert!(
        dir.join("src/flag.php").exists(),
        "gamebox template must scaffold src/flag.php"
    );
    assert!(dir.join("judge/check.py").exists());
    assert!(dir.join("awdp/exploit.py").exists());
}

#[test]
fn gamebox_template_meta_is_parseable() {
    let tmp = tempfile::TempDir::new().unwrap();
    let output = tmp.path().to_str().unwrap();

    template::generate_gamebox_template("gb-roundtrip", output, None, "gb-roundtrip").unwrap();

    let raw = std::fs::read_to_string(tmp.path().join("gb-roundtrip/meta.toml")).unwrap();
    let meta = GameBoxMeta::parse_and_validate(&raw, "gb-roundtrip").unwrap();
    assert_eq!(meta.name, "gb-roundtrip");
    assert_eq!(meta.version, "1.0.3");
    assert_eq!(meta.difficulty, fcmc::Difficulty::Unknown);
    let gamebox = meta.gamebox.as_ref().unwrap();
    assert_eq!(gamebox.username, "floatctf");
    assert!(!gamebox.healthchecks.is_empty());
    assert!(meta.judge.is_some());
    assert!(meta.awdp.is_some());
    assert_eq!(
        meta.awdp.as_ref().unwrap().exploit_script,
        "awdp/exploit.py"
    );
    assert_eq!(
        meta.awdp.as_ref().unwrap().source_code_dir.as_str(),
        "/var/www/html"
    );

    // 资源来自 [docker.recommended_resources]；不再有 [gamebox.recommended_resources]
    assert!(raw.contains("[docker.recommended_resources]"));
    assert!(
        !raw.contains("[gamebox.recommended_resources]"),
        "gamebox resources must live under [docker]"
    );
    let res = meta
        .docker
        .as_ref()
        .unwrap()
        .materialize_resources(fcmc::RecommendedResources::GAMEBOX_DEFAULTS);
    assert_eq!(res.cpu_millis, 1000);
    assert_eq!(res.memory_bytes, 536_870_912);
    assert_eq!(res.pids_limit, 100);
    assert_eq!(meta.docker.as_ref().unwrap().port, Some(80));

    // safe_name 注释说明由目录 ID 派生
    assert!(raw.contains("目录 ID"));

    // 没有 legacy 字段
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

    let raw = std::fs::read_to_string(tmp.path().join("gb-basic-rt/meta.toml")).unwrap();
    let meta = GameBoxMeta::parse_and_validate(&raw, "gb-basic-rt").unwrap();
    assert_eq!(meta.name, "awd-base");
    assert_eq!(meta.safe_name.as_deref(), Some("awd-base"));
    assert!(raw.contains("[docker.recommended_resources]"));
    assert!(!raw.contains("[gamebox.recommended_resources]"));
}

#[test]
fn gamebox_template_output_dir_already_exists() {
    let tmp = tempfile::TempDir::new().unwrap();
    let output = tmp.path().to_str().unwrap();

    template::generate_gamebox_template("gb-exists", output, None, "gb-exists").unwrap();
    template::generate_gamebox_template("gb-exists", output, None, "gb-exists").unwrap();

    assert!(tmp.path().join("gb-exists/meta.toml").exists());
}

#[test]
fn challenge_template_dockerfile_content() {
    let tmp = tempfile::TempDir::new().unwrap();
    let output = tmp.path().to_str().unwrap();

    template::generate_challenge_template("content-test", output, None, "content-test").unwrap();

    let dockerfile =
        std::fs::read_to_string(tmp.path().join("content-test/src/Dockerfile")).unwrap();
    assert!(dockerfile.contains("EXPOSE 80"));
    assert!(dockerfile.contains("ENTRYPOINT"));
}

#[test]
fn gamebox_template_dockerfile_content() {
    let tmp = tempfile::TempDir::new().unwrap();
    let output = tmp.path().to_str().unwrap();

    template::generate_gamebox_template("gb-content", output, None, "gb-content").unwrap();

    let dockerfile = std::fs::read_to_string(tmp.path().join("gb-content/src/Dockerfile")).unwrap();
    assert!(dockerfile.contains("FROM"));
}
