//! AWD API handlers — admin, player, and internal endpoints.

pub mod admin;
pub mod auth;
pub mod dto;
pub mod gamebox_admin;
pub mod internal;
pub mod player;

use actix_web::web;

/// Register AWD admin routes under `/api/admin`.
///
/// Final paths: `/api/admin/events/{event_id}/awd/...` and `POST /api/admin/events/awd`.
pub fn admin_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(admin::create_awd_event)
        .service(admin::start_awd_event)
        .service(admin::pause_awd_event)
        .service(admin::resume_awd_event)
        .service(admin::finish_awd_event)
        .service(admin::ban_team)
        .service(admin::unban_team)
        .service(admin::adjust_score)
        .service(admin::admin_reset_gamebox)
        .service(admin::deploy_awd_event)
        .service(admin::run_precheck)
        .service(admin::get_prechecks)
        .service(admin::get_judge_batches)
        .service(admin::archive_event)
        .service(admin::rotate_tokens)
        .service(admin::update_network)
        .service(admin::get_event_scores)
        // GameBox 库 + 赛事 GameBox 管理（§46 术语：gamebox / revision / event_gamebox）
        .service(gamebox_admin::list_gamebox_library)
        .service(gamebox_admin::create_gamebox)
        .service(gamebox_admin::edit_gamebox_revision)
        .service(gamebox_admin::hide_gamebox)
        .service(gamebox_admin::list_event_gameboxes)
        .service(gamebox_admin::add_event_gamebox)
        .service(gamebox_admin::update_event_gamebox)
        .service(gamebox_admin::delete_event_gamebox);
}

/// Register AWD player routes under `/api`.
///
/// Final paths: `/api/events/{event_id}/awd/...`
pub fn player_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(player::get_my_gameboxes)
        .service(player::reset_my_gamebox)
        .service(player::submit_flag)
        .service(player::get_scores)
        .service(player::get_wireguard_config)
        .service(player::event_stream);
}

/// Register AWD internal routes (FlagServer / JudgeServer).
///
/// Final paths: `/internal/awd/events/...`
pub fn internal_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(internal::issue_flag)
        .service(internal::judge_callback)
        .service(internal::event_health);
}
