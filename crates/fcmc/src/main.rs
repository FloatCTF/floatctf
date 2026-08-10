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

            let result = check::check_challenge(&dir)?;
            check::print_check_result(&result);

            if runtime && result.passed {
                println!("\n[运行时检查]");
                match check::check_challenge_runtime(&dir).await {
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
        Commands::Build { path, format, tag } => {
            let dir = path.unwrap_or_else(|| ".".to_string());
            let dir = PathBuf::from(&dir);

            match format {
                GenFormat::Challenge => {
                    build::build_challenge(&dir).await?;
                }
                GenFormat::Gamebox => {
                    build::build_gamebox(&dir, tag.as_deref()).await?;
                }
            }
        }
    }

    Ok(())
}
