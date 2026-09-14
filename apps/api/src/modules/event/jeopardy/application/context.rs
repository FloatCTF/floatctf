//! 请求级 Jeopardy 赛事上下文（替代历史 strategies/event EventContext）。

use std::sync::Arc;

use anyhow::{Context as _, Result, anyhow};
use chrono::Utc;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    core::AppConfig,
    entity::{event_challenge_instance, event_instances, event_teams, event_users, events, users},
    infrastructure::{WebDb, WebDocker},
    modules::event::common::domain::time_state::event_time_status_of,
};

/// 单次请求内 Jeopardy 操作上下文。
#[derive(Debug)]
pub struct EventContext {
    pub db: WebDb,
    /// 启动/销毁/提交路径需要；纯读路径可为占位。
    pub docker: WebDocker,
    pub event: events::Model,
    pub user: users::Model,
    pub team: Option<event_teams::Model>,
    /// 进程静态配置；启动路径用于实例并发上限等。
    pub config: Option<Arc<AppConfig>>,
}

#[derive(Debug)]
pub struct EventContextBuilder {
    db: Option<WebDb>,
    docker: Option<WebDocker>,
    event: Option<events::Model>,
    user: Option<users::Model>,
    team: Option<event_teams::Model>,
    /// 为 true 时按 user+event 从 `event_team_members` 解析战队。
    resolve_team: bool,
    config: Option<Arc<AppConfig>>,
}

impl EventContextBuilder {
    pub fn new() -> Self {
        Self {
            db: None,
            docker: None,
            event: None,
            user: None,
            team: None,
            resolve_team: false,
            config: None,
        }
    }

    pub fn db(mut self, db: WebDb) -> Self {
        self.db = Some(db);
        self
    }

    pub fn docker(mut self, docker: WebDocker) -> Self {
        self.docker = Some(docker);
        self
    }

    /// 必填：调用方须显式加载赛事（练习经 `require_practice_jeopardy_event`，
    /// 正式赛经路由 `event_id`）。
    pub fn event(mut self, event: events::Model) -> Self {
        self.event = Some(event);
        self
    }

    pub fn user(mut self, user: users::Model) -> Self {
        self.user = Some(user);
        self
    }

    pub fn team(mut self, team: event_teams::Model) -> Self {
        self.team = Some(team);
        self
    }

    /// 自动加载该用户在本赛事中的战队成员关系（若存在）。
    pub fn resolve_team(mut self) -> Self {
        self.resolve_team = true;
        self
    }

    /// 挂载进程静态配置（启动路径的实例并发上限等需要）。
    pub fn config(mut self, config: Arc<AppConfig>) -> Self {
        self.config = Some(config);
        self
    }

    /// 构造 [`EventContext`]。启动/销毁/提交需要 Docker；
    /// 纯读处理器宜直接使用仅依赖 `db` + `event` 的用例函数。
    ///
    /// 赛事必填——本构建器不会在缺省 event 时自动回落系统练习赛。
    pub async fn build(self) -> Result<EventContext> {
        let db = self.db.clone().context("db is required")?;
        let user = self.user.context("user is required")?;
        let event = self.event.context(
            "event is required (resolve Practice via require_practice_jeopardy_event first)",
        )?;

        let team = if let Some(t) = self.team {
            Some(t)
        } else if self.resolve_team {
            if let Some(m) = crate::entity::event_team_members::Entity::find()
                .filter(crate::entity::event_team_members::Column::EventId.eq(event.id))
                .filter(crate::entity::event_team_members::Column::UserId.eq(user.id))
                .one(db.get_ref())
                .await?
            {
                event_teams::Entity::find_by_id(m.team_id)
                    .one(db.get_ref())
                    .await?
            } else {
                None
            }
        } else {
            None
        };

        Ok(EventContext {
            docker: self.docker.context("docker is required")?,
            db,
            user,
            event,
            team,
            config: self.config,
        })
    }
}

impl Default for EventContextBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// 由赛事起止推导的时间窗状态（请求本地）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventTimeStatus {
    NotStarted,
    Ongoing,
    Ended,
}

impl EventContext {
    pub fn time_status(&self) -> EventTimeStatus {
        match event_time_status_of(&self.event, Utc::now()) {
            crate::modules::event::common::domain::time_state::EventTimeStatus::NotStarted => {
                EventTimeStatus::NotStarted
            }
            crate::modules::event::common::domain::time_state::EventTimeStatus::Ongoing => {
                EventTimeStatus::Ongoing
            }
            crate::modules::event::common::domain::time_state::EventTimeStatus::Ended => {
                EventTimeStatus::Ended
            }
        }
    }

    pub fn should_not_started(&self) -> Result<()> {
        match self.time_status() {
            EventTimeStatus::NotStarted => Ok(()),
            EventTimeStatus::Ongoing => Err(anyhow!("Event is ongoing")),
            EventTimeStatus::Ended => Err(anyhow!("Event is ended")),
        }
    }

    pub fn should_ongoing(&self) -> Result<()> {
        match self.time_status() {
            EventTimeStatus::NotStarted => Err(anyhow!("Event is not started")),
            EventTimeStatus::Ongoing => Ok(()),
            EventTimeStatus::Ended => Err(anyhow!("Event is ended")),
        }
    }

    pub fn should_ongoing_or_ended(&self) -> Result<()> {
        match self.time_status() {
            EventTimeStatus::NotStarted => Err(anyhow!("Event is not started")),
            EventTimeStatus::Ongoing | EventTimeStatus::Ended => Ok(()),
        }
    }

    pub async fn should_user_joined(&self) -> Result<()> {
        event_users::Entity::find_by_id((self.event.id, self.user.id))
            .one(self.db.get_ref())
            .await?
            .ok_or_else(|| anyhow!("User not joined the event!"))?;
        Ok(())
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SubmitFlagRequest {
    pub instance_id: Option<Uuid>,
    pub flag: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ModeInstanceResult {
    pub instance: event_challenge_instance::Model,
    /// 归一化运行时行（instances，id 与 instance 相同）。
    pub runtime: event_instances::Model,
    pub challenge_name: String,
    pub nickname: String,
}
