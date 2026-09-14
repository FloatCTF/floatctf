use clap::Parser;
use colored::*;
use fcmc::application::{build, check, generate, manual};
use fcmc::{Commands, GenFormat};
use std::path::{Path, PathBuf};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = fcmc::Args::parse();

    match args.command {
        Commands::Check { path, runtime } => {
            let dir = path.unwrap_or_else(|| ".".to_string());
            let dir = PathBuf::from(&dir);

            // 按 meta.toml 内容自动识别包类型：含 [gamebox] 段按 GameBox 检查，否则按 Challenge。
            let is_gamebox = matches!(detect_package_kind(&dir), GenFormat::Gamebox);

            let result = if is_gamebox {
                check::check_gamebox(&dir)?
            } else {
                check::check_challenge(&dir)?
            };
            check::print_check_result(&result);

            if runtime && result.passed {
                println!("\n[运行时检查]");
                let r = if is_gamebox {
                    check::check_gamebox_runtime(&dir).await
                } else {
                    check::check_challenge_runtime(&dir).await
                };
                match r {
                    Ok(_) => println!("  {}   运行时检查通过", "OK".green()),
                    Err(e) => {
                        println!("  {}   运行时检查失败: {}", "ERR".red(), e);
                        std::process::exit(1);
                    }
                }
            }

            if !result.passed {
                std::process::exit(1);
            }
        }
        Commands::Help { agent, command } => {
            if agent {
                manual::print_agent_manual();
            } else if let Some(cmd) = command {
                if let Err(e) = manual::print_command_manual(&cmd) {
                    anyhow::bail!("{e}");
                }
            } else {
                // 无参: 打印 clap 原生帮助。
                use clap::CommandFactory;
                fcmc::Args::command().print_help()?;
                println!();
            }
        }
        Commands::Gen {
            name,
            output,
            format,
            template,
        } => match format {
            GenFormat::Challenge => {
                generate::generate_challenge(&name, &output).await?;
            }
            GenFormat::Gamebox => {
                generate::generate_gamebox(&name, &output, template).await?;
            }
        },
        Commands::Build {
            path,
            format,
            tag,
            proxy,
        } => {
            let dir = path.unwrap_or_else(|| ".".to_string());
            let dir = PathBuf::from(&dir);

            // 未显式指定 --format 时按 meta.toml 自动识别包类型。
            let format = format.unwrap_or_else(|| detect_package_kind(&dir));

            match format {
                GenFormat::Challenge => {
                    build::build_challenge(&dir, tag.as_deref(), proxy.as_deref()).await?;
                }
                GenFormat::Gamebox => {
                    build::build_gamebox(&dir, tag.as_deref(), proxy.as_deref()).await?;
                }
            }
        }
    }

    Ok(())
}

/// 按 meta.toml 内容自动识别包类型：含 `[gamebox]` 段按 GameBox，否则按 Challenge。
/// 供 `check` 与未显式指定 `--format` 的 `build` 使用。
fn detect_package_kind(dir: &Path) -> GenFormat {
    let is_gamebox = std::fs::read_to_string(dir.join("meta.toml"))
        .map(|c| c.contains("[gamebox]"))
        .unwrap_or(false);
    if is_gamebox {
        GenFormat::Gamebox
    } else {
        GenFormat::Challenge
    }
}
