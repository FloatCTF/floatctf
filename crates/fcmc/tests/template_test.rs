//! 元数据模板相关集成测试。

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
    assert!(
        !dir.join("src/flag.sh").exists(),
        "flag.sh must not be scaffolded"
    );
    assert!(dir.join("src/entrypoint.sh").exists());
    assert!(dir.join("src/index.php").exists());
    assert!(dir.join("attachment").exists());
    assert!(
        dir.join("attachment/note.txt").exists(),
        "template must scaffold a sample attachment file (mirrors examples/test_c)"
    );
}

#[test]
fn challenge_template_meta_is_v1_and_roundtrips() {
    let tmp = tempfile::TempDir::new().unwrap();
    let output = tmp.path().to_str().unwrap();

    template::generate_challenge_template("roundtrip-test", output).unwrap();

    let meta_path = tmp.path().join("roundtrip-test").join("meta.toml");
    let content = std::fs::read_to_string(&meta_path).unwrap();

    // v1 manifest markers
    assert!(content.contains("version = \"1.0.0\""));
    assert!(content.contains("[flag]"));
    assert!(content.contains("type = \"dynamic\""));
    assert!(content.contains("[docker]"));
    assert!(content.contains("port = 80"));
    // attachment is enabled by default and points to the scaffolded sample file
    assert!(
        content.contains("attachment = \"attachment/note.txt\""),
        "template meta.toml must enable attachment (mirrors examples/test_c)"
    );
    // no legacy fields
    assert!(!content.contains("image_tag"));
    assert!(!content.contains("env_var"));
    assert!(!content.contains("flag.sh"));

    // parses + validates as a v1 manifest and round-trips
    let meta = ChallengeMeta::parse_and_validate(&content).unwrap();
    assert_eq!(meta.name, "roundtrip-test");
    assert_eq!(meta.version, "1.0.0");
    assert!(matches!(meta.flag, fcmc::ChallengeFlagConfig::Dynamic));
    let docker = meta.docker.unwrap();
    assert_eq!(docker.port, 80);
    let res = docker.recommended_resources.unwrap();
    assert_eq!(res.cpu_millis, 500);
    assert_eq!(res.memory_bytes, 268_435_456);
    assert_eq!(res.pids_limit, 100);
}

/// 生成的 entrypoint 必须把 FLAG 写入 flag 文件，并在同一 shell 中
/// `unset` 后再 `exec`——否则应用进程可能经 getenv /
/// `/proc/<pid>/environ` 读到真实 flag。
///
/// 真实脚本写入 `/flag`（容器根路径）；普通非 root
/// 开发 shell 无法写入，故在临时目录副本上把目标改写为
/// `./flag`——仍覆盖 shell 作用域契约
/// （先写、再 unset、再 exec）。
#[cfg(unix)]
#[test]
fn entrypoint_script_flag_contract() {
    let tmp = tempfile::TempDir::new().unwrap();
    template::generate_challenge_template("envtest", tmp.path().to_str().unwrap()).unwrap();
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
    template::generate_challenge_template("markers", tmp.path().to_str().unwrap()).unwrap();
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
    assert!(dir.join("awdp/exploit.py").exists());
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
    assert!(meta.awdp.is_some());
    assert_eq!(
        meta.awdp.as_ref().unwrap().exploit_script,
        "awdp/exploit.py"
    );
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
