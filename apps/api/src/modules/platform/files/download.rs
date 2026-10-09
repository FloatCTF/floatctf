//! 平台文件下载处理。
//!
//! # 对象级授权（安全不变量 #14）
//!
//! `GET /api/admin/download?key=...` **不得**把任意 object key 直通对象存储。
//! 本模块的 [`resolve_private_artifact`] 负责把 key 解析为一个**平台已知资源**
//! （当前锚点是赛事 `events.id`）；无法解析的 key 一律 `404`。
//!
//! 可达 `floatctf-private` 桶且经本端点的 key **只有两族**（源码推导）：
//!
//! | 族 | 形态 | 产生位置 | 锚点 |
//! |---|---|---|---|
//! | A | `writeups/{file_name}` | 选手提交 Writeup（`jeopardy/api/submit.rs`） | `event_writeup.file_url` 行（DB 主键检索） |
//! | B | `writeups/{event_id}/{title}_{event_id}.zip` | 管理员导出报告（`event/common/application/admin_service.rs`） | key 内嵌 `event_id` + `events` 行存在性 |
//!
//! 其它私有桶内容（AWDP `source.tar.gz`，前缀 `gameboxes/{id}/awdp/{digest}/...`）
//! 走**另一条**从 DB 取键的路径，不经本端点。
//!
//! 之所以这不是「prefix-only 检查」：族 A 依赖**数据库检索**，族 B 依赖
//! **解析出的 event_id 再查 `events` 存在性**，且授权决策建立在解析结果上——
//! 攻击者可以构造一个指向他人赛事的 key，但随后仍要通过该赛事的读取权限。
//!
//! **禁止**为了让新前缀「能用」而把这里放宽成 `starts_with("writeups/")`。

use crate::api::extractor::auth::RequirePermission;
use crate::core::permission::guards::EventsReadGuard;
use std::time::Duration;

use actix_web::{get, web::Query};
use anyhow::Result;
use aws_sdk_s3::presigning::PresigningConfig;
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter};
use serde::Deserialize;
use serde_json::json;
use tracing::info;
use uuid::Uuid;

use crate::{
    api::{AppError, UniResponse, UniResult, prelude::*},
    entity::{event_writeup, events},
    infrastructure::WebRustfs,
};

/// 私有制品 key 的最大长度（防超长 key 打爆签名与日志）。
const MAX_PRIVATE_KEY_LEN: usize = 512;

/// 私有桶名（两族 key 都在这个桶里）。
const PRIVATE_BUCKET: &str = "floatctf-private";

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

/// 将 RustFS 私有桶（`floatctf-private`）预签名 GET URL 转换为 Caddy `/private/`
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
/// 调用方无需关心 RustFS host、bucket 路径改写或 Caddy 映射。
///
/// **注意**：本函数不做任何授权判断。调用方必须自行保证 `key` 来自可信来源
/// （例如某个 DB 行里已有的列），或在传入用户输入前先经
/// [`resolve_private_artifact`] 校验。
pub async fn presign_private_download_url(
    rustfs: WebRustfs,
    key: &str,
    ttl_secs: u64,
) -> Result<String> {
    let signed = generate_presigned_download_url(rustfs, PRIVATE_BUCKET, key, ttl_secs).await?;
    private_presigned_proxy_path(&signed)
}

/// key 的基础形状校验。任一不满足即拒绝。
///
/// 目的：在触碰存储之前挡掉明显非法的 key（空 / 超长 / 绝对路径 / 反斜杠 /
/// 目录上跳 / 空路径段）。这**不是**授权判断，只是第一道形状闸门。
fn private_key_shape_ok(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= MAX_PRIVATE_KEY_LEN
        && !key.starts_with('/')
        && !key.contains('\\')
        && !key.contains("..")
        && !key.contains("//")
}

/// 从「报告 zip」形态的 key 里解析出 `event_id`。
///
/// 形态（严格，由 `admin_service::export_writeup_report` 生成）：
/// `writeups/{event_id}/{file_name}`，其中第三段必须是合法 UUID 且 `file_name`
/// 为单个非空路径段（不含 `/`）。
fn report_key_event_id(key: &str) -> Option<Uuid> {
    let rest = key.strip_prefix("writeups/")?;
    let (event_segment, file_name) = rest.split_once('/')?;
    if file_name.is_empty() || file_name.contains('/') {
        return None;
    }
    // UUID 的规范文本长度固定 36；先做长度短路，避免为垃圾串付出解析成本。
    if event_segment.len() != 36 {
        return None;
    }
    Uuid::parse_str(event_segment).ok()
}

/// 把用户提供的私有制品 key 解析为平台已知资源（当前锚点是 `events.id`）。
///
/// 这是 `/api/admin/download` 的**对象级授权**核心：
///
/// 1. **族 A（DB 锚定，不可伪造）**：`event_writeup.file_url = key` 命中时，
///    返回该行的 `event_id`。判定依据是数据库检索，字符串前缀不参与授权。
/// 2. **族 B（服务器生成格式 + 存在性）**：key 匹配 `writeups/{uuid}/{file}`
///    且 `events` 表存在该 `uuid` 时返回它。攻击者可以构造指向他人赛事的 key，
///    但随后仍要通过该赛事的读取权限——这正是应有的门槛。
/// 3. 其余一律 `NotFound`（对外表现为 404，不泄露桶结构）。
pub async fn resolve_private_artifact<C: ConnectionTrait>(
    db: &C,
    key: &str,
) -> Result<Uuid, AppError> {
    if !private_key_shape_ok(key) {
        return Err(AppError::NotFound("未找到该私有制品".into()));
    }

    // ── 族 A：event_writeup.file_url 唯一锚定 ──
    if let Some(writeup) = event_writeup::Entity::find()
        .filter(event_writeup::Column::FileUrl.eq(key))
        .one(db)
        .await?
    {
        return Ok(writeup.event_id);
    }

    // ── 族 B：服务器生成的报告 zip key ──
    if let Some(event_id) = report_key_event_id(key) {
        if events::Entity::find_by_id(event_id)
            .one(db)
            .await?
            .is_some()
        {
            return Ok(event_id);
        }
    }

    Err(AppError::NotFound("未找到该私有制品".into()))
}

#[derive(Deserialize)]
struct DownloadParams {
    key: String,
}

/// GET /api/admin/download?key=event_writeup/123.pdf
///
/// 授权模型（v2 最终形态）：
/// 1. 静态权限：`events.read`，由 `RequirePermission<EventsReadGuard>` 承担
///    （Wave 0 先做对象级绑定、守卫暂用 `SuperAdminJwtGuard`；Wave 5 已切换）；
/// 2. **对象级绑定**：[`resolve_private_artifact`] 必须能把 key 解析为平台已知资源，
///    否则 404。二者缺一不可。
#[get("/download")]
pub async fn download(
    ctx: ReqCtx,
    super_admin: RequirePermission<EventsReadGuard>,
    params: Query<DownloadParams>,
) -> UniResult<String> {
    let super_admin = super_admin.into_auth();

    // 对象级授权：把 key 绑定到平台已知资源（event）。无法绑定 → 404。
    let event_id = resolve_private_artifact(ctx.db.get_ref(), &params.key).await?;

    let proxy_url = presign_private_download_url(ctx.rustfs, &params.key, 90)
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    let message = format!(
        "[ADMIN] {} downloading {} (event {})",
        super_admin.username(),
        params.key,
        event_id
    );

    info!(message);

    ctx.log
        .add_log(
            "INFO",
            "FILES",
            "DOWNLOAD",
            &message,
            json!({"event_id": event_id}),
            Some(super_admin.id()),
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

    // ── 对象级授权：key 形状闸门 ──

    #[test]
    fn key_shape_accepts_normal_keys() {
        assert!(private_key_shape_ok("writeups/a.pdf"));
        assert!(private_key_shape_ok(
            "writeups/00000000-0000-0000-0000-000000000001/report.zip"
        ));
        // 路径里出现单个点（扩展名）必须仍然允许
        assert!(private_key_shape_ok("writeups/team.report.pdf"));
    }

    #[test]
    fn key_shape_rejects_empty_and_oversized() {
        assert!(!private_key_shape_ok(""));
        assert!(!private_key_shape_ok(&"a".repeat(MAX_PRIVATE_KEY_LEN + 1)));
    }

    #[test]
    fn key_shape_rejects_absolute_and_backslash() {
        assert!(!private_key_shape_ok("/etc/passwd"));
        assert!(!private_key_shape_ok("writeups\\evil.pdf"));
    }

    #[test]
    fn key_shape_rejects_traversal_forms() {
        assert!(!private_key_shape_ok("writeups/../secret.pdf"));
        assert!(!private_key_shape_ok("../writeups/a.pdf"));
        assert!(!private_key_shape_ok("writeups/..%2fsecret.pdf"));
        assert!(!private_key_shape_ok("writeups//a.pdf"));
    }

    // ── 对象级授权：族 B 解析 ──

    #[test]
    fn report_key_resolves_event_id() {
        let ev = "00000000-0000-0000-0000-000000000001";
        let key = format!("writeups/{ev}/CTF_2026_{ev}.zip");
        assert_eq!(
            report_key_event_id(&key),
            Some(Uuid::parse_str(ev).unwrap())
        );
    }

    #[test]
    fn report_key_rejects_non_uuid_segment() {
        assert_eq!(report_key_event_id("writeups/not-a-uuid/report.zip"), None);
        // 长度对但不是 UUID
        assert_eq!(
            report_key_event_id("writeups/zzzzzzzz-zzzz-zzzz-zzzz-zzzzzzzzzzzz/r.zip"),
            None
        );
    }

    #[test]
    fn report_key_rejects_wrong_shape() {
        // 缺文件名
        assert_eq!(
            report_key_event_id("writeups/00000000-0000-0000-0000-000000000001"),
            None
        );
        // 多级路径
        assert_eq!(
            report_key_event_id("writeups/00000000-0000-0000-0000-000000000001/a/b.zip"),
            None
        );
        // 非 writeups 前缀
        assert_eq!(
            report_key_event_id("gameboxes/00000000-0000-0000-0000-000000000001/a.zip"),
            None
        );
        // 空文件段
        assert_eq!(
            report_key_event_id("writeups/00000000-0000-0000-0000-000000000001/"),
            None
        );
    }

    /// 回归：AWDP `source.tar.gz` 前缀**不得**被解析为合法制品。
    #[test]
    fn awdp_source_prefix_is_not_resolvable() {
        assert_eq!(
            report_key_event_id(
                "gameboxes/00000000-0000-0000-0000-000000000001/awdp/deadbeef/source.tar.gz"
            ),
            None
        );
    }
}
