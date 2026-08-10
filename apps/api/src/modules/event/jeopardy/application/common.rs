//! Shared instance launch/destroy helpers for Jeopardy modes.

pub use anyhow::{Context, Result, anyhow};

use crate::{
    entity::{instances, users},
    infrastructure::settings::get_setting,
    infrastructure::{WebDb, WebDocker},
    modules::event::jeopardy::application::instance_service::InstanceService,
};
use uuid::Uuid;

/// InstanceLifecycle: launch / destroy / scheduled expire live here (fcmc is docker adapter).
///
/// Launch and destroy both go through `InstanceService`（单版本：直接使用 Challenge 当前版本）。

pub async fn launch_instance(
    db: &WebDb,
    docker: &WebDocker,
    challenge_id: Uuid,
    identifier: String,
    user_id: Uuid,
    r#ref: String,
    flag_prefix: Option<String>,
) -> anyhow::Result<instances::Model> {
    let service = InstanceService::with_docker(db.get_ref().clone(), docker.get_ref().clone());
    service
        .launch(challenge_id, identifier, user_id, r#ref, flag_prefix)
        .await
}

/// Destroy a user-owned running instance via `InstanceService`.
///
/// Order is always: stop/remove runtime first, then mark Completed.
/// Docker failures leave the row as Failed for scheduler retry (no silent DB-first delete).
/// Missing / already-completed instances are a no-op (HTTP DELETE + auto-destroy).
pub async fn destroy_instance(
    db: &WebDb,
    docker: &WebDocker,
    id: Uuid,
    user: &users::Model,
) -> Result<()> {
    let service = InstanceService::with_docker(db.get_ref().clone(), docker.get_ref().clone());
    service.destroy_owned(id, user.id).await?;
    Ok(())
}

pub use crate::modules::event::jeopardy::domain::scoring::calculate_next_dynamic_score;

pub fn get_uuid_prefix(uuid: &Uuid) -> String {
    let uuid_str = uuid.to_string();
    uuid_str.split('-').next().unwrap_or("").to_string()
}

pub async fn gen_flag(db: &WebDb, flag_prefix: Option<String>) -> String {
    let unique_value = Uuid::new_v4();

    let prefix = match flag_prefix {
        Some(prefix) => prefix,
        None => get_setting(db, "FLAG_PREFIX")
            .await
            .unwrap_or("flag".into()),
    };

    format!("{}{{{}}}", prefix, unique_value)
}
