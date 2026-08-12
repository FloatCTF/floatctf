use crate::entity::{event_challenge_instance, event_instances};
use sea_orm::entity::prelude::{DateTimeWithTimeZone, Uuid};
use serde::Serialize;

/// 选手侧挑战实例 DTO（归一化后由 event_challenge_instance + instances 共同构建）。
#[derive(Debug, Serialize)]
pub struct InstancesDto {
    pub id: Uuid,
    /// 通用运行时状态（instances.runtime_state：pending/running/completed/failed）。
    pub status: String,
    pub flag: String,
    pub content: Option<String>,
    /// 关联挑战（AWDP 练习实例为 None，用 gamebox_id/run_id 标识）。
    pub challenge_id: Option<Uuid>,
    pub event_id: Uuid,
    pub team_id: Option<Uuid>,
    pub user_id: Uuid,
    /// 容器名（instances.container_name，兼容旧字段名 identifier）。
    pub identifier: String,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
    /// 自动销毁时间（instances.expires_at；completed 后可能为空）。
    pub destroy_at: Option<DateTimeWithTimeZone>,
    /// 题目名称（列表页展示用，非数据库列）。
    pub challenge_title: Option<String>,
    /// 赛事标题（列表页展示用，非数据库列）。
    pub event_title: Option<String>,
    /// 启动用户昵称（列表页展示用，非数据库列）。
    pub user_name: Option<String>,
    /// AWDP 练习 run id（列表页跳转训练场用，非数据库列）。
    pub run_id: Option<Uuid>,
    /// AWDP 练习 GameBox id（列表页跳转/筛选用，非数据库列）。
    pub gamebox_id: Option<Uuid>,
    /// AWDP 练习 GameBox 名（列表页展示用，非数据库列）。
    pub gamebox_title: Option<String>,
}

impl InstancesDto {
    /// 由题目领域行 + 通用运行时行构建（归一化实例 1:1）。
    pub fn from_pair(
        instance: &event_challenge_instance::Model,
        runtime: &event_instances::Model,
    ) -> Self {
        Self {
            id: instance.id,
            status: runtime.runtime_state.clone(),
            flag: instance.flag.clone(),
            content: instance.content.clone(),
            challenge_id: Some(instance.challenge_id),
            event_id: instance.event_id,
            team_id: instance.team_id,
            user_id: instance.user_id,
            identifier: runtime.container_name.clone(),
            created_at: runtime.created_at,
            updated_at: runtime.updated_at,
            destroy_at: runtime.expires_at,
            challenge_title: None,
            event_title: None,
            user_name: None,
            run_id: None,
            gamebox_id: None,
            gamebox_title: None,
        }
    }

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
