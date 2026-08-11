//! 集中 HTTP 路由聚合——唯一组装各模块路由的位置。

use actix_web::web::{self, ServiceConfig, scope};

/// 在 `ServiceConfig` 上配置全部路由——路由组装的唯一权威位置。
///
/// 供 HTTP 服务与集成测试共用。
pub fn configure_all_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api")
            .configure(configure_player_routes)
            .service(web::scope("/admin").configure(configure_admin_routes))
            // AWD player: 实际匹配路径在 /events scope 内注册（见 configure_player_routes）；
            // 顶层注册仅为历史兼容（无匹配路径，保留以免破坏既有顺序假设）。
            .configure(crate::modules::event::awd::api::player_routes),
    );

    // ── AWD internal routes (FlagServer / JudgeServer) ──
    cfg.configure(crate::modules::event::awd::api::internal_routes);
}

/// 集成测试用别名。
pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    configure_all_routes(cfg);
}

/// 选手侧路由，挂在 `/api` 下（原 `api::service`）。
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
    // /api/events scope：common + AWD/AWDP player 同组注册（另起 scope 会被前缀吞掉）
    cfg.service(
        scope("/events")
            .configure(crate::modules::event::common::api::configure_player_routes)
            .configure(crate::modules::event::awd::api::player_routes)
            .configure(crate::modules::event::awdp::api::player_routes),
    );
}

/// 管理端路由，挂在 `/api/admin` 下（原 `api::admin`）。
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
            .configure(crate::modules::event::awd::api::admin_events_routes)
            // AWDP 赛事级路由同组注册。
            .configure(crate::modules::event::awdp::api::admin_events_routes),
    );

    // AWD 平台级路由（/api/admin/awd/*，无 events 前缀冲突）
    crate::modules::event::awd::api::admin_platform_routes(cfg);
}
