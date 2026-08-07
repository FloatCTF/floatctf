//! Platform / operational module — announcements, settings, files, and operations.

pub mod announcements;
pub mod files;
pub mod operations;
pub mod settings;

pub use announcements::AnnouncementsDto;
pub use files::generate_presigned_download_url;
pub use operations::{LogsDto, ScheduledTasksDto};
pub use settings::SettingsDto;

use actix_web::web::{ServiceConfig, scope};

/// Player-facing platform routes under `/api`.
pub fn configure_player_routes(cfg: &mut ServiceConfig) {
    cfg.service(scope("/announcements").service(announcements::get_player_announcements));

    cfg.service(
        scope("/uploads")
            .service(files::upload::upload_image)
            .service(files::upload::upload_avatar),
    );
}

/// Admin platform / operational routes under `/api/admin`.
pub fn configure_admin_routes(cfg: &mut ServiceConfig) {
    cfg.service(files::download::download);

    cfg.service(
        scope("system")
            .service(operations::system::get_sys_info)
            .service(operations::system::get_version),
    );

    cfg.service(scope("/database").service(operations::database::exec_sql));

    cfg.service(
        scope("/announcements")
            .service(announcements::get_announcements)
            .service(announcements::create_announcement)
            .service(announcements::delete_announcement)
            .service(announcements::patch_announcement),
    );

    cfg.service(
        scope("/settings")
            .service(settings::get_settings)
            .service(settings::create_setting)
            .service(settings::delete_setting)
            .service(settings::patch_setting),
    );

    cfg.service(
        scope("/instances")
            .service(operations::runtime_instances::get_instances)
            .service(operations::runtime_instances::get_instance),
    );

    cfg.service(
        scope("/scheduled_tasks")
            .service(operations::scheduled_tasks::create_scheduled_task)
            .service(operations::scheduled_tasks::run_scheduled_task)
            .service(operations::scheduled_tasks::delete_scheduled_task)
            .service(operations::scheduled_tasks::patch_scheduled_task)
            .service(operations::scheduled_tasks::get_scheduled_tasks)
            .service(operations::scheduled_tasks::get_scheduled_task),
    );

    cfg.service(
        scope("/logs")
            .service(operations::logs::get_logs)
            .service(operations::logs::get_log),
    );

    cfg.service(
        scope("/docker")
            .service(operations::docker::get_containers)
            .service(operations::docker::stop_container)
            .service(operations::docker::start_container)
            .service(operations::docker::delete_container)
            .service(operations::docker::get_images)
            .service(operations::docker::delete_image)
            .service(operations::docker::get_networks)
            .service(operations::docker::create_network)
            .service(operations::docker::delete_network),
    );

    cfg.service(scope("/terminal").service(operations::terminal::terminal_ws));
}
