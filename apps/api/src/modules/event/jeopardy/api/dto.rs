use crate::entity::{challenge_instances, sea_orm_active_enums::InstanceStatus};
use sea_orm::entity::prelude::{DateTimeWithTimeZone, Uuid};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct InstancesDto {
    pub id: Uuid,
    pub status: InstanceStatus,
    pub flag: String,
    pub content: Option<String>,
    pub challenge_id: Uuid,
    pub event_id: Uuid,
    pub team_id: Option<Uuid>,
    pub user_id: Uuid,
    pub identifier: String,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
    pub destroy_at: DateTimeWithTimeZone,
    /// 题目名称（列表页展示用，非数据库列）。
    pub challenge_title: Option<String>,
    /// 赛事标题（列表页展示用，非数据库列）。
    pub event_title: Option<String>,
    /// 启动用户昵称（列表页展示用，非数据库列）。
    pub user_name: Option<String>,
}

impl From<challenge_instances::Model> for InstancesDto {
    fn from(m: challenge_instances::Model) -> Self {
        Self {
            id: m.id,
            status: m.status,
            flag: m.flag,
            content: m.content,
            challenge_id: m.challenge_id,
            event_id: m.event_id,
            team_id: m.team_id,
            user_id: m.user_id,
            identifier: m.identifier,
            created_at: m.created_at,
            updated_at: m.updated_at,
            destroy_at: m.destroy_at,
            challenge_title: None,
            event_title: None,
            user_name: None,
        }
    }
}

impl InstancesDto {
    /// 填充列表页展示名称字段（题目/赛事/用户）。
    pub fn with_names(
        mut self,
        challenge_title: Option<String>,
        event_title: Option<String>,
        user_name: Option<String>,
    ) -> Self {
        self.challenge_title = challenge_title;
        self.event_title = event_title;
        self.user_name = user_name;
        self
    }
}
