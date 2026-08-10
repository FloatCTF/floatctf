//! Configuration validation logic.

use std::path::Path;

use anyhow::{Context, Result};
use colored::Colorize;

use crate::metadata::{
    ArtifactKind, ChallengeFlagConfig, ChallengeMeta, GameBoxMeta, build_artifact_image_ref,
};

/// Result of a configuration check.
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

/// Check a Challenge package directory (v1 manifest).
///
/// Validates:
/// - `meta.toml` parse + semantic rules (version, safe_name, flag, docker port/resources, attachment path)
/// - attachment file exists under the package root (when configured)
pub fn check_challenge(dir: &Path) -> Result<CheckResult> {
    let meta_path = dir.join("meta.toml");
    let mut messages = Vec::new();
    let mut passed = true;

    messages.push(CheckMessage {
        level: CheckLevel::Ok,
        section: "配置文件".into(),
        message: format!("配置文件: {:?}", meta_path),
    });

    // Parse TOML + semantic validation
    let content = std::fs::read_to_string(&meta_path).context("Failed to read meta.toml")?;

    match ChallengeMeta::parse_and_validate(&content) {
        Ok(cfg) => {
            messages.push(CheckMessage {
                level: CheckLevel::Ok,
                section: "解析结果".into(),
                message: "配置文件解析成功".into(),
            });

            // safe_name
            match cfg.resolved_safe_name() {
                Ok(s) => messages.push(CheckMessage {
                    level: CheckLevel::Ok,
                    section: "safe_name".into(),
                    message: format!("safe_name = {s}"),
                }),
                Err(e) => {
                    messages.push(CheckMessage {
                        level: CheckLevel::Err,
                        section: "safe_name".into(),
                        message: e.to_string(),
                    });
                    passed = false;
                }
            }

            // flag config
            let flag_type = match &cfg.flag {
                ChallengeFlagConfig::Dynamic => "dynamic",
                ChallengeFlagConfig::Static { .. } => "static",
            };
            messages.push(CheckMessage {
                level: CheckLevel::Ok,
                section: "flag".into(),
                message: format!("type = {flag_type}"),
            });

            // Check attachment
            messages.push(CheckMessage {
                level: CheckLevel::Ok,
                section: "附件检查".into(),
                message: String::new(),
            });

            if let Some(attachment) = &cfg.attachment {
                let attachment_path = dir.join(attachment);
                if attachment_path.exists() {
                    let size = std::fs::metadata(&attachment_path)
                        .map(|m| m.len())
                        .unwrap_or(0);
                    messages.push(CheckMessage {
                        level: CheckLevel::Ok,
                        section: "附件检查".into(),
                        message: format!("附件存在: {:?} ({} bytes)", attachment_path, size),
                    });
                } else {
                    messages.push(CheckMessage {
                        level: CheckLevel::Err,
                        section: "附件检查".into(),
                        message: format!("附件不存在: {:?}", attachment_path),
                    });
                    passed = false;
                }
            } else {
                messages.push(CheckMessage {
                    level: CheckLevel::Warn,
                    section: "附件检查".into(),
                    message: "未配置附件".into(),
                });
            }

            // Docker check (port + recommended_resources already validated by parse_and_validate)
            messages.push(CheckMessage {
                level: CheckLevel::Ok,
                section: "Docker 检查".into(),
                message: String::new(),
            });

            if let Some(ref docker) = cfg.docker {
                messages.push(CheckMessage {
                    level: CheckLevel::Ok,
                    section: "Docker 检查".into(),
                    message: format!("Docker 配置存在, port = {}", docker.port),
                });
                match &docker.recommended_resources {
                    Some(res) => messages.push(CheckMessage {
                        level: CheckLevel::Ok,
                        section: "资源配置".into(),
                        message: format!(
                            "cpu_millis={}, memory_bytes={}, pids_limit={}",
                            res.cpu_millis, res.memory_bytes, res.pids_limit
                        ),
                    }),
                    None => messages.push(CheckMessage {
                        level: CheckLevel::Warn,
                        section: "资源配置".into(),
                        message: "未配置 recommended_resources（将使用默认值）".into(),
                    }),
                }
            } else {
                messages.push(CheckMessage {
                    level: CheckLevel::Warn,
                    section: "Docker 检查".into(),
                    message: "未配置 Docker".into(),
                });
            }
        }
        Err(e) => {
            messages.push(CheckMessage {
                level: CheckLevel::Err,
                section: "解析结果".into(),
                message: format!("配置文件解析/校验失败: {}", e),
            });
            passed = false;
        }
    }

    Ok(CheckResult { passed, messages })
}

/// Check a GameBox package directory (new portable manifest).
///
/// Validates:
/// - `meta.toml` parse + semantic rules (version, safe_name, healthchecks, judge path, …)
/// - `src/Dockerfile` exists
/// - If `[judge]` is set, the script file exists under the package root
pub fn check_gamebox(dir: &Path) -> Result<CheckResult> {
    let meta_path = dir.join("meta.toml");
    let mut messages = Vec::new();
    let mut passed = true;

    messages.push(CheckMessage {
        level: CheckLevel::Ok,
        section: "配置文件".into(),
        message: format!("配置文件: {:?}", meta_path),
    });

    let content = std::fs::read_to_string(&meta_path).context("Failed to read meta.toml")?;

    match GameBoxMeta::parse_and_validate(&content) {
        Ok(cfg) => {
            messages.push(CheckMessage {
                level: CheckLevel::Ok,
                section: "解析结果".into(),
                message: "配置文件解析成功".into(),
            });

            // safe_name
            match cfg.resolved_safe_name() {
                Ok(s) => messages.push(CheckMessage {
                    level: CheckLevel::Ok,
                    section: "safe_name".into(),
                    message: format!("safe_name = {s}"),
                }),
                Err(e) => {
                    messages.push(CheckMessage {
                        level: CheckLevel::Err,
                        section: "safe_name".into(),
                        message: e.to_string(),
                    });
                    passed = false;
                }
            }

            // version
            messages.push(CheckMessage {
                level: CheckLevel::Ok,
                section: "version".into(),
                message: format!("version = {}", cfg.version),
            });

            // recommended resources
            if let Some(ref res) = cfg.gamebox.recommended_resources {
                if res.cpu_millis <= 0 || res.memory_bytes <= 0 || res.pids_limit <= 0 {
                    messages.push(CheckMessage {
                        level: CheckLevel::Err,
                        section: "资源配置".into(),
                        message: "recommended_resources 字段必须大于 0".into(),
                    });
                    passed = false;
                } else {
                    messages.push(CheckMessage {
                        level: CheckLevel::Ok,
                        section: "资源配置".into(),
                        message: format!(
                            "cpu_millis={}, memory_bytes={}, pids_limit={}",
                            res.cpu_millis, res.memory_bytes, res.pids_limit
                        ),
                    });
                }
            } else {
                messages.push(CheckMessage {
                    level: CheckLevel::Warn,
                    section: "资源配置".into(),
                    message: "未配置 recommended_resources（将使用默认值）".into(),
                });
            }

            // healthchecks
            messages.push(CheckMessage {
                level: CheckLevel::Ok,
                section: "healthchecks".into(),
                message: format!("{} 条 readiness 探针", cfg.gamebox.healthchecks.len()),
            });

            // src/Dockerfile required
            let dockerfile = dir.join("src").join("Dockerfile");
            if dockerfile.exists() {
                messages.push(CheckMessage {
                    level: CheckLevel::Ok,
                    section: "Dockerfile".into(),
                    message: format!("存在: {:?}", dockerfile),
                });
            } else {
                messages.push(CheckMessage {
                    level: CheckLevel::Err,
                    section: "Dockerfile".into(),
                    message: format!("缺少 src/Dockerfile: {:?}", dockerfile),
                });
                passed = false;
            }

            // judge script file
            if let Some(ref judge) = cfg.judge {
                let script_path = dir.join(&judge.script);
                if script_path.exists() {
                    messages.push(CheckMessage {
                        level: CheckLevel::Ok,
                        section: "Judge".into(),
                        message: format!("脚本存在: {:?}", script_path),
                    });
                } else {
                    messages.push(CheckMessage {
                        level: CheckLevel::Err,
                        section: "Judge".into(),
                        message: format!("脚本不存在: {:?}", script_path),
                    });
                    passed = false;
                }
            } else {
                messages.push(CheckMessage {
                    level: CheckLevel::Warn,
                    section: "Judge".into(),
                    message: "未配置 [judge]".into(),
                });
            }
        }
        Err(e) => {
            messages.push(CheckMessage {
                level: CheckLevel::Err,
                section: "解析结果".into(),
                message: format!("配置文件解析/校验失败: {}", e),
            });
            passed = false;
        }
    }

    Ok(CheckResult { passed, messages })
}

/// Print check results to stdout.
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

/// Runtime check: create a test container, print access info, and keep it alive
/// until the user presses Enter (or Ctrl+C), then stop and remove it.
///
/// Image ref is derived via `build_artifact_image_ref(Challenge, "floatctf", …)`;
/// dynamic flags are injected as `FLAG` env (static flags are baked into the image).
pub async fn check_challenge_runtime(dir: &Path) -> Result<()> {
    use crate::runtime::{
        ContainerRuntime, ContainerSpec, DockerContainerRuntime, ImageRuntime, PortBinding,
        ResourceLimits,
    };
    use bollard::Docker;
    use tokio::io::{AsyncBufReadExt, BufReader};

    let meta_path = dir.join("meta.toml");
    let content = std::fs::read_to_string(&meta_path).context("Failed to read meta.toml")?;
    let cfg = ChallengeMeta::parse_and_validate(&content).context("Invalid challenge meta.toml")?;

    let docker_meta = cfg.docker.as_ref().context("No docker config found")?;

    let safe_name = cfg
        .resolved_safe_name()
        .context("Cannot resolve safe_name for runtime image ref")?;
    let image_ref = build_artifact_image_ref(
        ArtifactKind::Challenge,
        "floatctf",
        &safe_name,
        &cfg.version,
    );

    let docker = Docker::connect_with_defaults().context("Failed to connect to Docker")?;

    let rt = DockerContainerRuntime::new(docker.clone());
    // 容器名必须合法（仅 [a-zA-Z0-9_.-]），故用 safe_name 而非显示名。
    let container_name = format!("fcmc_check_{safe_name}");

    // 本地缺镜像时自动拉取（检查前先把镜像就位）。
    rt.ensure_image(&image_ref, None).await?;

    // Dynamic flag: platform injects FLAG env (entrypoint writes it to /flag).
    // Static flag: the flag is baked into the image — no env.
    let env = match &cfg.flag {
        ChallengeFlagConfig::Dynamic => vec!["FLAG=flag{runtime-check}".to_string()],
        ChallengeFlagConfig::Static { .. } => Vec::new(),
    };

    let handle = rt
        .create_and_start(ContainerSpec {
            name: container_name.clone(),
            image: image_ref,
            env,
            labels: Default::default(),
            network_name: None,
            fixed_ip: None,
            port_bindings: vec![PortBinding {
                container_port: format!("{}/tcp", docker_meta.port),
                host_ip: Some("0.0.0.0".into()),
                host_port: None,
            }],
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
            "  {} 未发布任何端口（检查 meta.toml 的 [docker] port 配置）",
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
    if matches!(cfg.flag, ChallengeFlagConfig::Dynamic) {
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
