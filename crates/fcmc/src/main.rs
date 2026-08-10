use clap::Parser;
use colored::*;
use fcmc::application::{build, check, generate};
use fcmc::{Commands, GenFormat};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = fcmc::Args::parse();

    match args.command {
        Commands::Check { path, runtime } => {
            let dir = path.unwrap_or_else(|| ".".to_string());
            let dir = PathBuf::from(&dir);

            // 按 meta.toml 内容自动识别包类型：含 [gamebox] 段按 GameBox 检查，否则按 Challenge。
            let is_gamebox = std::fs::read_to_string(dir.join("meta.toml"))
                .map(|c| c.contains("[gamebox]"))
                .unwrap_or(false);

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
