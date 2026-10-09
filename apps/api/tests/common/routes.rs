//! Complete HTTP route catalog for floatctf-api (mirrors service + admin config).
//!
//! # 为什么这是一份「维护型清单」而不是自动派生
//!
//! Actix 的 `ServiceConfig` **不提供运行时路由内省**，因此无法在测试里枚举
//! 已注册路由。本文件是**唯一**的路由清单真相源，由下面的守卫共同保护：
//!
//! 1. [`admin_route_count_is_pinned`]：admin 路由数量**精确**钉住（不是 `>= N`），
//!    新增/删除路由而忘记更新清单 → 直接失败。
//! 2. [`admin_routes_have_no_duplicates`]：`(method, path)` 不得重复。
//! 3. [`admin_route_domains_meet_minimums`]：按域最小数量（整块域消失会被发现）。
//! 4. [`high_risk_admin_routes_are_present`]：高危路由的**具名锚点**必须存在
//!    （terminal / database / docker / awd / awdp / dashboard）。
//! 5. [`admin_scope_literals_are_covered`]：从源码扫描 admin 注册代码里的
//!    `scope("...")` 字面量，要求每个字面量都出现在至少一条清单路径里——
//!    这是**源码派生**的检查，能抓到「新增了一个 admin 子域但没登记」。
//!
//! 局限（如实记录）：本清单不能自动证明「源码里每个 handler 都有对应条目」，
//! 因为 Actix 不做路由内省、且路径前缀由 `configure_*_routes` 手工拼装。
//! 上述 5 条守卫是当前可做到的、无脆弱字符串解析的最强组合。

use super::{Auth, Method, NIL, Route};

/// 把路径模板里的**任意** `{...}` 占位符替换为 NIL UUID。
///
/// 早期版本只替换 `{id}`，导致 `{event_id}` / `{user_id}` 之类的模板会把花括号
/// 原样发出去。现在统一处理。
///
/// 同时被 [`p`] 与 [`high_risk_admin_routes_are_present`] 复用：锚点断言必须对
/// 「模板」与「已解析路径」做同一套归一化，否则会拿 `{event_id}` 去比对已经被
/// 替换成 NIL 的目录路径。
pub fn normalize_template(template: &str) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        match rest[start..].find('}') {
            Some(offset) => {
                out.push_str(NIL);
                rest = &rest[start + offset + 1..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

fn p(template: &str) -> &'static str {
    Box::leak(normalize_template(template).into_boxed_str())
}

/// Every service-side route under `/api`.
pub fn service_routes() -> Vec<Route> {
    vec![
        Route {
            method: Method::Post,
            path: "/api/users/session",
            auth: Auth::None,
            json_body: true,
        },
        Route {
            method: Method::Post,
            path: "/api/users",
            auth: Auth::None,
            json_body: true,
        },
        Route {
            method: Method::Post,
            path: "/api/users/reset_password",
            auth: Auth::None,
            json_body: true,
        },
        Route {
            method: Method::Post,
            path: "/api/users/reset?token=invalid",
            auth: Auth::None,
            json_body: true,
        },
        Route {
            method: Method::Get,
            path: "/api/users/me",
            auth: Auth::UserRequired,
            json_body: false,
        },
        Route {
            method: Method::Patch,
            path: "/api/users/me",
            auth: Auth::UserRequired,
            json_body: true,
        },
        Route {
            method: Method::Get,
            path: "/api/weapons",
            auth: Auth::UserRequired,
            json_body: false,
        },
        Route {
            method: Method::Get,
            path: "/api/announcements",
            auth: Auth::UserRequired,
            json_body: false,
        },
        Route {
            method: Method::Get,
            path: "/api/discussions",
            auth: Auth::UserRequired,
            json_body: false,
        },
        Route {
            method: Method::Get,
            path: p("/api/discussions/{id}"),
            auth: Auth::UserRequired,
            json_body: false,
        },
        Route {
            method: Method::Post,
            path: "/api/discussions",
            auth: Auth::UserRequired,
            json_body: true,
        },
        Route {
            method: Method::Patch,
            path: p("/api/discussions/{id}"),
            auth: Auth::UserRequired,
            json_body: true,
        },
        Route {
            method: Method::Delete,
            path: p("/api/discussions/{id}"),
            auth: Auth::UserRequired,
            json_body: false,
        },
        Route {
            method: Method::Post,
            path: p("/api/discussions/{id}/like"),
            auth: Auth::UserRequired,
            json_body: false,
        },
        Route {
            method: Method::Delete,
            path: p("/api/discussions/{id}/like"),
            auth: Auth::UserRequired,
            json_body: false,
        },
        Route {
            method: Method::Get,
            path: p("/api/discussions/{id}/comments"),
            auth: Auth::UserRequired,
            json_body: false,
        },
        Route {
            method: Method::Post,
            path: p("/api/discussions/{id}/comments"),
            auth: Auth::UserRequired,
            json_body: true,
        },
        Route {
            method: Method::Patch,
            path: p("/api/discussions/{id}/comments/{id}"),
            auth: Auth::UserRequired,
            json_body: true,
        },
        Route {
            method: Method::Delete,
            path: p("/api/discussions/{id}/comments/{id}"),
            auth: Auth::UserRequired,
            json_body: false,
        },
        Route {
            method: Method::Post,
            path: "/api/uploads/image",
            auth: Auth::UserRequired,
            json_body: false,
        },
        Route {
            method: Method::Patch,
            path: "/api/uploads/avatar",
            auth: Auth::UserRequired,
            json_body: false,
        },
        Route {
            method: Method::Post,
            path: "/api/submit/flag",
            auth: Auth::UserRequired,
            json_body: true,
        },
        Route {
            method: Method::Post,
            path: "/api/submit/writeup",
            auth: Auth::UserRequired,
            json_body: false,
        },
        Route {
            method: Method::Get,
            path: p("/api/writeups/{id}"),
            auth: Auth::UserRequired,
            json_body: false,
        },
        Route {
            method: Method::Get,
            path: "/api/writeups",
            auth: Auth::UserRequired,
            json_body: false,
        },
        Route {
            method: Method::Get,
            path: "/api/challenges",
            auth: Auth::UserRequired,
            json_body: false,
        },
        Route {
            method: Method::Get,
            path: p("/api/challenges/{id}"),
            auth: Auth::UserRequired,
            json_body: false,
        },
        Route {
            method: Method::Get,
            path: p("/api/challenges/{id}/instance"),
            auth: Auth::UserRequired,
            json_body: false,
        },
        Route {
            method: Method::Post,
            path: p("/api/challenges/{id}/my_writeup"),
            auth: Auth::UserRequired,
            json_body: true,
        },
        Route {
            method: Method::Get,
            path: p("/api/challenges/{id}/my_writeup"),
            auth: Auth::UserRequired,
            json_body: false,
        },
        Route {
            method: Method::Get,
            path: p("/api/challenges/{id}/writeups"),
            auth: Auth::UserRequired,
            json_body: false,
        },
        Route {
            method: Method::Get,
            path: "/api/challenge_sets",
            auth: Auth::UserRequired,
            json_body: false,
        },
        Route {
            method: Method::Get,
            path: p("/api/challenge_sets/{id}"),
            auth: Auth::UserRequired,
            json_body: false,
        },
        Route {
            method: Method::Get,
            path: "/api/instances",
            auth: Auth::UserRequired,
            json_body: false,
        },
        Route {
            method: Method::Get,
            path: p("/api/instances/{id}"),
            auth: Auth::UserRequired,
            json_body: false,
        },
        Route {
            method: Method::Post,
            path: "/api/instances/launch",
            auth: Auth::UserRequired,
            json_body: true,
        },
        Route {
            method: Method::Delete,
            path: p("/api/instances/{id}"),
            auth: Auth::UserRequired,
            json_body: false,
        },
        Route {
            method: Method::Get,
            path: "/api/solves",
            auth: Auth::UserRequired,
            json_body: false,
        },
        Route {
            method: Method::Get,
            path: "/api/solves/top15users",
            auth: Auth::UserRequired,
            json_body: false,
        },
        Route {
            method: Method::Get,
            path: "/api/events",
            auth: Auth::UserRequired,
            json_body: false,
        },
        Route {
            method: Method::Get,
            path: p("/api/events/{id}"),
            auth: Auth::UserRequired,
            json_body: false,
        },
        Route {
            method: Method::Get,
            path: p("/api/events/{id}/challenges"),
            auth: Auth::UserRequired,
            json_body: false,
        },
        Route {
            method: Method::Get,
            path: p("/api/events/{id}/instances"),
            auth: Auth::UserRequired,
            json_body: false,
        },
        Route {
            method: Method::Get,
            path: p("/api/events/{id}/challenges/{id}/instance"),
            auth: Auth::UserRequired,
            json_body: false,
        },
        Route {
            method: Method::Get,
            path: p("/api/events/{id}/scoreboard"),
            auth: Auth::UserRequired,
            json_body: false,
        },
        Route {
            method: Method::Get,
            path: p("/api/events/{id}/announcements"),
            auth: Auth::UserRequired,
            json_body: false,
        },
        Route {
            method: Method::Get,
            path: p("/api/events/{id}/trend"),
            auth: Auth::UserRequired,
            json_body: false,
        },
        Route {
            method: Method::Get,
            path: p("/api/events/{id}/own_wp"),
            auth: Auth::UserRequired,
            json_body: false,
        },
        Route {
            method: Method::Post,
            path: p("/api/events/{id}/join"),
            auth: Auth::UserRequired,
            json_body: false,
        },
        Route {
            method: Method::Delete,
            path: p("/api/events/{id}/leave"),
            auth: Auth::UserRequired,
            json_body: false,
        },
        Route {
            method: Method::Post,
            path: p("/api/events/{id}/team"),
            auth: Auth::UserRequired,
            json_body: true,
        },
        Route {
            method: Method::Post,
            path: p("/api/events/{id}/team/{id}/join"),
            auth: Auth::UserRequired,
            json_body: false,
        },
        Route {
            method: Method::Delete,
            path: p("/api/events/{id}/team/{id}"),
            auth: Auth::UserRequired,
            json_body: false,
        },
        // ── AWD 选手端（/api/events/{event_id}/awd/...）──
        // 这 8 条此前完全不在目录里，所以 get_scores 漏守卫时契约测试抓不到。
        // 只登记 /events 前缀：顶层 /api 的重复注册已移除（否则每条都有第二份可达路径）。
        Route {
            method: Method::Get,
            path: p("/api/events/{id}/awd/gameboxes"),
            auth: Auth::UserRequired,
            json_body: false,
        },
        Route {
            method: Method::Post,
            path: p("/api/events/{id}/awd/gameboxes/{id}/reset"),
            auth: Auth::UserRequired,
            json_body: false,
        },
        Route {
            method: Method::Post,
            path: p("/api/events/{id}/awd/submissions"),
            auth: Auth::UserRequired,
            json_body: true,
        },
        Route {
            method: Method::Get,
            path: p("/api/events/{id}/awd/ssh-config"),
            auth: Auth::UserRequired,
            json_body: false,
        },
        Route {
            method: Method::Get,
            path: p("/api/events/{id}/awd/scores"),
            auth: Auth::UserRequired,
            json_body: false,
        },
        Route {
            method: Method::Get,
            path: p("/api/events/{id}/awd/wireguard/config"),
            auth: Auth::UserRequired,
            json_body: false,
        },
        Route {
            method: Method::Get,
            path: p("/api/events/{id}/awd/status"),
            auth: Auth::UserRequired,
            json_body: false,
        },
        Route {
            method: Method::Get,
            path: p("/api/events/{id}/awd/stream"),
            auth: Auth::UserRequired,
            json_body: false,
        },
    ]
}

/// Every admin-side route under `/api/admin`.
///
/// 覆盖面：identity / files / system / announcements / community / settings /
/// scheduled_tasks / logs / docker / terminal / weapons / challenges /
/// challenge_sets / events / AWD（平台级 + 赛事级）/ AWDP（赛事级）。
pub fn admin_routes() -> Vec<Route> {
    vec![
        // [identity]
        Route {
            method: Method::Post,
            path: "/api/admin/session",
            auth: Auth::None,
            json_body: true,
        },
        // [identity]
        Route {
            method: Method::Get,
            path: "/api/admin/users",
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [identity]
        Route {
            method: Method::Get,
            path: p("/api/admin/users/{user_id}"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [identity]
        Route {
            method: Method::Post,
            path: "/api/admin/users",
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [identity]
        Route {
            method: Method::Delete,
            path: "/api/admin/users",
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [identity]
        Route {
            method: Method::Patch,
            path: p("/api/admin/users/{user_id}"),
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [identity]
        Route {
            method: Method::Get,
            path: "/api/admin/super_admin",
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [identity]
        Route {
            method: Method::Get,
            path: p("/api/admin/super_admin/{super_user_id}"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [identity]
        Route {
            method: Method::Post,
            path: "/api/admin/super_admin",
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [identity]
        Route {
            method: Method::Post,
            path: p("/api/admin/super_admin/{super_user_id}"),
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [identity]
        Route {
            method: Method::Delete,
            path: "/api/admin/super_admin",
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [files]
        Route {
            method: Method::Get,
            path: "/api/admin/download?key=x",
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [system]
        Route {
            method: Method::Get,
            path: "/api/admin/system/monitor",
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [system]
        Route {
            method: Method::Get,
            path: "/api/admin/system/version",
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [system]
        Route {
            method: Method::Get,
            path: "/api/admin/dashboard/summary",
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [system]
        Route {
            method: Method::Post,
            path: "/api/admin/database/exec_sql",
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [announcements]
        Route {
            method: Method::Get,
            path: "/api/admin/announcements",
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [announcements]
        Route {
            method: Method::Post,
            path: "/api/admin/announcements",
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [announcements]
        Route {
            method: Method::Delete,
            path: "/api/admin/announcements",
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [announcements]
        Route {
            method: Method::Patch,
            path: p("/api/admin/announcements/{announcement_id}"),
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [community]
        Route {
            method: Method::Get,
            path: "/api/admin/discussions",
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [community]
        Route {
            method: Method::Get,
            path: p("/api/admin/discussions/{discussion_id}"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [community]
        Route {
            method: Method::Delete,
            path: "/api/admin/discussions",
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [community]
        Route {
            method: Method::Get,
            path: p("/api/admin/discussions/{discussion_id}/comments"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [community]
        Route {
            method: Method::Delete,
            path: p("/api/admin/discussions/{discussion_id}/comments/{comment_id}"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [platform]
        Route {
            method: Method::Get,
            path: "/api/admin/settings",
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [platform]
        Route {
            method: Method::Post,
            path: "/api/admin/settings",
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [platform]
        Route {
            method: Method::Delete,
            path: "/api/admin/settings",
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [platform]
        Route {
            method: Method::Patch,
            path: p("/api/admin/settings/{setting_id}"),
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [platform]
        Route {
            method: Method::Get,
            path: "/api/admin/scheduled_tasks",
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [platform]
        Route {
            method: Method::Get,
            path: p("/api/admin/scheduled_tasks/{task_id}"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [platform]
        Route {
            method: Method::Post,
            path: "/api/admin/scheduled_tasks",
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [platform]
        Route {
            method: Method::Post,
            path: p("/api/admin/scheduled_tasks/{task_id}/run"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [platform]
        Route {
            method: Method::Delete,
            path: "/api/admin/scheduled_tasks",
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [platform]
        Route {
            method: Method::Patch,
            path: p("/api/admin/scheduled_tasks/{task_id}"),
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [platform]
        Route {
            method: Method::Get,
            path: "/api/admin/logs",
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [platform]
        Route {
            method: Method::Get,
            path: p("/api/admin/logs/{log_id}"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [infra]
        Route {
            method: Method::Get,
            path: "/api/admin/docker/containers",
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [infra]
        Route {
            method: Method::Post,
            path: p("/api/admin/docker/containers/{container_id}/stop"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [infra]
        Route {
            method: Method::Post,
            path: p("/api/admin/docker/containers/{container_id}/start"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [infra]
        Route {
            method: Method::Delete,
            path: p("/api/admin/docker/containers/{container_id}"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [infra]
        Route {
            method: Method::Get,
            path: "/api/admin/docker/images",
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [infra]
        Route {
            method: Method::Delete,
            path: p("/api/admin/docker/images/{image_id}"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [infra]
        Route {
            method: Method::Get,
            path: "/api/admin/docker/networks",
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [infra]
        Route {
            method: Method::Post,
            path: "/api/admin/docker/networks",
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [infra]
        Route {
            method: Method::Delete,
            path: p("/api/admin/docker/networks/{network_id}"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [infra]
        Route {
            method: Method::Post,
            path: "/api/admin/terminal/session",
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [infra]
        Route {
            method: Method::Get,
            path: "/api/admin/terminal/ws",
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [content]
        Route {
            method: Method::Get,
            path: "/api/admin/weapons",
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [content]
        Route {
            method: Method::Post,
            path: "/api/admin/weapons",
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [content]
        Route {
            method: Method::Delete,
            path: "/api/admin/weapons",
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [content]
        Route {
            method: Method::Patch,
            path: p("/api/admin/weapons/{weapon_id}"),
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [content]
        Route {
            method: Method::Post,
            path: p("/api/admin/weapons/{weapon_id}/upload"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [content]
        Route {
            method: Method::Get,
            path: "/api/admin/challenges",
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [content]
        Route {
            method: Method::Get,
            path: p("/api/admin/challenges/{challenge_id}"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [content]
        Route {
            method: Method::Post,
            path: "/api/admin/challenges",
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [content]
        Route {
            method: Method::Post,
            path: "/api/admin/challenges/check",
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [content]
        Route {
            method: Method::Post,
            path: "/api/admin/challenges/import",
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [content]
        Route {
            method: Method::Post,
            path: "/api/admin/challenges/build",
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [content]
        Route {
            method: Method::Post,
            path: "/api/admin/challenges/scan",
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [content]
        Route {
            method: Method::Delete,
            path: "/api/admin/challenges",
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [content]
        Route {
            method: Method::Patch,
            path: p("/api/admin/challenges/{challenge_id}"),
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [content]
        Route {
            method: Method::Get,
            path: "/api/admin/challenge_sets",
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [content]
        Route {
            method: Method::Get,
            path: p("/api/admin/challenge_sets/{challenge_set_id}"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [content]
        Route {
            method: Method::Post,
            path: "/api/admin/challenge_sets",
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [content]
        Route {
            method: Method::Delete,
            path: "/api/admin/challenge_sets",
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [content]
        Route {
            method: Method::Patch,
            path: p("/api/admin/challenge_sets/{challenge_set_id}"),
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [content]
        Route {
            method: Method::Post,
            path: p("/api/admin/challenge_sets/{challenge_set_id}/challenges"),
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [content]
        Route {
            method: Method::Delete,
            path: p("/api/admin/challenge_sets/{challenge_set_id}/challenges"),
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [events]
        Route {
            method: Method::Get,
            path: "/api/admin/events",
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [events]
        Route {
            method: Method::Get,
            path: p("/api/admin/events/{event_id}"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [events]
        Route {
            method: Method::Post,
            path: "/api/admin/events",
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [events]
        Route {
            method: Method::Delete,
            path: "/api/admin/events",
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [events]
        Route {
            method: Method::Patch,
            path: p("/api/admin/events/{event_id}"),
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [events]
        Route {
            method: Method::Get,
            path: p("/api/admin/events/{event_id}/data"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [events]
        Route {
            method: Method::Get,
            path: p("/api/admin/events/{event_id}/report"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [events]
        Route {
            method: Method::Get,
            path: p("/api/admin/events/{event_id}/instances"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [events]
        Route {
            method: Method::Get,
            path: p("/api/admin/events/{event_id}/writeups"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [events]
        Route {
            method: Method::Get,
            path: p("/api/admin/events/{event_id}/logs"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [events]
        Route {
            method: Method::Get,
            path: p("/api/admin/events/{event_id}/users"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [events]
        Route {
            method: Method::Post,
            path: p("/api/admin/events/{event_id}/users"),
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [events]
        Route {
            method: Method::Delete,
            path: p("/api/admin/events/{event_id}/users"),
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [events]
        Route {
            method: Method::Post,
            path: p("/api/admin/events/{event_id}/users/{user_id}/banned"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [events]
        Route {
            method: Method::Post,
            path: p("/api/admin/events/{event_id}/users/{user_id}/unbanned"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [events]
        Route {
            method: Method::Get,
            path: p("/api/admin/events/{event_id}/teams"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [events]
        Route {
            method: Method::Get,
            path: p("/api/admin/events/{event_id}/teams/{team_id}"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [events]
        Route {
            method: Method::Post,
            path: p("/api/admin/events/{event_id}/teams"),
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [events]
        Route {
            method: Method::Delete,
            path: p("/api/admin/events/{event_id}/teams"),
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [events]
        Route {
            method: Method::Post,
            path: p("/api/admin/events/{event_id}/teams/{team_id}/users"),
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [events]
        Route {
            method: Method::Delete,
            path: p("/api/admin/events/{event_id}/teams/{team_id}/users"),
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [events]
        Route {
            method: Method::Post,
            path: p("/api/admin/events/{event_id}/teams/{team_id}/banned"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [events]
        Route {
            method: Method::Post,
            path: p("/api/admin/events/{event_id}/teams/{team_id}/unbanned"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [events]
        Route {
            method: Method::Get,
            path: p("/api/admin/events/{event_id}/challenges"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [events]
        Route {
            method: Method::Post,
            path: p("/api/admin/events/{event_id}/challenges"),
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [events]
        Route {
            method: Method::Patch,
            path: p("/api/admin/events/{event_id}/challenges"),
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [events]
        Route {
            method: Method::Delete,
            path: p("/api/admin/events/{event_id}/challenges"),
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [events]
        Route {
            method: Method::Post,
            path: p("/api/admin/events/{event_id}/challenges/hidden"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [events]
        Route {
            method: Method::Post,
            path: p("/api/admin/events/{event_id}/challenges/open"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [events]
        Route {
            method: Method::Get,
            path: p("/api/admin/events/{event_id}/announcements"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [events]
        Route {
            method: Method::Get,
            path: p("/api/admin/events/{event_id}/announcements/{announcement_id}"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [events]
        Route {
            method: Method::Post,
            path: p("/api/admin/events/{event_id}/announcements"),
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [events]
        Route {
            method: Method::Patch,
            path: p("/api/admin/events/{event_id}/announcements/{announcement_id}"),
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [events]
        Route {
            method: Method::Delete,
            path: p("/api/admin/events/{event_id}/announcements"),
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [awd]
        Route {
            method: Method::Get,
            path: "/api/admin/awd/network",
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [awd]
        Route {
            method: Method::Patch,
            path: "/api/admin/awd/network",
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [awd]
        Route {
            method: Method::Get,
            path: "/api/admin/awd/network/health",
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [awd]
        Route {
            method: Method::Get,
            path: "/api/admin/awd/network/allocations",
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [awd]
        Route {
            method: Method::Get,
            path: "/api/admin/awd/gameboxes",
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [awd]
        Route {
            method: Method::Post,
            path: "/api/admin/awd/gameboxes/import",
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [awd]
        Route {
            method: Method::Post,
            path: "/api/admin/awd/gameboxes/check",
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [awd]
        Route {
            method: Method::Post,
            path: "/api/admin/awd/gameboxes/build",
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [awd]
        Route {
            method: Method::Post,
            path: "/api/admin/awd/gameboxes/scan",
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [awd]
        Route {
            method: Method::Patch,
            path: p("/api/admin/awd/gameboxes/{gamebox_id}"),
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [awd]
        Route {
            method: Method::Post,
            path: p("/api/admin/awd/gameboxes/{gamebox_id}/hide"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [awd]
        Route {
            method: Method::Delete,
            path: "/api/admin/awd/gameboxes",
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [awd]
        Route {
            method: Method::Get,
            path: p("/api/admin/events/{event_id}/awd"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [awd]
        Route {
            method: Method::Post,
            path: "/api/admin/events/awd",
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [awd]
        Route {
            method: Method::Patch,
            path: p("/api/admin/events/{event_id}/awd"),
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [awd]
        Route {
            method: Method::Post,
            path: p("/api/admin/events/{event_id}/awd/start"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [awd]
        Route {
            method: Method::Post,
            path: p("/api/admin/events/{event_id}/awd/pause"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [awd]
        Route {
            method: Method::Post,
            path: p("/api/admin/events/{event_id}/awd/resume"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [awd]
        Route {
            method: Method::Post,
            path: p("/api/admin/events/{event_id}/awd/finish"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [awd]
        Route {
            method: Method::Post,
            path: p("/api/admin/events/{event_id}/awd/archive"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [awd]
        Route {
            method: Method::Post,
            path: p("/api/admin/events/{event_id}/awd/deploy"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [awd]
        Route {
            method: Method::Post,
            path: p("/api/admin/events/{event_id}/awd/precheck"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [awd]
        Route {
            method: Method::Get,
            path: p("/api/admin/events/{event_id}/awd/prechecks"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [awd]
        Route {
            method: Method::Get,
            path: p("/api/admin/events/{event_id}/awd/judge"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [awd]
        Route {
            method: Method::Get,
            path: p("/api/admin/events/{event_id}/awd/scores"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [awd]
        Route {
            method: Method::Post,
            path: p("/api/admin/events/{event_id}/awd/score/adjust"),
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [awd]
        Route {
            method: Method::Post,
            path: p("/api/admin/events/{event_id}/awd/teams/{team_id}/ban"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [awd]
        Route {
            method: Method::Delete,
            path: p("/api/admin/events/{event_id}/awd/teams/{team_id}/ban"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [awd]
        Route {
            method: Method::Post,
            path: p("/api/admin/events/{event_id}/awd/tokens/rotate"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [awd]
        Route {
            method: Method::Post,
            path: p("/api/admin/events/{event_id}/awd/gameboxes/{instance_id}/reset"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [awd]
        Route {
            method: Method::Get,
            path: p("/api/admin/events/{event_id}/awd/gameboxes"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [awd]
        Route {
            method: Method::Post,
            path: p("/api/admin/events/{event_id}/awd/gameboxes"),
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [awd]
        Route {
            method: Method::Patch,
            path: p("/api/admin/events/{event_id}/awd/gameboxes/{event_gamebox_id}"),
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [awd]
        Route {
            method: Method::Delete,
            path: p("/api/admin/events/{event_id}/awd/gameboxes/{event_gamebox_id}"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [awd]
        Route {
            method: Method::Get,
            path: p("/api/admin/events/{event_id}/awd/network"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [awd]
        Route {
            method: Method::Put,
            path: p("/api/admin/events/{event_id}/awd/network"),
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [awd]
        Route {
            method: Method::Post,
            path: p("/api/admin/events/{event_id}/awd/network/reallocate"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [awd]
        Route {
            method: Method::Get,
            path: p("/api/admin/events/{event_id}/awd/stream"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [awdp]
        Route {
            method: Method::Get,
            path: p("/api/admin/events/{event_id}/awdp"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [awdp]
        Route {
            method: Method::Patch,
            path: p("/api/admin/events/{event_id}/awdp"),
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [awdp]
        Route {
            method: Method::Post,
            path: p("/api/admin/events/{event_id}/awdp/start"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [awdp]
        Route {
            method: Method::Post,
            path: p("/api/admin/events/{event_id}/awdp/break-to-fix"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [awdp]
        Route {
            method: Method::Post,
            path: p("/api/admin/events/{event_id}/awdp/finish"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [awdp]
        Route {
            method: Method::Get,
            path: p("/api/admin/events/{event_id}/awdp/runs"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [awdp]
        Route {
            method: Method::Post,
            path: p("/api/admin/events/{event_id}/awdp/gameboxes"),
            auth: Auth::SuperAdminRequired,
            json_body: true,
        },
        // [awdp]
        Route {
            method: Method::Delete,
            path: p("/api/admin/events/{event_id}/awdp/gameboxes/{eg_id}"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [awdp]
        Route {
            method: Method::Get,
            path: p("/api/admin/events/{event_id}/awdp/gameboxes"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [awdp]
        Route {
            method: Method::Get,
            path: p("/api/admin/events/{event_id}/awdp/instances"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [awdp]
        Route {
            method: Method::Get,
            path: p("/api/admin/events/{event_id}/awdp/scores"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
        // [awdp]
        Route {
            method: Method::Get,
            path: p("/api/admin/events/{event_id}/awdp/data"),
            auth: Auth::SuperAdminRequired,
            json_body: false,
        },
    ]
}

/// admin 路由的**精确**数量。
///
/// **新增或删除 admin 路由时必须同步更新本常量与 [`admin_routes`]**。
/// 用「精确钉住」而不是 `>= N`：后者对**删除**完全无感（历史脚本曾用
/// `assert!(a.len() >= 60)`，而当时实际有 94 条且漏掉了整个 terminal / AWD / AWDP 面）。
pub const ADMIN_ROUTE_COUNT: usize = 153;

/// 各域条数：`(域, 条数, 最小要求)`。整块域消失会被 [`admin_route_domains_meet_minimums`] 抓到。
pub fn admin_route_domains() -> Vec<(&'static str, usize, usize)> {
    vec![
        ("announcements", 4, 4),
        ("awd", 38, 38),
        ("awdp", 12, 12),
        ("community", 5, 5),
        ("content", 21, 21),
        ("events", 34, 34),
        ("files", 1, 1),
        ("identity", 11, 11),
        ("infra", 11, 11),
        ("platform", 12, 12),
        ("system", 4, 4),
    ]
}

// ============================================================================
// 清单守卫（自校验，不需要运行中的 API）
// ============================================================================

/// 数量精确钉住：新增/删除必须显式更新 [`ADMIN_ROUTE_COUNT`]。
pub fn admin_route_count_is_pinned() -> Result<(), String> {
    let actual = admin_routes().len();
    if actual != ADMIN_ROUTE_COUNT {
        return Err(format!(
            "admin route catalog size drifted: actual={actual} pinned={ADMIN_ROUTE_COUNT}.              新增/删除 admin 路由时必须同步更新 ADMIN_ROUTE_COUNT 与 admin_routes()"
        ));
    }
    Ok(())
}

/// `(method, path)` 不得重复（重复会让「每条路由都拒绝匿名」的断言重复跑而无意义）。
pub fn admin_routes_have_no_duplicates() -> Result<(), String> {
    let mut seen = std::collections::HashSet::new();
    for route in admin_routes() {
        let key = (route.method, route.path);
        if !seen.insert(key) {
            return Err(format!(
                "duplicate admin route entry: {} {}",
                route.method.as_str(),
                route.path
            ));
        }
    }
    Ok(())
}

/// 按域最小数量：整块域被误删时失败。
pub fn admin_route_domains_meet_minimums() -> Result<(), String> {
    for (domain, count, minimum) in admin_route_domains() {
        if count < minimum {
            return Err(format!(
                "admin domain {domain} has {count} routes, minimum is {minimum}"
            ));
        }
    }
    Ok(())
}

/// 高危路由的**具名锚点**：这些端点是安全审阅重点，必须始终在清单里。
///
/// 它们曾是历史盲区（`terminal` / `dashboard` / AWD / AWDP 完全不在目录里，
/// 而目录只断言 `len >= 60`）。
pub const HIGH_RISK_ADMIN_ROUTES: &[(&str, &str)] = &[
    ("POST", "/api/admin/terminal/session"),
    ("GET", "/api/admin/terminal/ws"),
    ("POST", "/api/admin/database/exec_sql"),
    ("DELETE", "/api/admin/docker/containers/"),
    ("POST", "/api/admin/docker/containers/"),
    ("GET", "/api/admin/dashboard/summary"),
    ("GET", "/api/admin/awd/network"),
    ("PATCH", "/api/admin/awd/network"),
    ("POST", "/api/admin/events/{event_id}/awd/start"),
    ("POST", "/api/admin/events/{event_id}/awd/tokens/rotate"),
    ("POST", "/api/admin/events/{event_id}/awd/score/adjust"),
    ("POST", "/api/admin/events/{event_id}/awdp/start"),
    ("POST", "/api/admin/events/{event_id}/awdp/break-to-fix"),
    ("GET", "/api/admin/download?key=x"),
];

pub fn high_risk_admin_routes_are_present() -> Result<(), String> {
    for (method, fragment) in HIGH_RISK_ADMIN_ROUTES {
        // 目录里的路径已由 `p()` 把 `{...}` 归一化为 NIL，因此锚点也必须归一化后再比。
        let expected = normalize_template(fragment);
        let found = admin_routes()
            .iter()
            .any(|r| r.method.as_str() == *method && r.path.contains(&expected));
        if !found {
            return Err(format!(
                "high-risk admin route missing from catalog: {method} {fragment}"
            ));
        }
    }
    Ok(())
}

/// admin 注册源码文件 + 扫描范围。
///
/// `AdminFnsOnly`：文件同时注册选手端与管理端路由（如 `bootstrap/routes.rs` 的
/// `scope("submit")`、`platform/mod.rs` 的 `scope("uploads")`），因此只扫描函数名
/// **含 `admin`** 的函数体；`WholeFile` 用于文件本身就是管理端专用的场合。
enum ScanScope {
    WholeFile,
    AdminFnsOnly,
}

const ADMIN_REGISTRATION_FILES: &[(&str, ScanScope)] = &[
    ("src/bootstrap/routes.rs", ScanScope::AdminFnsOnly),
    ("src/modules/platform/mod.rs", ScanScope::AdminFnsOnly),
    ("src/modules/community/mod.rs", ScanScope::AdminFnsOnly),
    ("src/modules/identity/mod.rs", ScanScope::AdminFnsOnly),
    ("src/modules/challenge/mod.rs", ScanScope::AdminFnsOnly),
    (
        "src/modules/event/common/api/mod.rs",
        ScanScope::AdminFnsOnly,
    ),
    ("src/modules/event/awd/api/mod.rs", ScanScope::AdminFnsOnly),
    (
        "src/modules/event/awdp/api/admin.rs",
        ScanScope::AdminFnsOnly,
    ),
];

/// **源码派生**守卫：扫描 admin 注册代码里的 `scope("...")` 字面量，
/// 要求每个字面量都出现在至少一条 admin 清单路径中。
///
/// 这能抓到「新增了一个 admin 子域（scope）但忘记登记路由清单」。
/// 只扫描 `scope(` 调用（且在 admin 函数体内），不做路径前缀重建，
/// 因此不依赖脆弱的字符串拼接解析。
pub fn admin_scope_literals_are_covered() -> Result<(), String> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut missing = Vec::new();
    for (rel, scope) in ADMIN_REGISTRATION_FILES {
        let path = root.join(rel);
        let Ok(source) = std::fs::read_to_string(&path) else {
            // 文件不存在 = 仓库结构变了，必须显式失败而不是跳过
            return Err(format!(
                "admin registration file not found: {}",
                path.display()
            ));
        };
        let scanned = match scope {
            ScanScope::WholeFile => source.clone(),
            // 只保留「函数名含 admin」的函数体
            ScanScope::AdminFnsOnly => extract_admin_fn_bodies(&source).join("\n"),
        };
        for literal in extract_scope_literals(&scanned) {
            let covered = admin_routes()
                .iter()
                .any(|r| r.path.contains(&format!("/{literal}")) || r.path.contains(&literal));
            if !covered {
                missing.push(format!("{rel}: scope(\"{literal}\")"));
            }
        }
    }
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "admin scopes not represented in the route catalog:\n  {}",
            missing.join("\n  ")
        ))
    }
}

/// 提取源码里**函数名含 `admin`** 的 `fn ... { ... }` 函数体。
///
/// 用大括号配对而非正则：跳过字符串字面量与 `//` 行注释里的花括号，
/// 避免被 `format!("{...}")` 之类的内容带偏。
fn extract_admin_fn_bodies(source: &str) -> Vec<String> {
    let bytes = source.as_bytes();
    let mut bodies = Vec::new();
    let mut cursor = 0usize;

    while let Some(found) = source[cursor..].find("fn ") {
        let fn_start = cursor + found;
        cursor = fn_start + 3;

        // 函数名（支持 `pub fn` / `pub async fn` / `fn`）
        let name: String = source[cursor..]
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if !name.to_ascii_lowercase().contains("admin") {
            continue;
        }

        // 找函数体起始 `{`
        let Some(brace_offset) = source[cursor..].find('{') else {
            continue;
        };
        let body_start = cursor + brace_offset;
        let mut depth = 0i32;
        let mut i = body_start;
        let mut in_string = false;
        while i < bytes.len() {
            let c = bytes[i];
            if in_string {
                if c == b'"' && (i == 0 || bytes[i - 1] != b'\\') {
                    in_string = false;
                }
            } else if c == b'"' {
                in_string = true;
            } else if c == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                // 跳到行尾
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
                continue;
            } else if c == b'{' {
                depth += 1;
            } else if c == b'}' {
                depth -= 1;
                if depth == 0 {
                    bodies.push(source[body_start..=i].to_string());
                    cursor = i + 1;
                    break;
                }
            }
            i += 1;
        }
    }
    bodies
}

/// 从源码里提取 `scope("<literal>")` 的字符串字面量。
///
/// 只识别左花括号 `{` 开始的**路径参数** scope（如 `"/{event_id}/users"`）与
/// 普通静态 scope（如 `"/docker"`），忽略 `scope("")`（无路径段，无需覆盖）。
fn extract_scope_literals(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = source;
    // 归一化：去掉 `web::` 前缀的差异由 `scope(` 本身覆盖
    while let Some(idx) = rest.find("scope(") {
        rest = &rest[idx + "scope(".len()..];
        let trimmed = rest.trim_start();
        let Some(after_quote) = trimmed.strip_prefix('"') else {
            continue;
        };
        let Some(end) = after_quote.find('"') else {
            continue;
        };
        let literal = &after_quote[..end];
        // 模板占位符形态（"/{event_id}/users"）取第一段真实名字用于覆盖判断
        let name = literal
            .trim_start_matches('/')
            .split('/')
            .find(|seg| !seg.is_empty() && !seg.starts_with('{'))
            .unwrap_or("")
            .to_string();
        if !name.is_empty() && !out.contains(&name) {
            out.push(name);
        }
        rest = &after_quote[end + 1..];
    }
    out
}
