//! AWD internal API authentication baseline (Phase 0 P0-3).
//!
//! Two layers:
//! 1. **Source-scan assertion（始终执行）**：`api/internal.rs` 中每个 `#[post]`/`#[get]`
//!    路由 handler 的第一个参数必须是 `auth: AwdInternalAuth` —— 防止新增 internal 端点
//!    忘记加服务身份认证。
//! 2. **Live-API 断言（soft-skip）**：若 `FLOATCTF_API_BASE` 可达，无 token 访问三个
//!    `/internal/awd/*` 端点必须 401。

mod common;

use common::{Method, Route, api_reachable, call, json_code};
use std::path::Path;

const INTERNAL_RS: &str = "src/modules/event/awd_team/api/internal.rs";

fn read_internal_source() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(INTERNAL_RS);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// 提取所有路由属性行 + 紧随其后的 handler 签名，断言首参为 AwdInternalAuth。
#[test]
fn every_internal_route_requires_internal_auth() {
    let src = read_internal_source();
    let lines: Vec<&str> = src.lines().collect();

    let mut checked = 0usize;
    let mut i = 0usize;
    while i < lines.len() {
        let line = lines[i].trim();
        // 路由属性：#[post("...")] / #[get("...")] / #[actix_web::post(...)] / #[actix_web::get(...)]
        let trimmed = line.trim_start_matches("#[actix_web::");
        let is_route = line.starts_with("#[post(")
            || line.starts_with("#[get(")
            || trimmed.starts_with("post(")
            || trimmed.starts_with("get(");
        if !is_route {
            i += 1;
            continue;
        }
        // 向后找 handler 签名行（可跳过 doc 注释）
        let mut j = i + 1;
        while j < lines.len() && (lines[j].trim().starts_with("///") || lines[j].trim().is_empty())
        {
            j += 1;
        }
        // 属性行可能跨行（如 #[post("/...")] 单行——本仓库全部单行）
        let sig = lines
            .get(j)
            .unwrap_or_else(|| panic!("no handler after route attr at line {}", i + 1));
        let sig = sig.trim();
        assert!(
            sig.starts_with("pub async fn"),
            "route at line {} must be followed by `pub async fn`, got: {sig}",
            i + 1
        );

        // 收集签名（可跨行）直到 `) ->`：首参必须是 AwdInternalAuth
        let mut body = String::new();
        let mut k = j;
        loop {
            body.push_str(lines[k]);
            body.push('\n');
            if lines[k].contains(") ->") || lines[k].contains(')') && lines[k].trim().ends_with(')')
            {
                break;
            }
            k += 1;
        }
        let start = body.find('(').expect("handler signature must have params");
        let body = &body[start + 1..];
        let params = body.split(')').next().unwrap_or("").trim();
        let first_param = params.split(',').next().map(|p| p.trim()).unwrap_or("");

        assert!(
            first_param.starts_with("auth: AwdInternalAuth")
                || first_param.starts_with("_auth: AwdInternalAuth"),
            "route at line {} (handler `{}`) first param must be `auth: AwdInternalAuth`, got `{first_param}`",
            i + 1,
            lines[j].trim()
        );
        checked += 1;
        i = k + 1;
    }

    assert!(
        checked >= 3,
        "expected at least 3 internal routes (issue_flag / judge_callback / event_health), found {checked}"
    );
}

/// 无 token 访问 internal 端点必须 401（live API soft-skip）。
#[tokio::test]
async fn internal_endpoints_reject_missing_token() {
    if !api_reachable().await {
        return;
    }
    // 固定假 event id（合法 UUID 格式，不存在的赛事 → auth 先于业务返回 401）
    const EP: &str = "/internal/awd/events/00000000-0000-0000-0000-000000000000";
    for (method, path) in [
        (
            Method::Post,
            "/internal/awd/events/00000000-0000-0000-0000-000000000000/flags/issue",
        ),
        (
            Method::Post,
            "/internal/awd/events/00000000-0000-0000-0000-000000000000/judge/callback",
        ),
        (
            Method::Get,
            "/internal/awd/events/00000000-0000-0000-0000-000000000000/health",
        ),
    ] {
        let _ = EP;
        let route = Route {
            method,
            path,
            auth: common::Auth::None,
            json_body: false,
        };
        let resp = call(&route, None).await;
        let (status, code, body) = json_code(resp).await;
        assert_eq!(
            status, 401,
            "internal endpoint {path} without token must be 401, got {status} body={body}"
        );
        assert_eq!(code, Some(401), "business code for {path}, body={body}");
    }
}
