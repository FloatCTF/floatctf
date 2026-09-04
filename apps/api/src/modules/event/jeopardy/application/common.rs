//! Jeopardy 实例启动/销毁共享辅助。

pub use anyhow::{Context, Result, anyhow};

use crate::{
    entity::{event_challenge_instance, users},
    infrastructure::settings::get_setting,
    infrastructure::{WebDb, WebDocker},
    modules::event::jeopardy::application::instance_service::InstanceService,
};
use uuid::Uuid;

/// 实例生命周期：启动 / 销毁 / 定时过期（`fcmc` 为 Docker 适配层）。
///
/// 启动与销毁均经 `InstanceService`（单版本模型：直接使用 Challenge 当前版本字段）。

pub async fn launch_instance(
    db: &WebDb,
    docker: &WebDocker,
    event_id: Uuid,
    challenge_id: Uuid,
    identifier: String,
    user_id: Uuid,
    team_id: Option<Uuid>,
    flag_prefix: Option<String>,
) -> anyhow::Result<event_challenge_instance::Model> {
    let service = InstanceService::with_docker(db.get_ref().clone(), docker.get_ref().clone());
    service
        .launch(
            event_id,
            challenge_id,
            identifier,
            user_id,
            team_id,
            flag_prefix,
        )
        .await
}

/// 经 `InstanceService` 销毁用户拥有的运行中实例。
///
/// 顺序固定：先停止/移除运行时，再将行标为 Completed。
/// Docker 失败时行记为 Failed 供调度重试（禁止静默先删库）。
/// 实例不存在或已完成视为幂等成功（HTTP DELETE 与自动销毁共用）。
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
