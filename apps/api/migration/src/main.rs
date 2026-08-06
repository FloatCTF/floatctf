//! CLI: apply / status / check / baseline FloatCTF schema migrations.
//!
//! ```text
//! DATABASE_URL=postgres://... cargo run -p floatctf-migration -- up
//! DATABASE_URL=postgres://... cargo run -p floatctf-migration -- status
//! DATABASE_URL=postgres://... cargo run -p floatctf-migration -- check
//! DATABASE_URL=postgres://... cargo run -p floatctf-migration -- baseline
//! ```
//!
//! Wrapper: `scripts/migrate.sh <up|status|check|baseline>`.

use clap::{Parser, Subcommand};
use floatctf_migration::{
    migrate_baseline, migrate_status, migrate_up, schema_check,
};
use tracing::{error, info, warn};

#[derive(Parser, Debug)]
#[command(name = "floatctf-migration", about = "FloatCTF DB schema migrations")]
struct Cli {
    /// PostgreSQL connection string (default: DATABASE_URL env).
    #[arg(long, env = "DATABASE_URL")]
    database_url: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Apply all pending migrations (base + AWD + incremental).
    Up,
    /// Show applied / pending migration status.
    Status,
    /// Verify expected tables, enums, and minimum column counts.
    Check {
        /// Exit non-zero if schema is incomplete (default true).
        #[arg(long, default_value_t = true)]
        strict: bool,
    },
    /// After a successful schema check, mark all migrations applied without DDL.
    ///
    /// For existing DBs that were created via Docker init / manual SQL.
    Baseline {
        /// Skip the automatic schema check (dangerous; default false).
        #[arg(long, default_value_t = false)]
        force: bool,
    },
}

#[async_std::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("floatctf_migration=info".parse().unwrap()),
        )
        .init();

    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Up => {
            info!("running migrations…");
            migrate_up(&cli.database_url).await.map(|_| {
                info!("migrations applied");
            })
        }
        Commands::Status => {
            info!("migration status:");
            migrate_status(&cli.database_url).await
        }
        Commands::Check { strict } => match schema_check::check_schema(&cli.database_url).await {
            Ok(report) => {
                print_check_report(&report);
                if strict && !report.ok() {
                    error!("schema check failed");
                    std::process::exit(1);
                }
                Ok(())
            }
            Err(e) => Err(e),
        },
        Commands::Baseline { force } => {
            if !force {
                match schema_check::check_schema(&cli.database_url).await {
                    Ok(report) => {
                        print_check_report(&report);
                        if !report.ok() {
                            error!(
                                "schema check failed — refuse to baseline. \
                                 Fix schema or pass --force (not recommended)."
                            );
                            std::process::exit(1);
                        }
                    }
                    Err(e) => {
                        error!("schema check error: {e}");
                        std::process::exit(1);
                    }
                }
            } else {
                warn!("--force: skipping schema check before baseline");
            }

            match migrate_baseline(&cli.database_url).await {
                Ok(report) => {
                    info!("{}", report.summary());
                    Ok(())
                }
                Err(e) => Err(e),
            }
        }
    };

    if let Err(e) = result {
        error!("migration error: {e}");
        std::process::exit(1);
    }
}

fn print_check_report(report: &schema_check::SchemaCheckReport) {
    info!("{}", report.summary());
    if !report.missing_core.is_empty() {
        error!("missing core tables: {:?}", report.missing_core);
    }
    if !report.missing_awd.is_empty() {
        error!("missing AWD tables: {:?}", report.missing_awd);
    }
    if !report.missing_enums.is_empty() {
        error!("missing enums: {:?}", report.missing_enums);
    }
    if !report.short_columns.is_empty() {
        for (t, live, exp) in &report.short_columns {
            error!("table {t}: column count {live} < expected min {exp}");
        }
    }
    if !report.extra_columns.is_empty() {
        for (t, live, exp) in &report.extra_columns {
            info!("table {t}: column count {live} > base SQL min {exp} (ok if historical patches)");
        }
    }
    if !report.extra.is_empty() {
        info!("extra tables (ignored): {:?}", report.extra);
    }
}
