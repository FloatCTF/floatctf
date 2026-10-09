//! Auth contract: every protected API denies unauthenticated access.
//!
//! Requires a running API (see `tests/common`). Soft-skips if unreachable.
//!
//! `catalog_*` 测试**不需要**运行中的 API：它们自校验路由清单本身
//! （数量钉住 / 无重复 / 分域下限 / 高危锚点 / 源码 scope 覆盖）。

mod common;

use common::{
    Auth, api_reachable, assert_auth_denied, call, json_code,
    routes::{
        ADMIN_ROUTE_COUNT, admin_route_count_is_pinned, admin_route_domains_meet_minimums,
        admin_routes, admin_routes_have_no_duplicates, admin_scope_literals_are_covered,
        high_risk_admin_routes_are_present, service_routes,
    },
};

// ============================================================================
// 清单自校验（无 API 依赖）
// ============================================================================

#[test]
fn catalog_admin_route_count_is_pinned() {
    admin_route_count_is_pinned().expect("admin route catalog size must be pinned");
    assert_eq!(admin_routes().len(), ADMIN_ROUTE_COUNT);
}

#[test]
fn catalog_admin_routes_have_no_duplicates() {
    admin_routes_have_no_duplicates().expect("admin route catalog must not contain duplicates");
}

#[test]
fn catalog_admin_route_domains_meet_minimums() {
    admin_route_domains_meet_minimums().expect("every admin domain must keep its routes");
}

#[test]
fn catalog_high_risk_admin_routes_are_present() {
    high_risk_admin_routes_are_present()
        .expect("terminal / database / docker / awd / awdp / dashboard must stay in the catalog");
}

#[test]
fn catalog_admin_scope_literals_are_covered() {
    admin_scope_literals_are_covered()
        .expect("every admin scope literal in source must appear in the route catalog");
}

#[test]
fn catalog_service_routes_are_not_empty() {
    assert!(
        service_routes().len() >= 40,
        "service catalog too small: {}",
        service_routes().len()
    );
}

// ============================================================================
// 匿名 / 垃圾凭据拒绝（需要运行中的 API）
// ============================================================================

#[tokio::test]
async fn protected_service_routes_reject_anonymous() {
    if !api_reachable().await {
        return;
    }
    let mut checked = 0usize;
    for route in service_routes() {
        if !matches!(route.auth, Auth::UserRequired) {
            continue;
        }
        let resp = call(&route, None).await;
        let (status, code, _) = json_code(resp).await;
        assert_auth_denied(status, code, &route);
        checked += 1;
    }
    assert!(
        checked > 30,
        "expected many user-protected routes, got {checked}"
    );
    eprintln!("service auth contract: {checked} routes denied without token");
}

#[tokio::test]
async fn protected_admin_routes_reject_anonymous() {
    if !api_reachable().await {
        return;
    }
    let mut checked = 0usize;
    for route in admin_routes() {
        if !matches!(route.auth, Auth::SuperAdminRequired) {
            continue;
        }
        let resp = call(&route, None).await;
        let (status, code, _) = json_code(resp).await;
        assert_auth_denied(status, code, &route);
        checked += 1;
    }
    // 目录现在覆盖 153 条 admin 路由，其中绝大多数要求管理端凭据。
    // 用「与被断言的路由数量同量级」的下限，而不是历史那种远低于实际的 `>= 50`。
    assert!(
        checked >= 140,
        "expected nearly every admin route to be protected, got {checked}"
    );
    eprintln!("admin auth contract: {checked} routes denied without token");
}

#[tokio::test]
async fn protected_routes_reject_garbage_bearer() {
    if !api_reachable().await {
        return;
    }
    let sample: Vec<_> = service_routes()
        .into_iter()
        .filter(|r| matches!(r.auth, Auth::UserRequired))
        .take(10)
        .collect();
    for route in sample {
        let resp = call(&route, Some("not.a.jwt")).await;
        let (status, code, _) = json_code(resp).await;
        assert_auth_denied(status, code, &route);
    }
}
