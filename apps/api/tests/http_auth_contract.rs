//! Auth contract: every protected API denies unauthenticated access.
//!
//! Requires a running API (see `tests/common`). Soft-skips if unreachable.

mod common;

use common::{
    Auth, api_reachable, assert_auth_denied, call, json_code, routes::admin_routes,
    routes::service_routes,
};

#[tokio::test]
async fn catalog_sizes() {
    let s = service_routes();
    let a = admin_routes();
    assert!(s.len() >= 40, "service catalog too small: {}", s.len());
    assert!(a.len() >= 60, "admin catalog too small: {}", a.len());
}

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
    assert!(
        checked > 50,
        "expected many admin-protected routes, got {checked}"
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
