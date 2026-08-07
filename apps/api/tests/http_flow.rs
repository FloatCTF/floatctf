//! Authenticated smoke / happy-path checks against a running API.
//!
//! Env:
//! - `FLOATCTF_API_BASE` (default http://127.0.0.1:8080)
//! - `FLOATCTF_TEST_USER` / `FLOATCTF_TEST_PASS` — optional user login
//! - `FLOATCTF_TEST_ADMIN` / `FLOATCTF_TEST_ADMIN_PASS` — optional super admin
//! - `FLOATCTF_API_REQUIRE=1` — fail if API down or credentials missing when used

mod common;

use common::{
    Auth, Method, NIL, Route, api_reachable, base_url, call, client, json_code, login_admin,
    login_user, routes::admin_routes, routes::service_routes,
};
use serde_json::Value;
use uuid::Uuid;

fn env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|s| !s.is_empty())
}

#[tokio::test]
async fn user_login_and_list_endpoints() {
    if !api_reachable().await {
        return;
    }
    let (Some(user), Some(pass)) = (env("FLOATCTF_TEST_USER"), env("FLOATCTF_TEST_PASS")) else {
        eprintln!(
            "skip flow: set FLOATCTF_TEST_USER + FLOATCTF_TEST_PASS for authenticated checks"
        );
        return;
    };
    let token = login_user(&user, &pass)
        .await
        .expect("user login should return JWT");

    // GET lists that must return UniResponse code 0
    for path in [
        "/api/users/me",
        "/api/events",
        "/api/challenges",
        "/api/challenge_sets",
        "/api/weapons",
        "/api/announcements",
        "/api/discussions",
        "/api/instances",
        "/api/solves",
        "/api/solves/top15users",
        "/api/writeups",
    ] {
        let route = Route {
            method: Method::Get,
            path,
            auth: Auth::UserRequired,
            json_body: false,
        };
        let resp = call(&route, Some(&token)).await;
        let (status, code, body) = json_code(resp).await;
        assert_eq!(status, 200, "{path} http status, body={body}");
        assert_eq!(code, Some(0), "{path} business code, body={body}");
    }
}

#[tokio::test]
async fn admin_login_and_list_endpoints() {
    if !api_reachable().await {
        return;
    }
    let (Some(user), Some(pass)) = (env("FLOATCTF_TEST_ADMIN"), env("FLOATCTF_TEST_ADMIN_PASS"))
    else {
        eprintln!("skip flow: set FLOATCTF_TEST_ADMIN + FLOATCTF_TEST_ADMIN_PASS for admin checks");
        return;
    };
    let token = login_admin(&user, &pass)
        .await
        .expect("admin login should return JWT");

    for path in [
        "/api/admin/events",
        "/api/admin/challenges",
        "/api/admin/users",
        "/api/admin/settings",
        "/api/admin/weapons",
        "/api/admin/announcements",
        "/api/admin/challenge_sets",
        "/api/admin/instances",
        "/api/admin/scheduled_tasks",
        "/api/admin/logs",
        "/api/admin/super_admin",
        "/api/admin/system/monitor",
        "/api/admin/docker/containers",
        "/api/admin/docker/images",
        "/api/admin/docker/networks",
    ] {
        let route = Route {
            method: Method::Get,
            path,
            auth: Auth::SuperAdminRequired,
            json_body: false,
        };
        let resp = call(&route, Some(&token)).await;
        let (status, code, body) = json_code(resp).await;
        assert_eq!(status, 200, "{path} http status, body={body}");
        assert_eq!(code, Some(0), "{path} business code, body={body}");
    }
}

#[tokio::test]
async fn user_token_cannot_call_admin() {
    if !api_reachable().await {
        return;
    }
    let (Some(user), Some(pass)) = (env("FLOATCTF_TEST_USER"), env("FLOATCTF_TEST_PASS")) else {
        return;
    };
    let token = match login_user(&user, &pass).await {
        Some(t) => t,
        None => return,
    };
    let route = Route {
        method: Method::Get,
        path: "/api/admin/events",
        auth: Auth::SuperAdminRequired,
        json_body: false,
    };
    let resp = call(&route, Some(&token)).await;
    let (status, code, _) = json_code(resp).await;
    assert!(
        status == 401 || status == 403 || code == Some(401) || code == Some(403),
        "user JWT must not access admin, status={status} code={code:?}"
    );
}

#[tokio::test]
async fn register_login_roundtrip_when_open() {
    if !api_reachable().await {
        return;
    }
    // Best-effort: create a unique user; skip if registration disabled/errors.
    let uname = format!("t_{}", &Uuid::new_v4().to_string()[..8]);
    let url = format!("{}/api/users", base_url());
    let resp = client()
        .post(&url)
        .json(&serde_json::json!({
            "username": uname,
            "nickname": uname,
            "password": "TestPass123!",
            "email": format!("{uname}@example.test"),
        }))
        .send()
        .await
        .expect("register");
    let body: Value = resp.json().await.unwrap_or(Value::Null);
    let code = body.get("code").and_then(|c| c.as_i64());
    if code != Some(0) {
        eprintln!("skip register roundtrip: create_user code={code:?} body={body}");
        return;
    }
    let token = login_user(&uname, "TestPass123!")
        .await
        .expect("login after register");
    let route = Route {
        method: Method::Get,
        path: "/api/users/me",
        auth: Auth::UserRequired,
        json_body: false,
    };
    let resp = call(&route, Some(&token)).await;
    let (status, code, body) = json_code(resp).await;
    assert_eq!(status, 200);
    assert_eq!(code, Some(0), "me after register: {body}");
}

#[tokio::test]
async fn eventmode_scoreboard_and_trend_with_auth() {
    if !api_reachable().await {
        return;
    }
    let (Some(user), Some(pass)) = (env("FLOATCTF_TEST_USER"), env("FLOATCTF_TEST_PASS")) else {
        return;
    };
    let token = match login_user(&user, &pass).await {
        Some(t) => t,
        None => return,
    };

    // List events; if any, hit scoreboard + trend (EventMode surface after refactor).
    let list = Route {
        method: Method::Get,
        path: "/api/events",
        auth: Auth::UserRequired,
        json_body: false,
    };
    let resp = call(&list, Some(&token)).await;
    let (_, code, body) = json_code(resp).await;
    if code != Some(0) {
        return;
    }
    let events = body
        .get("data")
        .and_then(|d| d.as_array())
        .cloned()
        .unwrap_or_default();
    if events.is_empty() {
        eprintln!("skip eventmode detail: no events in DB");
        return;
    }
    let id = events[0].get("id").and_then(|v| v.as_str()).unwrap_or(NIL);
    for suffix in ["/scoreboard", "/trend", "/announcements"] {
        let path = format!("/api/events/{id}{suffix}");
        let route = Route {
            method: Method::Get,
            path: Box::leak(path.into_boxed_str()),
            auth: Auth::UserRequired,
            json_body: false,
        };
        let resp = call(&route, Some(&token)).await;
        let (status, code, body) = json_code(resp).await;
        // May be 400 if event not started / type unsupported — must not 500.
        assert_ne!(status, 500, "{} server error: {body}", route.path);
        assert!(
            status == 200 || status == 400 || status == 404,
            "{} unexpected status={status} code={code:?} body={body}",
            route.path
        );
    }
}

#[tokio::test]
async fn all_catalog_get_routes_with_admin_do_not_500() {
    if !api_reachable().await {
        return;
    }
    let (Some(user), Some(pass)) = (env("FLOATCTF_TEST_ADMIN"), env("FLOATCTF_TEST_ADMIN_PASS"))
    else {
        return;
    };
    let token = match login_admin(&user, &pass).await {
        Some(t) => t,
        None => return,
    };
    let mut ok = 0usize;
    for route in admin_routes()
        .into_iter()
        .chain(service_routes())
        .filter(|r| matches!(r.method, Method::Get))
    {
        let bearer = match route.auth {
            Auth::None => None,
            Auth::UserRequired => {
                // admin token is SuperAdmin JWT — may fail UserJwtGuard; skip user GETs here
                continue;
            }
            Auth::SuperAdminRequired => Some(token.as_str()),
        };
        let resp = call(&route, bearer).await;
        let status = resp.status().as_u16();
        assert_ne!(status, 500, "GET {} returned 500", route.path);
        ok += 1;
    }
    assert!(ok > 20, "checked too few admin GETs: {ok}");
    eprintln!("admin GET smoke (no 500): {ok} routes");
}

#[tokio::test]
async fn instance_launch_destroy_roundtrip() {
    if !api_reachable().await {
        return;
    }
    let (Some(user), Some(pass)) = (env("FLOATCTF_TEST_USER"), env("FLOATCTF_TEST_PASS")) else {
        return;
    };
    let token = match login_user(&user, &pass).await {
        Some(t) => t,
        None => return,
    };

    // Best-effort cleanup: practice mode allows only one live instance per user,
    // and a previously failed destroy (e.g. the status-enum bug) leaves it running.
    let list_inst = Route {
        method: Method::Get,
        path: "/api/instances",
        auth: Auth::UserRequired,
        json_body: false,
    };
    if let (_, _, ibody) = json_code(call(&list_inst, Some(&token)).await).await {
        if let Some(arr) = ibody.get("data").and_then(|d| d.as_array()) {
            for inst in arr {
                let id = inst.get("id").and_then(|v| v.as_str());
                if let Some(id) = id {
                    let path = Box::leak(format!("/api/instances/{id}").into_boxed_str());
                    let r = Route {
                        method: Method::Delete,
                        path,
                        auth: Auth::UserRequired,
                        json_body: false,
                    };
                    let _ = call(&r, Some(&token)).await; // best effort
                }
            }
        }
    }

    let list = Route {
        method: Method::Get,
        path: "/api/challenges",
        auth: Auth::UserRequired,
        json_body: false,
    };
    let (_, code, body) = json_code(call(&list, Some(&token)).await).await;
    if code != Some(0) {
        eprintln!("skip instance roundtrip: challenges list code={code:?}");
        return;
    }
    let challenges = body
        .get("data")
        .and_then(|d| d.as_array())
        .cloned()
        .unwrap_or_default();
    if challenges.is_empty() {
        eprintln!("skip instance roundtrip: no challenges in DB");
        return;
    }
    let challenge_id = challenges[0]
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or(NIL);

    let launch_url = format!("{}/api/instances/launch", base_url());
    let resp = client()
        .post(&launch_url)
        .bearer_auth(&token)
        .json(&serde_json::json!({ "challenge_id": challenge_id }))
        .send()
        .await
        .expect("launch request");
    let (lstatus, lcode, lbody) = json_code(resp).await;
    assert_eq!(lstatus, 200, "launch http status, body={lbody}");
    assert_eq!(lcode, Some(0), "launch business code, body={lbody}");
    let instance_id = lbody
        .get("data")
        .and_then(|d| d.get("id"))
        .and_then(|v| v.as_str())
        .unwrap_or(NIL);
    assert_ne!(
        instance_id, NIL,
        "launch must return an instance id: {lbody}"
    );

    // Destroy: regression target — bug returned 400
    // "column \"status\" is of type instance_status but expression is of type text".
    let destroy_path = Box::leak(format!("/api/instances/{instance_id}").into_boxed_str());
    let route = Route {
        method: Method::Delete,
        path: destroy_path,
        auth: Auth::UserRequired,
        json_body: false,
    };
    let resp = call(&route, Some(&token)).await;
    let (dstatus, dcode, dbody) = json_code(resp).await;
    assert_eq!(
        dstatus, 200,
        "destroy http status should be 200, got {dstatus}: {dbody}"
    );
    assert_eq!(
        dcode,
        Some(0),
        "destroy business code should be 0, got {dcode:?}: {dbody}"
    );
}
