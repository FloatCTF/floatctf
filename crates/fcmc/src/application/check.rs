//! Configuration validation logic.

use std::path::Path;

use anyhow::{Context, Result};
use colored::Colorize;

use crate::metadata::{ChallengeMeta, GameBoxMeta};

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

/// Check a ChallengeMeta configuration directory.
pub fn check_challenge(dir: &Path) -> Result<CheckResult> {
    let meta_path = dir.join("meta.toml");
    let mut messages = Vec::new();
    let mut passed = true;

    messages.push(CheckMessage {
        level: CheckLevel::Ok,
        section: "配置文件".into(),
        message: format!("配置文件: {:?}", meta_path),
    });

    // Parse TOML
    let content = std::fs::read_to_string(&meta_path).context("Failed to read meta.toml")?;

    match toml::from_str::<ChallengeMeta>(&content) {
        Ok(cfg) => {
            messages.push(CheckMessage {
                level: CheckLevel::Ok,
                section: "解析结果".into(),
                message: "配置文件解析成功".into(),
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

            // Docker check (static only)
            messages.push(CheckMessage {
                level: CheckLevel::Ok,
                section: "Docker 检查".into(),
                message: String::new(),
            });

            if cfg.docker.is_some() {
                messages.push(CheckMessage {
                    level: CheckLevel::Ok,
                    section: "Docker 检查".into(),
                    message: "Docker 配置存在".into(),
                });
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
                message: format!("配置文件解析失败: {}", e),
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

/// Runtime check: create a test container and verify it starts.
pub async fn check_challenge_runtime(dir: &Path) -> Result<()> {
    use crate::runtime::{
        ContainerRuntime, ContainerSpec, DockerContainerRuntime, PortBinding, ResourceLimits,
    };
    use bollard::Docker;

    let meta_path = dir.join("meta.toml");
    let content = std::fs::read_to_string(&meta_path).context("Failed to read meta.toml")?;
    let cfg: ChallengeMeta = toml::from_str(&content).context("Failed to parse meta.toml")?;

    let docker_meta = cfg.docker.as_ref().context("No docker config found")?;

    let docker = Docker::connect_with_defaults().context("Failed to connect to Docker")?;

    let rt = DockerContainerRuntime::new(docker.clone());
    let container_name = format!("fcmc_check_{}", cfg.name);
    let flag = "flag{runtime-check}";

    let handle = rt
        .create_and_start(ContainerSpec {
            name: container_name.clone(),
            image: docker_meta.image_tag.clone(),
            env: vec![format!("{}={}", cfg.flag.env_var, flag)],
            labels: Default::default(),
            network_name: None,
            fixed_ip: None,
            port_bindings: vec![PortBinding {
                container_port: docker_meta.port.clone(),
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

    // Auto-cleanup
    rt.stop_and_remove(&handle.container_id, std::time::Duration::from_secs(5))
        .await?;

    Ok(())
}
