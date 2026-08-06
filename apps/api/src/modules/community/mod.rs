//! Community module — discussions, comments, and likes.

pub mod comment;
pub mod discussion;
pub mod like;

use actix_web::web::ServiceConfig;

/// Register player discussion routes under an existing `/discussions` scope.
///
/// Final paths: `/api/discussions/**`
pub fn configure_player_routes(cfg: &mut ServiceConfig) {
    cfg.service(discussion::player::get_discussions)
        .service(discussion::player::get_discussion)
        .service(discussion::player::create_discussion)
        .service(discussion::player::patch_discussion)
        .service(discussion::player::delete_discussion)
        .service(like::like_discussion)
        .service(like::unlike_discussion)
        .service(comment::player::get_discussion_comments)
        .service(comment::player::create_comment)
        .service(comment::player::patch_comment)
        .service(comment::player::delete_comment);
}

/// Register admin discussion routes under an existing `/discussions` scope.
///
/// Final paths: `/api/admin/discussions/**`
pub fn configure_admin_routes(cfg: &mut ServiceConfig) {
    cfg.service(discussion::admin::get_discussions)
        .service(discussion::admin::get_discussion)
        .service(discussion::admin::delete_discussions)
        .service(comment::admin::get_discussion_comments)
        .service(comment::admin::delete_comment);
}
