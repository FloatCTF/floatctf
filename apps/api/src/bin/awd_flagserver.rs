//! AWD FlagServer — standalone HTTP service for on-demand flag issuing.
//!
//! # Architecture
//!
//! 1. GameBox makes an HTTP request to FlagServer (via Docker network)
//! 2. FlagServer reads the TCP source IP from the socket
//! 3. FlagServer calls the platform's internal API to issue the flag
//! 4. Platform validates (event running, attack phase, team not banned) and returns flag
//!
//! # Configuration (env vars)
//!
//! - `PLATFORM_INTERNAL_URL` — base URL of the FloatCTF platform
//! - `EVENT_ID` — UUID of the AWD event this server serves
//! - `INTERNAL_TOKEN` — Bearer token for platform authentication
//! - `LISTEN_ADDR` — bind address (default "0.0.0.0:8081")
//! - `REQUEST_TIMEOUT_SECS` — platform request timeout (default 10)
//! - `RETRY_COUNT` — retry count on platform failure (default 3)

use actix_web::{App, HttpRequest, HttpResponse, HttpServer, middleware, web};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::env;
use tokio::time::Duration;

#[derive(Debug, Serialize, Deserialize)]
struct IssueFlagRequest {
    source_ip: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct IssueFlagResponse {
    code: i32,
    message: String,
    data: Option<FlagData>,
}

#[derive(Debug, Serialize, Deserialize)]
struct FlagData {
    flag: String,
}

#[derive(Clone)]
struct AppState {
    client: Client,
    platform_url: String,
    event_id: String,
    internal_token: String,
    retry_count: u32,
}

/// GET /flag — return a flag for the requesting GameBox.
///
/// The source IP is read from the TCP connection, NOT from headers.
async fn get_flag(req: HttpRequest, state: web::Data<AppState>) -> HttpResponse {
    // Read REAL source IP from the TCP connection
    let source_ip = req
        .connection_info()
        .realip_remote_addr()
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            // Fallback: try peer_addr
            req.peer_addr()
                .map(|a| a.ip().to_string())
                .unwrap_or_else(|| "unknown".to_string())
        });

    let request_body = IssueFlagRequest {
        source_ip: source_ip.clone(),
    };

    let url = format!(
        "{}/internal/awd/events/{}/flags/issue",
        state.platform_url, state.event_id
    );

    // Retry loop
    for attempt in 1..=state.retry_count {
        let result = state
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", state.internal_token))
            .json(&request_body)
            .timeout(Duration::from_secs(10))
            .send()
            .await;

        match result {
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() {
                    match resp.json::<IssueFlagResponse>().await {
                        Ok(body) => {
                            if body.code == 0 {
                                if let Some(data) = body.data {
                                    // Log flag issuance (flag value not logged!)
                                    tracing::info!(
                                        "Flag issued for IP: {} (attempt {})",
                                        source_ip,
                                        attempt
                                    );
                                    return HttpResponse::Ok()
                                        .content_type("text/plain")
                                        .body(data.flag);
                                }
                            }
                            // Platform returned an error
                            tracing::warn!(
                                "Platform refused flag for IP {}: {}",
                                source_ip,
                                body.message
                            );
                            return HttpResponse::build(
                                actix_web::http::StatusCode::from_u16(status.as_u16())
                                    .unwrap_or(actix_web::http::StatusCode::SERVICE_UNAVAILABLE),
                            )
                            .content_type("text/plain")
                            .body(format!("Platform: {}", body.message));
                        }
                        Err(e) => {
                            tracing::error!("Failed to parse platform response: {}", e);
                        }
                    }
                } else {
                    tracing::warn!("Platform returned {} (attempt {})", status, attempt);
                }
            }
            Err(e) => {
                tracing::error!(
                    "Platform request failed (attempt {}/{}): {}",
                    attempt,
                    state.retry_count,
                    e
                );
            }
        }

        if attempt < state.retry_count {
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    // All retries exhausted
    HttpResponse::ServiceUnavailable()
        .content_type("text/plain")
        .body("Platform unavailable — try again later")
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("default=info".parse().unwrap()),
        )
        .init();

    let platform_url =
        env::var("PLATFORM_INTERNAL_URL").unwrap_or_else(|_| "http://127.0.0.1:8080".to_string());
    let event_id = env::var("EVENT_ID").expect("EVENT_ID must be set");
    let internal_token = env::var("INTERNAL_TOKEN").expect("INTERNAL_TOKEN must be set");
    let listen_addr = env::var("LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:8081".to_string());
    let retry_count: u32 = env::var("RETRY_COUNT")
        .unwrap_or_else(|_| "3".to_string())
        .parse()
        .unwrap_or(3);

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .expect("Failed to create HTTP client");

    let state = AppState {
        client,
        platform_url,
        event_id,
        internal_token,
        retry_count,
    };

    tracing::info!("FloatCTF AWD FlagServer starting on {}", listen_addr);

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(state.clone()))
            .route("/flag", web::get().to(get_flag))
    })
    .bind(&listen_addr)?
    .run()
    .await
}
