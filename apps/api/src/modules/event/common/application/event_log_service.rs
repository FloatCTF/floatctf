//! 赛事日志（event_logs）写入服务。
//!
//! `event_logs` 是赛事维度（含虚拟训练赛事）的操作留痕表：谁在什么时间对
//! 某个赛事做了什么动作。目前主要由 AWDP 训练（Practice）路径写入，
//! 管理员在赛事管理页的 Logs Tab 查看用户练习过程。
//!
//! 写入失败不向上抛：留痕失败不应影响用户实际操作结果。

use sea_orm::{ActiveModelTrait, DatabaseConnection, EntityTrait, Set};
use serde_json::Value;
use uuid::Uuid;

use crate::entity::{event_logs, events};

/// 写入一条赛事日志。
///
/// - `level`：info / warn / error（默认 info）；
/// - `action`：动作名（event_logs.action 为 varchar(50)）；
/// - `details`：JSON 详情（run_id / gamebox_id / phase 等）。
///
/// event 行不存在时静默跳过（例如虚拟训练赛事被清理后）。
pub async fn insert_event_log(
    db: &DatabaseConnection,
    event_id: Uuid,
    user_id: Option<Uuid>,
    team_id: Option<Uuid>,
    level: &str,
    action: &str,
    details: Value,
) {
    let Ok(Some(event)) = events::Entity::find_by_id(event_id).one(db).await else {
        return;
    };
    let log = event_logs::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        user_id: Set(user_id),
        team_id: Set(team_id),
        ip_address: sea_orm::NotSet,
        level: Set(level.to_string()),
        action: Set(action.to_string()),
        details: Set(details),
        created_at: sea_orm::NotSet,
        family: Set(event.family),
        purpose: Set(event.purpose),
        participant_mode: Set(event.participant_mode),
    };
    let _ = log.insert(db).await;
}
