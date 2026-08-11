//! 赛事公共 HTTP 处理器。

pub mod dto;
pub mod event_announcement_dto;
pub mod event_challenge_dto;
pub mod event_log_dto;
pub mod event_team_dto;
pub mod event_team_member_dto;
pub mod event_user_dto;
pub mod writeup_dto;

pub use dto::EventsDto;
pub use event_announcement_dto::EventAnnouncementsDto;
pub use event_challenge_dto::EventChallengesDto;
pub use event_log_dto::EventLogsDto;
pub use event_team_dto::EventTeamsDto;
pub use event_team_member_dto::EventTeamMembersDto;
pub use event_user_dto::EventUsersDto;
pub use writeup_dto::EventWriteupDto;

pub mod admin;
pub mod event_announcements;
pub mod event_challenges;
pub mod event_logs;
pub mod event_teams;
pub mod event_users;
pub mod event_writeups;
pub mod player;

pub use admin::{
    CreateEventRequest, DataEventChallenge, DataEventChallengeSolve, DataPresent,
    PatchEventRequest, ReportTeam, ReportUser,
};
pub use player::{
    __get_scoreboard, __get_trend, CreateUserTeam, EventChallengeResult, EventInfo,
    EventInstanceResult, EventStatus, EventTeamMemberResult, EventTeamResult, ScoreboardItem,
    TrendItem, TrendPoint,
};

use actix_web::web::{self, ServiceConfig, scope};

/// Register player common event routes under an existing `/events` scope.
pub fn configure_player_routes(cfg: &mut ServiceConfig) {
    cfg.service(player::get_events)
        .service(player::get_event_challenges)
        .service(player::get_event)
        .service(player::get_event_capabilities)
        .service(player::get_event_instances)
        .service(player::get_event_challenge_instance)
        .service(player::get_scoreboard)
        .service(player::get_announcements)
        .service(player::get_trend)
        .service(player::get_own_wp)
        .service(player::join_event)
        .service(player::leave_event)
        .service(player::create_team)
        .service(player::join_team)
        .service(player::quit_team);
}

/// Register admin common event CRUD under an existing `/events` scope.
pub fn configure_admin_routes(cfg: &mut ServiceConfig) {
    cfg.service(admin::create_event)
        .service(admin::delete_event)
        .service(admin::patch_event)
        .service(admin::get_events)
        .service(admin::get_event)
        .service(admin::get_data)
        .service(admin::get_report);
}

/// Nested admin event sub-resources (users/teams/challenges/announcements/writeups/logs).
pub fn configure_admin_nested_routes(cfg: &mut ServiceConfig) {
    cfg.service(
        scope("/{event_id}/users")
            .service(event_users::add_user)
            .service(event_users::remove_user)
            .service(event_users::banned_user)
            .service(event_users::unbanned_user)
            .service(event_users::get_users),
    )
    .service(
        scope("/{event_id}/teams")
            .service(event_teams::add_team)
            .service(event_teams::remove_team)
            .service(event_teams::get_teams)
            .service(event_teams::get_team_members)
            .service(event_teams::add_user_to_team)
            .service(event_teams::remove_user_from_team)
            .service(event_teams::ban_team)
            .service(event_teams::unbanned_team),
    )
    .service(
        scope("/{event_id}/challenges")
            .service(event_challenges::add_challenge)
            .service(event_challenges::remove_challenge)
            .service(event_challenges::get_challenges)
            .service(event_challenges::hidden_challenges)
            .service(event_challenges::open_challenges),
    )
    .service(
        scope("/{event_id}/announcements")
            .service(event_announcements::add_event_announcement)
            .service(event_announcements::patch_event_announcement)
            .service(event_announcements::remove_event_announcement)
            .service(event_announcements::get_event_announcement)
            .service(event_announcements::list_event_announcements),
    )
    .service(scope("/{event_id}/writeups").service(event_writeups::get_all_event_writeups))
    .service(scope("/{event_id}/logs").service(event_logs::get_event_logs));
}
