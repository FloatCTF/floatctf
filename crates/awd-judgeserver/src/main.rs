//! FloatCTF AWD JudgeServer — 独立 Pull Worker，通过轮询执行健康检查/裁判脚本。
//!
//! 本文件只负责启动/组合；核心逻辑见 `worker.rs`、`protocol.rs`、`outcome.rs`。

mod outcome;
mod protocol;
mod worker;

use actix_web::{App, HttpResponse, HttpServer, web};
use std::env;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Semaphore;
use tokio::time::{Duration, sleep};
use uuid::Uuid;

use worker::{RealHttpClient, WorkerState, poll_loop};

// ── Health endpoints ──

async fn health() -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({"status": "ok"}))
}

async fn ready() -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({"status": "ready"}))
}

// ── Main ──

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("default=info".parse().unwrap()),
        )
        .init();

    let platform_url = env::var("PLATFORM_INTERNAL_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:9090".to_string());
    let event_id = env::var("EVENT_ID").expect("EVENT_ID must be set");
    let internal_token = env::var("INTERNAL_TOKEN").expect("INTERNAL_TOKEN must be set");
    let listen_addr = env::var("LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());
    let max_concurrent: usize = env::var("MAX_CONCURRENT")
        .unwrap_or_else(|_| "5".to_string())
        .parse()
        .unwrap_or(5);
    let work_dir = env::var("WORK_DIR").unwrap_or_else(|_| "/tmp/judge".to_string());
    let poll_interval_secs: u64 = env::var("POLL_INTERVAL_SECS")
        .unwrap_or_else(|_| "5".to_string())
        .parse()
        .unwrap_or(5);
    let heartbeat_interval_secs: u64 = env::var("HEARTBEAT_INTERVAL_SECS")
        .unwrap_or_else(|_| "30".to_string())
        .parse()
        .unwrap_or(30);
    let lease_ttl_secs: u64 = env::var("LEASE_TTL_SECS")
        .unwrap_or_else(|_| "120".to_string())
        .parse()
        .unwrap_or(120);

    if heartbeat_interval_secs >= lease_ttl_secs / 3 {
        tracing::warn!(
            "HEARTBEAT_INTERVAL_SECS ({}) >= LEASE_TTL_SECS/3 ({}); heartbeats may be too infrequent",
            heartbeat_interval_secs,
            lease_ttl_secs / 3
        );
    }

    let worker_id = Uuid::new_v4().to_string();

    tokio::fs::create_dir_all(&work_dir)
        .await
        .expect("Failed to create work directory");

    let http = RealHttpClient::new();
    let shutdown = Arc::new(AtomicBool::new(false));

    let worker_state = WorkerState {
        http: Arc::new(http),
        platform_url,
        event_id,
        internal_token,
        worker_id,
        concurrency: Arc::new(Semaphore::new(max_concurrent)),
        work_dir,
        poll_interval: Duration::from_secs(poll_interval_secs),
        heartbeat_interval: Duration::from_secs(heartbeat_interval_secs),
        shutdown: shutdown.clone(),
    };

    let poll_state = worker_state.clone();
    let poll_handle = actix_web::rt::spawn(async move {
        poll_loop(poll_state).await;
    });

    tracing::info!(
        "FloatCTF AWD JudgeServer starting on {} (max_concurrent={}, worker_id={})",
        listen_addr,
        max_concurrent,
        worker_state.worker_id
    );

    let server = HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(worker_state.clone()))
            .route("/health", web::get().to(health))
            .route("/ready", web::get().to(ready))
    })
    .bind(&listen_addr)?
    .run();

    let server_handle = server.handle();
    let shutdown_flag = shutdown.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        tracing::info!("Shutdown signal received, draining...");
        shutdown_flag.store(true, Ordering::Relaxed);
        sleep(Duration::from_secs(10)).await;
        server_handle.stop(true).await;
    });

    server.await?;
    poll_handle.abort();

    Ok(())
}