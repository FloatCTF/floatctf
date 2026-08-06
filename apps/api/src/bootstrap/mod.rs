//! Application bootstrap — initialization and HTTP server startup.
//!
//! This module contains the startup logic previously in `main.rs`,
//! exposed as a library function for integration testing.

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
    modules::event::EventModuleRegistry,
    modules::event::awd_team::crypto::AwdCrypto,
};

pub use state::{AppState, AwdDependencies};

/// Initialize and run the FloatCTF HTTP server.
///
/// This is the single entry point for both the binary and integration tests.
/// Returns `Err` if initialization fails (database, Docker, or S3 connection).
pub async fn run() -> std::io::Result<()> {
    // Load environment variables
    dotenvy::dotenv().ok();

    // Typed static config — fail fast before touching infrastructure.
    let config = match AppConfig::from_env() {
        Ok(c) => Arc::new(c),
        Err(e) => {
            eprintln!("configuration error: {e}");
            panic!("configuration error: {e}");
        }
    };

    // Set working directory
    std::env::set_current_dir(&config.server.work_dir)
        .unwrap_or_else(|e| panic!("failed to set WORK_DIR={}: {}", config.server.work_dir, e));

    // Initialize logging
    init_logging(&config.server.log_dir);

    let version = env!("CARGO_PKG_VERSION");
    info!(
        "Current working dir = {}, version = {}",
        config.server.work_dir, version
    );
    config.log_source_summary();

    // JWT secret once for the process lifetime
    jwt::configure_jwt_secret(config.auth.jwt_secret.clone());

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
    seed_default_settings(&db).await;
    let log_service = LogService::new(db.clone());
    let audit_service = AuditService::new(log_service.clone());
    // Realtime hub: local broadcast for SSE; optional Redis fan-out when
    // REALTIME_REDIS_URL is set (feature `realtime-redis`). See infrastructure/realtime.
    let (broadcast_hub, publisher) = crate::infrastructure::realtime::build_realtime_from_env(256);

    // AWD host network runtime (shared by HTTP + scheduler).
    let awd_network: Arc<
        dyn crate::modules::event::awd_team::infrastructure::network::AwdNetworkRuntime,
    > = match std::env::var("AWD_HOST_NETWORK").as_deref() {
        Ok("1") | Ok("true") | Ok("TRUE") => {
            info!("AWD_HOST_NETWORK enabled — using HostNetworkRuntime");
            Arc::new(
                crate::modules::event::awd_team::infrastructure::network::HostNetworkRuntime::new(),
            )
        }
        _ => Arc::new(crate::modules::event::awd_team::infrastructure::network::NoopNetworkRuntime),
    };

    // Initialize scheduler
    let task_scheduler = scheduler::build_task_scheduler(
        db.clone(),
        docker.clone(),
        rustfs.clone(),
        log_service.clone(),
        awd_network.clone(),
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
        audit_service,
        publisher.clone(),
        task_scheduler_arc.clone(),
        EventModuleRegistry::new(),
    ));

    // AWD container runtime + crypto
    let awd_containers: Arc<dyn fcmc::AwdContainerRuntime> =
        Arc::new(fcmc::DockerRuntime::new(docker.get_ref().clone()));

    let awd_deps = match AwdCrypto::from_secret_bytes(config.auth.jwt_secret.as_bytes()) {
        Ok(crypto) => web::Data::new(AwdDependencies {
            crypto: Arc::new(crypto),
            publisher: publisher.clone(),
            containers: awd_containers.clone(),
            network: awd_network.clone(),
        }),
        Err(e) => {
            error!(
                "Failed to initialize AWD crypto: {}. AWD features will be unavailable.",
                e
            );
            web::Data::new(AwdDependencies {
                crypto: Arc::new(AwdCrypto::new(
                    crate::modules::event::awd_team::crypto::AwdSecret::new(vec![0u8; 32]),
                )),
                publisher: publisher.clone(),
                containers: awd_containers,
                network: awd_network,
            })
        }
    };

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
            // Same registry instance as AppState (handlers may extract either)
            .app_data(web::Data::new(app_state.get_ref().event_registry.clone()))
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
    .await
}

pub use routes::{configure_all_routes, configure_routes};

// ── Private helpers ──

fn init_logging(log_dir: &str) {
    use tracing_appender::rolling;
    use tracing_subscriber::{EnvFilter, fmt::writer::MakeWriterExt};

    let file_appender = rolling::daily(log_dir, "log");
    let (file_writer, _guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env().add_directive("default=info".parse().unwrap()),
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
