//! Challenge catalog module — challenges, sets, writeups, build/import.
//!
//! Manages the challenge *catalog* itself (not event-challenge relationships).
//! `jeopardy_event_challenges` remains under the event module / admin API.

pub mod build;
pub mod catalog;
pub mod set;
pub mod writeup;

use actix_web::web::{self, ServiceConfig};

/// Player-facing challenge routes (under `/api`).
///
/// Scopes: `/challenges`, `/challenge_sets`, `/writeups`, `/solves`.
pub fn configure_player_routes(cfg: &mut ServiceConfig) {
    cfg.service(
        web::scope("/writeups")
            // GET /api/writeups/{writeup_id}
            .service(writeup::get_writeup)
            // GET /api/writeups
            .service(writeup::get_writeups),
    );

    cfg.service(
        web::scope("/challenges")
            // GET /api/challenges
            .service(catalog::player::get_challenges)
            // GET /api/challenges/{challenge_id}
            .service(catalog::player::get_challenge)
            // GET /api/challenges/{challenge_id}/instance
            .service(catalog::player::get_challenge_instance)
            // POST /api/challenges/{challenge_id}/my_writeup
            .service(writeup::create_challenge_writeup)
            // GET /api/challenges/{challenge_id}/my_writeup
            .service(writeup::get_challenge_writeup)
            // GET /api/challenges/{challenge_id}/writeups
            .service(writeup::get_challenge_writeups),
    );

    cfg.service(
        web::scope("/challenge_sets")
            // GET /api/challenge_sets
            .service(set::player::get_challenge_sets)
            // GET /api/challenge_sets/{challenge_set_id}
            .service(set::player::get_challenge_set),
    );

    cfg.service(
        web::scope("/solves")
            // GET /api/challenge_solves (mounted as /solves historically)
            .service(catalog::solves::get_solves)
            // GET /api/challenge_solves/top15users
            .service(catalog::solves::get_top_15_users),
    );
}

/// Admin challenge routes (under `/api/admin`).
///
/// Scopes: `/challenges`, `/challenge_sets`.
pub fn configure_admin_routes(cfg: &mut ServiceConfig) {
    cfg.service(
        web::scope("/challenges")
            // POST /api/admin/challenges/check
            .service(build::check_challenges)
            // POST /api/admin/challenges/import
            .service(build::web_import_challenge)
            // POST /api/admin/challenges/build
            .service(build::build_challenge)
            // POST /api/admin/challenges/scan
            .service(build::scan_challenges)
            // POST /api/admin/challenges
            .service(catalog::admin::create_challenge)
            // DELETE /api/admin/challenges
            .service(catalog::admin::delete_challenge)
            // PATCH /api/admin/challenges/{challenge_id}
            .service(catalog::admin::patch_challenge)
            // GET /api/admin/challenges
            .service(catalog::admin::get_challenges)
            // GET /api/admin/challenges/{challenge_id}
            .service(catalog::admin::get_challenge),
    );

    cfg.service(
        web::scope("/challenge_sets")
            // POST /api/admin/challenge_sets
            .service(set::admin::create_challenge_set)
            // DELETE /api/admin/challenge_sets
            .service(set::admin::delete_challenge_set)
            // GET /api/admin/challenge_sets
            .service(set::admin::get_challenge_sets)
            // GET /api/admin/challenge_sets/{challenge_set_id}
            .service(set::admin::get_challenge_set)
            // DELETE /api/admin/challenge_sets/{challenge_set_id}/challenges
            .service(set::admin::delete_challenge_from_set)
            // POST /api/admin/challenge_sets/{challenge_set_id}/challenges
            .service(set::admin::add_challenge_to_set)
            // PATCH /api/admin/challenge_sets/{challenge_set_id}
            .service(set::admin::patch_challenge_set),
    );
}
