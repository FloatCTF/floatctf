//! 平台文件下载处理。

use std::time::Duration;

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

/// 为对象生成预签名 GET URL（选手赛事 Writeup 等共用）。
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

/// 将 RustFS 私有桶（`floatctf-private`）预签名 GET URL 转换为 nginx `/private/`
/// 代理路径，供浏览器直接访问。
///
/// 输入（aws-sdk-s3 按 `[rustfs].endpoint_url` 生成的绝对签名 URL）：
///
/// ```text
/// http://127.0.0.1:9000/floatctf-private/writeups/xxx.pdf?X-Amz-Algorithm=...&X-Amz-Signature=...
/// ```
///
/// 输出（浏览器当前 origin 下可访问的相对路径）：
///
/// ```text
/// /private/writeups/xxx.pdf?X-Amz-Algorithm=...&X-Amz-Signature=...
/// ```
///
/// 转换规则：
/// - path 必须以 `/floatctf-private/` 开头，否则拒绝（防止把其它桶误转成私有代理路径）；
/// - object key 取 `/floatctf-private/` 之后的部分，保持原始 percent-encoding，不二次编解码；
/// - 完整 query string（`X-Amz-*` 签名参数等）原样保留，不重排、不丢参、不重新编码——
///   RustFS 验签依赖它逐字节一致。
pub fn private_presigned_proxy_path(signed_url: &str) -> Result<String> {
    let parsed =
        url::Url::parse(signed_url).map_err(|e| anyhow::anyhow!("invalid presigned URL: {e}"))?;
    let path = parsed.path();
    let object_key = path.strip_prefix("/floatctf-private/").ok_or_else(|| {
        anyhow::anyhow!("presigned URL path must start with /floatctf-private/, got: {path}")
    })?;

    let mut proxy = format!("/private/{object_key}");
    if let Some(query) = parsed.query() {
        proxy.push('?');
        proxy.push_str(query);
    }
    Ok(proxy)
}

/// 生成私有桶对象的浏览器代理下载路径（presign → `/private/` 转换一步到位）。
///
/// 调用方无需关心 RustFS host、bucket 路径改写或 nginx 映射。
pub async fn presign_private_download_url(
    rustfs: WebRustfs,
    key: &str,
    ttl_secs: u64,
) -> Result<String> {
    let signed = generate_presigned_download_url(rustfs, "floatctf-private", key, ttl_secs).await?;
    private_presigned_proxy_path(&signed)
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

    let proxy_url = presign_private_download_url(ctx.rustfs, &params.key, 90)
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

    UniResponse::ok(proxy_url.into()).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_flat_key_with_query() {
        let url = "http://127.0.0.1:9000/floatctf-private/a.pdf?X-Amz-A=1&X-Amz-B=2";
        assert_eq!(
            private_presigned_proxy_path(url).unwrap(),
            "/private/a.pdf?X-Amz-A=1&X-Amz-B=2"
        );
    }

    #[test]
    fn converts_nested_key_with_query() {
        let url = "http://127.0.0.1:9000/floatctf-private/team/123/report.zip?X-Amz-Sig=abc&X-Amz-Date=20260811";
        assert_eq!(
            private_presigned_proxy_path(url).unwrap(),
            "/private/team/123/report.zip?X-Amz-Sig=abc&X-Amz-Date=20260811"
        );
    }

    #[test]
    fn preserves_percent_encoded_object_key() {
        // 对象键含 percent-encoding：path 不应二次解码/编码
        let url = "http://127.0.0.1:9000/floatctf-private/team/a%20b/report%2Bv2.zip?X-Amz-Sig=abc";
        assert_eq!(
            private_presigned_proxy_path(url).unwrap(),
            "/private/team/a%20b/report%2Bv2.zip?X-Amz-Sig=abc"
        );
    }

    #[test]
    fn preserves_query_order_and_special_chars() {
        let url = "http://127.0.0.1:9000/floatctf-private/x.pdf?X-Amz-Credential=rustfsadmin%2F20260811%2Fcn-east-1%2Fs3%2Faws4_request&X-Amz-SignedHeaders=host&X-Amz-Signature=fccba83b7a45f4d5c3f7e08efb0e6396f97356878139ca58a69cb64f14d1a2c9";
        let out = private_presigned_proxy_path(url).unwrap();
        assert!(out.starts_with("/private/x.pdf?"));
        assert!(
            out.contains("X-Amz-Credential=rustfsadmin%2F20260811%2Fcn-east-1%2Fs3%2Faws4_request")
        );
        assert!(out.contains("&X-Amz-SignedHeaders=host"));
        assert!(out.contains(
            "&X-Amz-Signature=fccba83b7a45f4d5c3f7e08efb0e6396f97356878139ca58a69cb64f14d1a2c9"
        ));
        // 顺序保持：Credential 在 SignedHeaders 之前
        let cred_pos = out.find("X-Amz-Credential").unwrap();
        let sig_headers_pos = out.find("X-Amz-SignedHeaders").unwrap();
        let sig_pos = out.find("X-Amz-Signature=").unwrap();
        assert!(cred_pos < sig_headers_pos && sig_headers_pos < sig_pos);
    }

    #[test]
    fn rejects_public_bucket_path() {
        let url = "http://127.0.0.1:9000/floatctf-public/a.pdf?X-Amz-Sig=abc";
        assert!(private_presigned_proxy_path(url).is_err());
    }

    #[test]
    fn rejects_unrelated_path() {
        let url = "http://127.0.0.1:9000/other-bucket/a.pdf?X-Amz-Sig=abc";
        assert!(private_presigned_proxy_path(url).is_err());
    }

    #[test]
    fn rejects_malformed_url() {
        assert!(private_presigned_proxy_path("not a url").is_err());
    }

    #[test]
    fn accepts_no_query() {
        let url = "http://127.0.0.1:9000/floatctf-private/a.pdf";
        assert_eq!(private_presigned_proxy_path(url).unwrap(), "/private/a.pdf");
    }
}
