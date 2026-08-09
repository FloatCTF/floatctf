//! Central HTTP route aggregation — only place that composes module routes.

use actix_web::web::{self, ServiceConfig, scope};

/// Configure ALL routes on a `ServiceConfig` — single source of truth.
///
/// Used by the HTTP server and integration tests.
pub fn configure_all_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api")
            .configure(configure_player_routes)
            .service(web::scope("/admin").configure(configure_admin_routes))
            // AWD player: /api/events/{event_id}/awd/...
            // 挂进 common 的 /events scope（同组注册，避免同前缀 scope 被吞）
            .configure(crate::modules::event::awd_team::api::player_routes),
    );

    // ── AWD internal routes (FlagServer / JudgeServer) ──
    cfg.configure(crate::modules::event::awd_team::api::internal_routes);
}

/// Alias for integration tests.
pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    configure_all_routes(cfg);
}

/// Player-facing routes under `/api` (formerly `api::service`).
fn configure_player_routes(cfg: &mut ServiceConfig) {
    // POST /api/admin/session
    cfg.configure(crate::modules::identity::configure_session_routes);
    // Player identity: /api/users/*
    cfg.configure(crate::modules::identity::configure_player_routes);
    // GET /api/weapons
    cfg.service(scope("/weapons").configure(crate::modules::weapon::configure_player_routes));

    // GET /api/announcements + POST /api/uploads/*
    crate::modules::platform::configure_player_routes(cfg);

    // GET /api/discussions/**
    cfg.service(
        scope("/discussions").configure(crate::modules::community::configure_player_routes),
    );

    cfg.service(
        scope("/submit").configure(crate::modules::event::jeopardy::api::configure_submit_routes),
    );

    // Challenge catalog player (challenges, sets, writeups, solves)
    crate::modules::challenge::configure_player_routes(cfg);

    cfg.service(
        scope("/instances")
            .configure(crate::modules::event::jeopardy::api::configure_instance_routes),
    );
    // /api/events scope：common + AWD player 同组注册（AWD 另起 scope 会被前缀吞掉）
    cfg.service(
        scope("/events")
            .configure(crate::modules::event::common::api::configure_player_routes)
            .configure(crate::modules::event::awd_team::api::player_routes),
    );
}

/// Admin routes under `/api/admin` (formerly `api::admin`).
fn configure_admin_routes(cfg: &mut ServiceConfig) {
    // Operational / platform admin
    crate::modules::platform::configure_admin_routes(cfg);

    cfg.service(scope("/discussions").configure(crate::modules::community::configure_admin_routes));

    cfg.service(scope("weapons").configure(crate::modules::weapon::configure_admin_routes));

    // /api/admin/users + /api/admin/super_admin
    cfg.configure(crate::modules::identity::configure_admin_routes);

    // Challenge catalog admin
    crate::modules::challenge::configure_admin_routes(cfg);

    cfg.service(
        scope("/events")
            .configure(crate::modules::event::common::api::configure_admin_routes)
            .configure(crate::modules::event::common::api::configure_admin_nested_routes)
            // AWD 赛事级路由必须与 common 同 scope 注册，否则被吞（见 api/mod.rs 注释）
            .configure(crate::modules::event::awd_team::api::admin_events_routes),
    );

    // AWD 平台级路由（/api/admin/awd/*，无 events 前缀冲突）
    crate::modules::event::awd_team::api::admin_platform_routes(cfg);
}
