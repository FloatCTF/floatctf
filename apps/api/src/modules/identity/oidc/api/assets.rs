//! OP 页面的同源静态资源与本浏览器账号头像。
//!
//! - `GET /oauth/assets/float.png|ui.css|ui.js`：编译期嵌入的官方 Logo（与 `apps/web/public/float.png`
//!   同一文件）、样式与渐进增强脚本。`/oauth/*` 在所有部署形态（安装器 Caddy、10.66:8443 全量反代）中都路由到 API，
//!   因此不依赖 OP 所在 origin 是否还挂着 SPA 的 `/float.png`。
//! - `GET /oauth/accounts/{session_id}/avatar`：只对持有本浏览器账号容器（或旧版 SSO）Cookie、且该会话属于本浏览器时可见；
//!   只读取 RustFS 公开桶 `images/` 下平台上传的头像，魔数校验后以图片类型返回；失败返回 404，页面显示首字母。

use actix_web::{HttpRequest, HttpResponse, get, http::header, web};
use uuid::Uuid;

use super::protocol::same_origin_avatar;
use crate::infrastructure::WebDb;
use crate::modules::identity::oidc::{WebOidc, accounts};

const LOGO_PNG: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../web/public/float.png"
));
const UI_CSS: &str = include_str!("../assets/ui.css");
const UI_JS: &str = include_str!("../assets/ui.js");

/// 头像大小上限（平台上传接口本身允许更大，但选择页只展示小图；超限不展示）。
const MAX_AVATAR_BYTES: usize = 5 * 1024 * 1024;
const PUBLIC_BUCKET: &str = "floatctf-public";

fn asset(content_type: &'static str, body: &'static [u8]) -> HttpResponse {
    HttpResponse::Ok()
        .content_type(content_type)
        .insert_header((header::CACHE_CONTROL, "public, max-age=3600"))
        .insert_header(("X-Content-Type-Options", "nosniff"))
        .insert_header(("Cross-Origin-Resource-Policy", "same-origin"))
        .body(body)
}

/// GET /oauth/assets/{name}
#[get("/oauth/assets/{name}")]
pub async fn static_asset(path: web::Path<String>, oidc: Option<WebOidc>) -> HttpResponse {
    if oidc.is_none() {
        return super::not_enabled();
    }
    match path.as_str() {
        "float.png" => asset("image/png", LOGO_PNG),
        "ui.css" => asset("text/css; charset=utf-8", UI_CSS.as_bytes()),
        "ui.js" => asset("text/javascript; charset=utf-8", UI_JS.as_bytes()),
        _ => HttpResponse::NotFound().finish(),
    }
}

/// 由文件头识别允许展示的图片格式（不信任扩展名与存储的 Content-Type；SVG 一律拒绝）。
pub fn sniff_image(bytes: &[u8]) -> Option<&'static str> {
    match bytes {
        [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, ..] => Some("image/png"),
        [0xFF, 0xD8, 0xFF, ..] => Some("image/jpeg"),
        [b'G', b'I', b'F', b'8', b'7' | b'9', b'a', ..] => Some("image/gif"),
        [
            b'R',
            b'I',
            b'F',
            b'F',
            _,
            _,
            _,
            _,
            b'W',
            b'E',
            b'B',
            b'P',
            ..,
        ] => Some("image/webp"),
        _ => None,
    }
}

fn not_found() -> HttpResponse {
    HttpResponse::NotFound()
        .insert_header((header::CACHE_CONTROL, "no-store"))
        .finish()
}

/// GET /oauth/accounts/{session_id}/avatar
#[get("/oauth/accounts/{session_id}/avatar")]
pub async fn account_avatar(
    req: HttpRequest,
    path: web::Path<String>,
    oidc: Option<WebOidc>,
    db: WebDb,
    rustfs: Option<web::Data<aws_sdk_s3::Client>>,
) -> HttpResponse {
    let Some(oidc) = oidc else {
        return super::not_enabled();
    };
    let Ok(session_id) = Uuid::parse_str(&path) else {
        return not_found();
    };
    let browser = match accounts::load(&db, &oidc, &req).await {
        Ok(b) => b,
        Err(e) => {
            tracing::error!(error = %e, "avatar: browser lookup failed");
            return not_found();
        }
    };
    // 只能看本浏览器自己的账号头像：不能借此枚举全站用户。
    let Some((_, user)) = browser.find(session_id) else {
        return not_found();
    };
    let Some(key) = same_origin_avatar(user.avatar.as_deref()) else {
        return not_found();
    };
    let Some(rustfs) = rustfs else {
        return not_found();
    };
    let object = match rustfs
        .get_object()
        .bucket(PUBLIC_BUCKET)
        .key(&key)
        .send()
        .await
    {
        Ok(o) => o,
        Err(e) => {
            // 记录资源异常便于定位（例如对象被删除、RustFS 不可达），页面回退首字母。
            tracing::warn!(user_id = %user.id, key = %key, error = %e, "avatar object unavailable");
            return not_found();
        }
    };
    if object
        .content_length()
        .is_some_and(|len| len < 0 || len as usize > MAX_AVATAR_BYTES)
    {
        tracing::warn!(user_id = %user.id, key = %key, "avatar too large for chooser");
        return not_found();
    }
    let bytes = match object.body.collect().await {
        Ok(b) => b.into_bytes(),
        Err(e) => {
            tracing::warn!(user_id = %user.id, key = %key, error = %e, "avatar read failed");
            return not_found();
        }
    };
    if bytes.len() > MAX_AVATAR_BYTES {
        return not_found();
    }
    let Some(content_type) = sniff_image(&bytes) else {
        tracing::warn!(user_id = %user.id, key = %key, "avatar is not a supported image");
        return not_found();
    };
    let mut resp = HttpResponse::Ok()
        .content_type(content_type)
        .insert_header((header::CACHE_CONTROL, "private, max-age=300"))
        .insert_header(("X-Content-Type-Options", "nosniff"))
        .insert_header(("Content-Security-Policy", "default-src 'none'; sandbox"))
        .insert_header(("Cross-Origin-Resource-Policy", "same-origin"))
        .body(bytes);
    for cookie in &browser.cookies {
        let _ = resp.add_cookie(cookie);
    }
    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn official_logo_is_embedded_png() {
        assert_eq!(sniff_image(LOGO_PNG), Some("image/png"));
    }

    #[test]
    fn only_raster_images_are_served() {
        assert_eq!(
            sniff_image(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0]),
            Some("image/png")
        );
        assert_eq!(sniff_image(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("image/jpeg"));
        assert_eq!(sniff_image(b"GIF89a...."), Some("image/gif"));
        assert_eq!(
            sniff_image(b"RIFF\x00\x00\x00\x00WEBPVP8 "),
            Some("image/webp")
        );
        for rejected in [
            &b"<svg xmlns='http://www.w3.org/2000/svg'><script>alert(1)</script></svg>"[..],
            b"<!doctype html><script>alert(1)</script>",
            b"",
            b"RIFF\x00\x00\x00\x00WAVE",
        ] {
            assert_eq!(sniff_image(rejected), None);
        }
    }

    #[test]
    fn stylesheet_and_script_need_no_inline_privileges() {
        // 页面不再含内联 <style>/<script>：CSP 只放行同源资源。
        assert!(UI_JS.contains("data-fallback"));
        assert!(UI_CSS.contains("prefers-color-scheme: dark"));
        assert!(UI_CSS.contains("prefers-reduced-motion"));
    }
}
