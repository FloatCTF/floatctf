//! Platform file download helpers (admin private download + presigned URLs).

use std::{env, time::Duration};

use actix_web::{get, web::Query};
use anyhow::Result;
use aws_sdk_s3::presigning::PresigningConfig;
use serde::Deserialize;
use serde_json::json;
use tracing::info;

use crate::{
    api::{AppError, UniResponse, UniResult, extractor::auth::SuperAdminJwtGuard, prelude::*},
    infrastructure::WebRustfs,
};

/// Generate a presigned GET URL for an object (shared by player event writeups, etc.).
pub async fn generate_presigned_download_url(
    rustfs: WebRustfs,
    bucket: &str,
    key: &str,
    ttl_secs: u64,
) -> Result<String> {
    let presigned = rustfs
        .get_object()
        .bucket(bucket)
        .key(key)
        .presigned(PresigningConfig::expires_in(Duration::from_secs(ttl_secs))?)
        .await?;

    Ok(presigned.uri().to_string())
}

#[derive(Deserialize)]
struct DownloadParams {
    key: String,
}

/// GET /api/admin/download?key=event_writeup/123.pdf
#[get("/download")]
pub async fn download(
    ctx: ReqCtx,
    super_admin: SuperAdminJwtGuard,
    params: Query<DownloadParams>,
) -> UniResult<String> {
    let super_admin = super_admin.into_inner();

    let presigned = ctx
        .rustfs
        .get_object()
        .bucket("floatctf-private")
        .key(&params.key)
        .presigned(
            PresigningConfig::expires_in(Duration::from_secs(90))
                .map_err(|e| AppError::BadRequest(e.to_string()))?,
        )
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    let message = format!(
        "[ADMIN] {} downloading {}",
        super_admin.username, params.key
    );

    info!(message);

    ctx.log
        .add_log(
            "INFO",
            "FILES",
            "DOWNLOAD",
            &message,
            json!([]),
            None,
            Some(super_admin.id),
            Some(&ctx.req),
        )
        .await;

    let rustfs_endpoint_url = env::var("RUSTFS_ENDPOINT_URL").unwrap();
    let final_uri = presigned.uri().replace(
        &format!("{}/floatctf-private", rustfs_endpoint_url),
        "/private",
    );
    UniResponse::ok(final_uri.into()).into()
}
