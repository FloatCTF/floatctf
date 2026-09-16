use clap::Parser;
use colored::*;
use fcmc::application::{build, check, generate, manual};
use fcmc::{Commands, GenFormat};
use std::path::{Path, PathBuf};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = fcmc::Args::parse();

    match args.command {
        Commands::Check {
            path,
            format,
            runtime,
        } => {
            let dir = path.unwrap_or_else(|| ".".to_string());
            let dir = PathBuf::from(&dir);

            // 包类型：显式 --format > 路径 (challenges/<id> | gameboxes/<id>) >
            // standalone 含 [gamebox] 段 > 回退 challenge。
            let format = format.unwrap_or_else(|| detect_package_kind(&dir));
            let is_gamebox = matches!(format, GenFormat::Gamebox);

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
            safe_name,
        } => match format {
            GenFormat::Challenge => {
                generate::generate_challenge(&name, &output, safe_name.as_deref()).await?;
            }
            GenFormat::Gamebox => {
                generate::generate_gamebox(&name, &output, template, safe_name.as_deref()).await?;
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

            // 未显式指定 --format 时按路径与 meta.toml 自动识别包类型。
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

/// 识别包类型（floatctf-content 的目录约定优先）：
///
/// 1. 用户显式 `--format`（调用方处理，不在此函数内）
/// 2. 路径位于 `gameboxes/<id>` → GameBox
/// 3. 路径位于 `challenges/<id>` → Challenge
/// 4. standalone 包：meta.toml 存在 `[gamebox]` 段 → GameBox
/// 5. 最后回退 → Challenge
///
/// 注意：官方 canonical GameBox **可以没有** `[gamebox]` 段，所以 standalone 包
/// 若没写 `[gamebox]` 时需要显式 `--format gamebox`（或放进 `gameboxes/<id>`）。
fn detect_package_kind(dir: &Path) -> GenFormat {
    let resolved = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());

    if let Some(parent) = resolved
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
    {
        match parent {
            "gameboxes" => return GenFormat::Gamebox,
            "challenges" => return GenFormat::Challenge,
            _ => {}
        }
    }

    let is_gamebox = std::fs::read_to_string(resolved.join("meta.toml"))
        .map(|raw| {
            raw.parse::<toml::Table>()
                .is_ok_and(|t| t.contains_key("gamebox"))
        })
        .unwrap_or(false);

    if is_gamebox {
        GenFormat::Gamebox
    } else {
        GenFormat::Challenge
    }
}
