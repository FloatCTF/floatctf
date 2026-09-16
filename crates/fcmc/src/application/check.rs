//! 包/环境检查用例。
//!
//! 检查分两层（**错误信息必须区分**）：
//!
//! * **A. FloatCTF Content Contract**（`floatctf-content/scripts/content.py`）：
//!   `name` / `version` / `author` / `category` / `difficulty` / `tags` /
//!   `description` / `safe_name` / `[docker]`。违反 = metadata 不合法。
//! * **B. FCMC operational checks**：附件文件是否存在、`src/Dockerfile`
//!   是否存在（container vs static）、judge / awdp 脚本是否存在、
//!   `[gamebox]` 运行时扩展是否齐备。违反 = **该操作无法执行**，
//!   而不是“metadata 不合法”。

use std::path::Path;

use anyhow::{Context, Result};
use colored::Colorize;

use crate::application::package::{has_dockerfile, resolve_content_id};
use crate::metadata::{
    ArtifactKind, CONTENT_IMAGE_NAMESPACE, ChallengeFlagConfig, ChallengeMeta, GameBoxMeta,
    RecommendedResources, content_image_ref,
};

/// a configuration check的结果。
#[derive(Debug)]
pub struct CheckResult {
    pub passed: bool,
    pub messages: Vec<CheckMessage>,
}

#[derive(Debug)]
pub struct CheckMessage {
    pub level: CheckLevel,
    pub section: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy)]
pub enum CheckLevel {
    Ok,
    Warn,
    Err,
}

impl std::fmt::Display for CheckLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CheckLevel::Ok => write!(f, "{}", "OK".green()),
            CheckLevel::Warn => write!(f, "{}", "WARN".yellow()),
            CheckLevel::Err => write!(f, "{}", "ERR".red()),
        }
    }
}

fn push(messages: &mut Vec<CheckMessage>, level: CheckLevel, section: &str, message: String) {
    messages.push(CheckMessage {
        level,
        section: section.into(),
        message,
    });
}

/// 检查 Challenge 包目录。
///
/// **A. Content Contract**：`meta.toml` 解析 + 官方公共字段校验
///（`name/version/author/category/difficulty/tags/description`、`safe_name`
/// 由目录名派生、`[flag]` / `[docker]`）。
///
/// **B. FCMC operational**：附件文件是否真实存在、`src/Dockerfile` 是否存在
///（container vs static）、动态 flag 是否有可注入的容器。
pub fn check_challenge(dir: &Path) -> Result<CheckResult> {
    let meta_path = dir.join("meta.toml");
    let mut messages = Vec::new();
    let mut passed = true;

    push(
        &mut messages,
        CheckLevel::Ok,
        "配置文件",
        format!("配置文件: {:?}", meta_path),
    );

    let content_id = resolve_content_id(dir)?;
    let content = std::fs::read_to_string(&meta_path).context("Failed to read meta.toml")?;

    let cfg = match ChallengeMeta::parse_and_validate(&content, &content_id) {
        Ok(cfg) => {
            push(
                &mut messages,
                CheckLevel::Ok,
                "Content Contract",
                format!("metadata 合法（content id = {content_id}）"),
            );
            cfg
        }
        Err(e) => {
            push(
                &mut messages,
                CheckLevel::Err,
                "Content Contract",
                format!("metadata contract invalid: {e}"),
            );
            return Ok(CheckResult {
                passed: false,
                messages,
            });
        }
    };

    // safe_name（由 content id 派生）
    match cfg.resolved_safe_name(&content_id) {
        Ok(s) => push(
            &mut messages,
            CheckLevel::Ok,
            "safe_name",
            format!("safe_name = {s}"),
        ),
        Err(e) => {
            push(&mut messages, CheckLevel::Err, "safe_name", e.to_string());
            passed = false;
        }
    }

    push(
        &mut messages,
        CheckLevel::Ok,
        "difficulty",
        format!("difficulty = {}, tags = {:?}", cfg.difficulty, cfg.tags),
    );

    // flag：官方 Content Contract 不要求 [flag]
    match &cfg.flag {
        Some(ChallengeFlagConfig::Dynamic) => push(
            &mut messages,
            CheckLevel::Ok,
            "flag",
            "type = dynamic".into(),
        ),
        Some(ChallengeFlagConfig::Static { .. }) => push(
            &mut messages,
            CheckLevel::Ok,
            "flag",
            "type = static".into(),
        ),
        None => push(
            &mut messages,
            CheckLevel::Warn,
            "flag",
            "未声明 [flag]：metadata 合法，运行时不会注入 FLAG".into(),
        ),
    }

    // B. 容器判定只看 src/Dockerfile
    let container = has_dockerfile(dir);
    if container {
        push(
            &mut messages,
            CheckLevel::Ok,
            "Dockerfile",
            format!(
                "container content: {:?}",
                dir.join("src").join("Dockerfile")
            ),
        );
    } else {
        push(
            &mut messages,
            CheckLevel::Warn,
            "Dockerfile",
            "content is static: src/Dockerfile not found（不可 build；[docker] 会被忽略）".into(),
        );
    }

    // B. 附件文件真实存在
    push(&mut messages, CheckLevel::Ok, "附件检查", String::new());
    if let Some(attachment) = &cfg.attachment {
        let attachment_path = dir.join(attachment);
        if attachment_path.exists() {
            let size = std::fs::metadata(&attachment_path)
                .map(|m| m.len())
                .unwrap_or(0);
            push(
                &mut messages,
                CheckLevel::Ok,
                "附件检查",
                format!("附件存在: {:?} ({} bytes)", attachment_path, size),
            );
        } else {
            push(
                &mut messages,
                CheckLevel::Err,
                "附件检查",
                format!("附件不存在: {:?}", attachment_path),
            );
            passed = false;
        }
    } else {
        push(
            &mut messages,
            CheckLevel::Warn,
            "附件检查",
            "未配置附件".into(),
        );
    }

    // Docker metadata（公共 [docker]）
    match &cfg.docker {
        Some(docker) => {
            push(
                &mut messages,
                CheckLevel::Ok,
                "Docker 检查",
                match docker.port {
                    Some(port) => format!("[docker] port = {port}"),
                    None => "[docker] 未声明 port（运行时不绑定端口）".into(),
                },
            );
            let res = docker.materialize_resources(RecommendedResources::CHALLENGE_DEFAULTS);
            push(
                &mut messages,
                CheckLevel::Ok,
                "资源配置",
                format!(
                    "cpu_millis={}, memory_bytes={}, pids_limit={}（缺省已物化）",
                    res.cpu_millis, res.memory_bytes, res.pids_limit
                ),
            );
        }
        None => push(
            &mut messages,
            CheckLevel::Warn,
            "Docker 检查",
            "未配置 [docker]（metadata 合法；无端口/资源建议）".into(),
        ),
    }

    // B. 动态 flag 需要容器承载
    if matches!(cfg.flag, Some(ChallengeFlagConfig::Dynamic)) && !container {
        push(
            &mut messages,
            CheckLevel::Warn,
            "运行时前置",
            "dynamic flag 需要容器承载，但缺少 src/Dockerfile：无法交付动态 flag".into(),
        );
    }

    Ok(CheckResult { passed, messages })
}

/// 检查 GameBox 包目录。
///
/// **A. Content Contract**：与 Challenge 完全相同的公共字段校验。
///
/// **B. FCMC operational**：`src/Dockerfile` 是否存在、`[gamebox]` 运行时扩展
///（username / healthchecks）、`[judge]` / `[awdp]` 脚本文件是否真实存在。
/// 缺少 `[gamebox]` 只影响运行时操作，**不会**被判为 metadata 不合法。
pub fn check_gamebox(dir: &Path) -> Result<CheckResult> {
    let meta_path = dir.join("meta.toml");
    let mut messages = Vec::new();
    let mut passed = true;

    push(
        &mut messages,
        CheckLevel::Ok,
        "配置文件",
        format!("配置文件: {:?}", meta_path),
    );

    let content_id = resolve_content_id(dir)?;
    let content = std::fs::read_to_string(&meta_path).context("Failed to read meta.toml")?;

    let cfg = match GameBoxMeta::parse_and_validate(&content, &content_id) {
        Ok(cfg) => {
            push(
                &mut messages,
                CheckLevel::Ok,
                "Content Contract",
                format!("metadata 合法（content id = {content_id}）"),
            );
            cfg
        }
        Err(e) => {
            push(
                &mut messages,
                CheckLevel::Err,
                "Content Contract",
                format!("metadata contract invalid: {e}"),
            );
            return Ok(CheckResult {
                passed: false,
                messages,
            });
        }
    };

    match cfg.resolved_safe_name(&content_id) {
        Ok(s) => push(
            &mut messages,
            CheckLevel::Ok,
            "safe_name",
            format!("safe_name = {s}"),
        ),
        Err(e) => {
            push(&mut messages, CheckLevel::Err, "safe_name", e.to_string());
            passed = false;
        }
    }

    push(
        &mut messages,
        CheckLevel::Ok,
        "version",
        format!("version = {}", cfg.version),
    );
    push(
        &mut messages,
        CheckLevel::Ok,
        "difficulty",
        format!("difficulty = {}, tags = {:?}", cfg.difficulty, cfg.tags),
    );

    // 资源配置唯一来源：[docker.recommended_resources]
    let res = cfg
        .docker
        .as_ref()
        .map(|d| d.materialize_resources(RecommendedResources::GAMEBOX_DEFAULTS))
        .unwrap_or(RecommendedResources::GAMEBOX_DEFAULTS);
    push(
        &mut messages,
        CheckLevel::Ok,
        "资源配置",
        format!(
            "cpu_millis={}, memory_bytes={}, pids_limit={}（来源 [docker.recommended_resources]，缺省已物化）",
            res.cpu_millis, res.memory_bytes, res.pids_limit
        ),
    );

    // B. [gamebox] 是运行时扩展，不是 Content Contract 必需字段
    match &cfg.gamebox {
        Some(gamebox) => {
            push(
                &mut messages,
                CheckLevel::Ok,
                "gamebox",
                format!(
                    "username = {}, {} 条 readiness 探针",
                    gamebox.username,
                    gamebox.healthchecks.len()
                ),
            );
        }
        None => push(
            &mut messages,
            CheckLevel::Warn,
            "gamebox",
            "未声明 [gamebox]：metadata 合法，但 check --runtime / AWD 部署需要它（username/healthchecks）"
                .into(),
        ),
    }

    // B. GameBox 必须有构建上下文
    let dockerfile = dir.join("src").join("Dockerfile");
    if dockerfile.is_file() {
        push(
            &mut messages,
            CheckLevel::Ok,
            "Dockerfile",
            format!("存在: {:?}", dockerfile),
        );
    } else {
        push(
            &mut messages,
            CheckLevel::Err,
            "Dockerfile",
            format!(
                "content is static: src/Dockerfile not found (expected {:?})",
                dockerfile
            ),
        );
        passed = false;
    }

    // B. judge 脚本文件
    if let Some(ref judge) = cfg.judge {
        let script_path = dir.join(&judge.script);
        if script_path.exists() {
            push(
                &mut messages,
                CheckLevel::Ok,
                "Judge",
                format!("脚本存在: {:?}", script_path),
            );
        } else {
            push(
                &mut messages,
                CheckLevel::Err,
                "Judge",
                format!("脚本不存在: {:?}", script_path),
            );
            passed = false;
        }
    } else {
        push(
            &mut messages,
            CheckLevel::Warn,
            "Judge",
            "未配置 [judge]".into(),
        );
    }

    // B. awdp exploit 脚本文件
    if let Some(ref awdp) = cfg.awdp {
        let script_path = dir.join(&awdp.exploit_script);
        if script_path.exists() {
            push(
                &mut messages,
                CheckLevel::Ok,
                "AWDP",
                format!("exploit 脚本存在: {:?}", script_path),
            );
        } else {
            push(
                &mut messages,
                CheckLevel::Err,
                "AWDP",
                format!("exploit 脚本不存在: {:?}", script_path),
            );
            passed = false;
        }
        push(
            &mut messages,
            CheckLevel::Ok,
            "AWDP",
            format!(
                "source_code_dir = {}（打包源码提供给选手）",
                awdp.source_code_dir
            ),
        );
    } else {
        push(
            &mut messages,
            CheckLevel::Warn,
            "AWDP",
            "未配置 [awdp]".into(),
        );
    }

    Ok(CheckResult { passed, messages })
}

/// 将检查结果打印到 stdout。
pub fn print_check_result(result: &CheckResult) {
    println!("\n================ 配置检查报告 ================\n");

    for msg in &result.messages {
        if !msg.message.is_empty() {
            println!("  {}   {}", msg.level, msg.message);
        }
    }

    println!("\n----------------------------------------------");
    if result.passed {
        println!("最终结果: {}", "通过".green());
    } else {
        println!("最终结果: {}", "失败".red());
    }
    println!("==============================================\n");
}

/// 运行时检查：创建测试容器、打印访问信息，并保持存活
/// 直到用户按 Enter（或 Ctrl+C），随后停止并删除。
///
/// 镜像引用使用 canonical ref
/// `floatctf/{safe_name}:challenge-v{version}`；
/// 端口绑定来自 `[docker].port`（无 port → 不绑定）；
/// 动态 flag 经 `FLAG` 环境变量注入，`[flag]` 缺失/static → 不注入。
pub async fn check_challenge_runtime(dir: &Path) -> Result<()> {
    use crate::runtime::{
        ContainerRuntime, ContainerSpec, DockerContainerRuntime, ImageRuntime, PortBinding,
        ResourceLimits, connect_preferred,
    };
    use tokio::io::{AsyncBufReadExt, BufReader};

    let content_id = resolve_content_id(dir)?;
    let meta_path = dir.join("meta.toml");
    let content = std::fs::read_to_string(&meta_path).context("Failed to read meta.toml")?;
    let cfg = ChallengeMeta::parse_and_validate(&content, &content_id)
        .context("Invalid challenge meta.toml")?;

    let safe_name = cfg
        .resolved_safe_name(&content_id)
        .context("Cannot resolve safe_name for runtime image ref")?;
    let image_ref = content_image_ref(
        ArtifactKind::Challenge,
        CONTENT_IMAGE_NAMESPACE,
        &safe_name,
        &cfg.version,
    );

    let (docker, _) = connect_preferred()
        .await
        .context("Failed to connect to Docker")?;

    let rt = DockerContainerRuntime::new(docker.clone());
    // 容器名必须合法（仅 [a-zA-Z0-9_.-]），故用 safe_name 而非显示名。
    let container_name = format!("fcmc_check_{safe_name}");

    // 本地缺镜像时自动拉取（检查前先把镜像就位）。
    rt.ensure_image(&image_ref, None).await?;

    // Dynamic flag: platform injects FLAG env (entrypoint writes it to /flag).
    // Static flag / no [flag]: no env injection at all.
    let env = match &cfg.flag {
        Some(ChallengeFlagConfig::Dynamic) => vec!["FLAG=flag{runtime-check}".to_string()],
        Some(ChallengeFlagConfig::Static { .. }) | None => Vec::new(),
    };

    // 官方 [docker].port 可选：没有就不绑定任何端口。
    let port_bindings: Vec<PortBinding> = cfg
        .docker
        .as_ref()
        .and_then(|d| d.port)
        .map(|port| {
            vec![PortBinding {
                container_port: format!("{port}/tcp"),
                host_ip: Some("0.0.0.0".into()),
                host_port: None,
            }]
        })
        .unwrap_or_default();

    let handle = rt
        .create_and_start(ContainerSpec {
            name: container_name.clone(),
            image: image_ref,
            env,
            labels: Default::default(),
            network_name: None,
            fixed_ip: None,
            network_aliases: vec![],
            port_bindings,
            auto_remove: true,
            resources: ResourceLimits::default(),
            network_mode: None,
            healthcheck: None,
        })
        .await?;

    // Wait briefly for container to start
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Verify container is running
    let state = rt.inspect_container(&handle.container_id).await?;
    if !state.running {
        anyhow::bail!("Container {} is not running", container_name);
    }

    // 打印访问信息，保持容器存活直到用户测试完成。
    println!(
        "\n  {} 容器已启动: {} ({})",
        "OK".green(),
        state.container_name,
        state.container_id
    );
    if state.published_ports.is_empty() {
        println!(
            "  {} 未发布任何端口（meta.toml 未声明 [docker].port）",
            "WARN".yellow()
        );
    } else {
        let mut ports: Vec<(&String, &u16)> = state.published_ports.iter().collect();
        ports.sort();
        for (container_port, host_port) in ports {
            println!(
                "  {} 访问地址: http://127.0.0.1:{}  (容器内 {})",
                "OK".green(),
                host_port,
                container_port
            );
        }
    }
    if matches!(cfg.flag, Some(ChallengeFlagConfig::Dynamic)) {
        println!(
            "  {} 动态 flag：容器内已注入 FLAG=flag{{runtime-check}}，入口脚本会写入 /flag",
            "提示".yellow()
        );
    }
    println!("\n  测试完成后按 {} 停止并删除容器 …\n", "Enter".bold());

    // 等待 Enter 或 Ctrl+C（两者都走同一清理路径）。
    let mut line = String::new();
    let mut stdin = BufReader::new(tokio::io::stdin());
    tokio::select! {
        _ = stdin.read_line(&mut line) => {}
        _ = tokio::signal::ctrl_c() => {
            println!("\n  收到 Ctrl+C，停止并删除容器 …");
        }
    }

    rt.stop_and_remove(&handle.container_id, std::time::Duration::from_secs(5))
        .await?;
    println!("  {} 容器已停止并删除", "OK".green());

    Ok(())
}

/// GameBox 运行时检查：启动测试容器并打印 SSH
/// 凭据（docker IP + 用户 + 口令）供用户连接测试。
/// 保持容器存活直到 Enter（或 Ctrl+C），随后停止并删除。
///
/// 镜像引用使用 canonical ref
/// `floatctf/{safe_name}:gamebox-v{version}`；SSH 凭据经
/// `GAMEBOX_USERNAME` / `GAMEBOX_USERPASS` 环境变量传入（与 examples/test-g
/// 的 entrypoint 契约一致）。
///
/// `[gamebox].username` 是 **FCMC 运行时扩展**：缺失时返回明确的 operational
/// 错误，而不是声称 metadata 不合法。
pub async fn check_gamebox_runtime(dir: &Path) -> Result<()> {
    use crate::runtime::{
        ContainerRuntime, ContainerSpec, DockerContainerRuntime, ImageRuntime, PortBinding,
        ResourceLimits, connect_preferred,
    };
    use tokio::io::{AsyncBufReadExt, BufReader};

    let content_id = resolve_content_id(dir)?;
    let meta_path = dir.join("meta.toml");
    let content = std::fs::read_to_string(&meta_path).context("Failed to read meta.toml")?;
    let cfg = GameBoxMeta::parse_and_validate(&content, &content_id)
        .context("Invalid gamebox meta.toml")?;

    let gamebox = cfg.require_gamebox_section().context(
        "GameBox runtime metadata [gamebox] is required for runtime check \
         (username is needed for GAMEBOX_USERNAME)",
    )?;

    let safe_name = cfg
        .resolved_safe_name(&content_id)
        .context("Cannot resolve safe_name for runtime image ref")?;
    let image_ref = content_image_ref(
        ArtifactKind::GameBox,
        CONTENT_IMAGE_NAMESPACE,
        &safe_name,
        &cfg.version,
    );

    let (docker, _) = connect_preferred()
        .await
        .context("Failed to connect to Docker")?;

    let rt = DockerContainerRuntime::new(docker.clone());
    let container_name = format!("fcmc_check_gb_{safe_name}");

    // 本地缺镜像时自动拉取。
    rt.ensure_image(&image_ref, None).await?;

    // 测试凭据：username 来自 meta.toml [gamebox]，密码随机生成并打印给用户。
    let username = gamebox.username.clone();
    let password = {
        let u = uuid::Uuid::new_v4().simple().to_string();
        format!("Fc{}", &u[..12])
    };

    let handle = rt
        .create_and_start(ContainerSpec {
            name: container_name.clone(),
            image: image_ref,
            env: vec![
                format!("GAMEBOX_USERNAME={username}"),
                format!("GAMEBOX_USERPASS={password}"),
            ],
            labels: Default::default(),
            network_name: None,
            fixed_ip: None,
            network_aliases: vec![],
            port_bindings: vec![
                PortBinding {
                    container_port: "80/tcp".into(),
                    host_ip: Some("0.0.0.0".into()),
                    host_port: None,
                },
                PortBinding {
                    container_port: "22/tcp".into(),
                    host_ip: Some("0.0.0.0".into()),
                    host_port: None,
                },
            ],
            auto_remove: true,
            resources: ResourceLimits::default(),
            network_mode: None,
            healthcheck: None,
        })
        .await?;

    // Wait briefly for sshd/apache to come up.
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    let state = rt.inspect_container(&handle.container_id).await?;
    if !state.running {
        anyhow::bail!("Container {} is not running", container_name);
    }

    // 输出 Docker IP + SSH 凭据，保持容器存活直到用户测试完成。
    let ip = state
        .ip_address
        .clone()
        .unwrap_or_else(|| "<unknown>".into());
    println!(
        "\n  {} 容器已启动: {} ({})",
        "OK".green(),
        state.container_name,
        state.container_id
    );
    println!("  {} Docker IP: {}", "OK".green(), ip);
    println!("  {} SSH 用户: {}", "OK".green(), username);
    println!("  {} SSH 密码: {}", "OK".green(), password);
    println!("  {} SSH 连接: ssh {}@{}", "OK".green(), username, ip);
    let mut ports: Vec<(&String, &u16)> = state.published_ports.iter().collect();
    ports.sort();
    for (container_port, host_port) in ports {
        println!(
            "  {} 端口映射: 127.0.0.1:{} -> 容器内 {}",
            "OK".green(),
            host_port,
            container_port
        );
    }
    println!("\n  测试完成后按 {} 停止并删除容器 …\n", "Enter".bold());

    // 等待 Enter 或 Ctrl+C（两者都走同一清理路径）。
    let mut line = String::new();
    let mut stdin = BufReader::new(tokio::io::stdin());
    tokio::select! {
        _ = stdin.read_line(&mut line) => {}
        _ = tokio::signal::ctrl_c() => {
            println!("\n  收到 Ctrl+C，停止并删除容器 …");
        }
    }

    rt.stop_and_remove(&handle.container_id, std::time::Duration::from_secs(5))
        .await?;
    println!("  {} 容器已停止并删除", "OK".green());

    Ok(())
}
