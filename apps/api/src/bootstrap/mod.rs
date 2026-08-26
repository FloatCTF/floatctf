//! 应用引导：初始化与 HTTP 服务启动。
//!
//! 原 `main.rs` 中的启动逻辑集中于此，并以库函数形式暴露，供集成测试复用。

pub mod routes;
pub mod scheduler;
pub mod state;

use actix_cors::Cors;
use actix_web::{App, HttpServer, middleware::Logger, web};
use std::sync::Arc;
use tracing::{error, info};
use tracing_actix_web::TracingLogger;

use crate::{
    core::AppConfig,
    core::security::jwt,
    infrastructure::{
        LogService, WebDb, WebDocker, WebRustfs, audit::AuditService, database, docker,
        seed_default_settings, storage,
    },
    modules::event::awd::crypto::AwdCrypto,
};

pub use state::{AppState, AwdDependencies};

/// 启动阶段致命错误（fail-fast）。
///
/// Phase 0 P0-2：AWD crypto 初始化失败不允许降级运行（历史存在全零密钥回退，
/// 攻击者可预测所有 token/flag）。`run()` 返回 Err 后由 `main` 打印原因并 `exit(1)`。
#[derive(Debug, thiserror::Error)]
pub enum BootstrapError {
    #[error("AWD crypto initialization failed: {0}")]
    Crypto(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// 初始化并运行 FloatCTF HTTP 服务。
///
/// 二进制与集成测试共用的唯一入口。
/// 初始化失败（数据库、Docker、S3 或 AWD 加密）时返回 `Err`。
pub async fn run() -> Result<(), BootstrapError> {
    // Load all process-static settings from TOML — fail fast before touching infrastructure.
    let config_path = std::env::var_os("FLOATCTF_CONFIG")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("config/development.toml"));
    let config = match AppConfig::from_file(&config_path) {
        Ok(c) => Arc::new(c),
        Err(e) => {
            eprintln!("configuration error: {e}");
            panic!("configuration error: {e}");
        }
    };

    // Set working directory: absolutize relative paths before chdir so later
    // derived paths (e.g. logs) don't double-apply the relative work_dir.
    // 先记录启动目录：settings 里 `{{WORK_DIR}}/challenges` 等相对路径以它为锚。
    let launch_dir = std::env::current_dir().unwrap_or_default();
    crate::infrastructure::settings::set_launch_dir(launch_dir.clone());
    let work_dir_abs = launch_dir.join(&config.server.work_dir);
    std::env::set_current_dir(&work_dir_abs)
        .unwrap_or_else(|e| panic!("failed to set WORK_DIR={}: {}", config.server.work_dir, e));

    // Honor the configured timezone for the process-local logger (ChronoLocal).
    // Must run before init_logging so log timestamps use the configured zone.
    if !config.logging.timezone.is_empty() {
        // SAFETY: single-threaded startup; no other thread reads TZ yet.
        unsafe { std::env::set_var("TZ", &config.logging.timezone) };
    }

    // Initialize logging: logs always live under WORK_DIR/logs/<service>.
    let log_dir = work_dir_abs.join("logs").join("api");
    let log_dir = log_dir.to_string_lossy().into_owned();
    init_logging(&log_dir, &config.logging.filter);

    let version = env!("CARGO_PKG_VERSION");
    info!(
        "Current working dir = {}, version = {}",
        config.server.work_dir, version
    );
    config.log_source_summary();

    // Secrets are loaded once from TOML for the process lifetime.
    jwt::configure_jwt_secret(config.auth.jwt_secret.clone());
    AwdCrypto::configure_secret(config.auth.jwt_secret.clone());

    // Infrastructure
    let db: WebDb = match database::connect(&config.database).await {
        Ok(db) => web::Data::new(db),
        Err(e) => {
            error!("init db failed: {}", e);
            panic!("init db failed: {}", e);
        }
    };

    let docker: WebDocker = match docker::connect(&config.docker).await {
        Ok(docker) => web::Data::new(docker),
        Err(e) => {
            error!("init docker failed: {}", e);
            panic!("init docker failed: {}", e);
        }
    };

    let rustfs: WebRustfs = match storage::connect(&config.storage).await {
        Ok(rustfs) => web::Data::new(rustfs),
        Err(e) => {
            error!("init rustfs failed: {}", e);
            panic!("init rustfs failed: {}", e);
        }
    };

    // Dynamic DB settings seed (upsert defaults, never overwrite)
    seed_default_settings(&db, &config).await;
    let log_service = LogService::new(db.clone());
    let audit_service = AuditService::new(log_service.clone());
    // Realtime hub: local broadcast with optional Redis fan-out from TOML.
    let (broadcast_hub, publisher) = crate::infrastructure::realtime::build_realtime(
        256,
        config.realtime.redis_url.as_deref(),
        config.realtime.redis_channel.as_deref(),
    );

    // AWD host network runtime (shared by HTTP + scheduler).
    let awd_network: Arc<
        dyn crate::modules::event::awd::infrastructure::network::AwdNetworkRuntime,
    > = if config.awd.network_runtime == "host" {
        info!("AWD host network enabled — using HostNetworkRuntime");
        Arc::new(crate::modules::event::awd::infrastructure::network::HostNetworkRuntime::new())
    } else {
        info!("AWD host network disabled (network_runtime=noop) — NoopNetworkRuntime");
        Arc::new(crate::modules::event::awd::infrastructure::network::NoopNetworkRuntime)
    };

    // AWD firewall runtime：唯一生产实现为 native nftables（Phase 1）。
    // Noop 仅用于 unit test / dev mock，且 Noop 永远不允许 Verified（Phase 2 双门禁）。
    let awd_firewall: Arc<
        dyn crate::modules::event::awd::infrastructure::firewall::FirewallRuntime,
    > = if config.awd.network_runtime == "host" {
        info!("AWD firewall enabled — using NftablesFirewallRuntime");
        Arc::new(
            crate::modules::event::awd::infrastructure::firewall::NftablesFirewallRuntime::new(),
        )
    } else {
        info!("AWD firewall disabled — using NoopFirewallRuntime (dev/mock only)");
        Arc::new(crate::modules::event::awd::infrastructure::firewall::NoopFirewallRuntime)
    };

    // AWD crypto（fail-fast，Phase 0 P0-2）
    let awd_crypto = Arc::new(
        AwdCrypto::from_secret_bytes(config.auth.jwt_secret.as_bytes())
            .map_err(|e| BootstrapError::Crypto(e.to_string()))?,
    );

    // AWD startup recovery（Phase 1 P1-16 接线）：
    // Firewall Recovery 必须先于 AWD Scheduler（§5.14 启动顺序）。
    // recover_all 失败不阻塞启动——记录错误，交由 Precheck/Start gate 判定。
    {
        let awd_containers_for_recovery: Arc<dyn fcmc::AwdContainerRuntime> =
            Arc::new(fcmc::DockerRuntime::new(docker.get_ref().clone()));
        match crate::modules::event::awd::service::recovery_service::recover_all(
            db.get_ref(),
            awd_containers_for_recovery.as_ref(),
            awd_network.as_ref(),
            awd_firewall.as_ref(),
            awd_crypto.as_ref(),
            publisher.as_ref(),
        )
        .await
        {
            Ok(n) => info!("[Recovery] startup recovery complete: {n} resources reconciled"),
            Err(e) => error!("[Recovery] startup recovery failed: {}", e),
        }
    }

    // Initialize scheduler
    let task_scheduler = scheduler::build_task_scheduler(
        db.clone(),
        docker.clone(),
        rustfs.clone(),
        log_service.clone(),
        awd_network.clone(),
        awd_firewall.clone(),
        awd_crypto.clone(),
        publisher.clone(),
        config.clone(),
    )
    .await
    .expect("init startup handlers failed!");

    let task_scheduler_arc = std::sync::Arc::new(task_scheduler);
    task_scheduler_arc
        .init_and_recover()
        .await
        .expect("init task scheduler failed!");

    // Start background polling
    let sc_clone = task_scheduler_arc.clone();
    actix_web::rt::spawn(async move {
        sc_clone.start_polling().await;
    });

    // Create centralized AppState
    let app_state = web::Data::new(AppState::new(
        config.clone(),
        db.get_ref().clone(),
        docker.get_ref().clone(),
        rustfs.get_ref().clone(),
        log_service.clone(),
        audit_service.clone(),
        publisher.clone(),
        task_scheduler_arc.clone(),
    ));

    // AWD container runtime + crypto
    let awd_containers: Arc<dyn fcmc::AwdContainerRuntime> =
        Arc::new(fcmc::DockerRuntime::new(docker.get_ref().clone()));

    let awd_deps = web::Data::new(AwdDependencies {
        crypto: awd_crypto.clone(),
        publisher: publisher.clone(),
        containers: awd_containers.clone(),
        network: awd_network.clone(),
        firewall: awd_firewall.clone(),
        rate_limiter: Arc::new(crate::infrastructure::ratelimit::RateLimiter::new()),
        audit: audit_service.clone(),
    });

    let ip = config.server.listen_ip.clone();
    let port = config.server.listen_port;
    let cors_origins = config.cors.allowed_origins.clone();

    info!("Starting server on {}:{}", ip, port);

    HttpServer::new(move || {
        let cors = build_cors(&cors_origins);

        App::new()
            .wrap(Logger::default())
            .wrap(TracingLogger::default())
            .wrap(cors)
            // New centralized state
            .app_data(app_state.clone())
            .app_data(awd_deps.clone())
            // Concrete hub for SSE / WS fan-out (subscribe)
            .app_data(web::Data::from(broadcast_hub.clone()))
            // Legacy individual app_data (kept for backward compatibility)
            .app_data(db.clone())
            .app_data(docker.clone())
            .app_data(rustfs.clone())
            .app_data(web::Data::new(log_service.clone()))
            .app_data(web::Data::new(task_scheduler_arc.clone()))
            // All routes registered centrally
            .configure(configure_all_routes)
    })
    .bind((ip, port))?
    .run()
    .await?;
    Ok(())
}

pub use routes::{configure_all_routes, configure_routes};

// ── Private helpers ──

fn init_logging(log_dir: &str, filter: &str) {
    use tracing_appender::rolling;
    use tracing_subscriber::{EnvFilter, fmt::writer::MakeWriterExt};

    let file_appender = rolling::daily(log_dir, "log");
    let (file_writer, guard) = tracing_appender::non_blocking(file_appender);
    // 泄漏 guard：non_blocking 的 worker 线程必须存活到进程结束。
    // 若随函数返回 drop，worker 被关停，文件日志会全部丢失（历史 bug：日志文件 0 字节）。
    std::mem::forget(guard);

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_new(filter).unwrap_or_else(|_| EnvFilter::new("default=info")),
        )
        .with_writer(std::io::stdout.and(file_writer))
        .with_timer(tracing_subscriber::fmt::time::ChronoLocal::rfc_3339())
        .init();
}

fn build_cors(allowed_origins: &[String]) -> Cors {
    let mut cors = Cors::default()
        .allow_any_header()
        .allow_any_method()
        .supports_credentials()
        .max_age(3600);
    for origin in allowed_origins {
        cors = cors.allowed_origin(origin.as_str());
    }
    cors
}
