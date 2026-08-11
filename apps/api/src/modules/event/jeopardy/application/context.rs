//! Request-scoped Jeopardy event context (replaces strategies/event EventContext).

use std::sync::Arc;

use anyhow::{Context as _, Result, anyhow};
use chrono::Utc;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    core::AppConfig,
    entity::{challenge_instances, event_teams, event_users, events, users},
    infrastructure::{WebDb, WebDocker},
    modules::event::common::domain::practice_event::require_practice_jeopardy_event,
    modules::event::common::domain::time_state::{
        EventTimeStatus as CommonEventTimeStatus, event_time_status_of,
    },
};

/// Single-request context for Jeopardy mode operations.
#[derive(Debug)]
pub struct EventContext {
    pub db: WebDb,
    /// Present for launch/destroy/submit paths; may be a placeholder for pure-read builds.
    pub docker: WebDocker,
    pub event: events::Model,
    pub user: users::Model,
    pub team: Option<event_teams::Model>,
    /// Static process config; set on launch-capable paths (instance limits).
    pub config: Option<Arc<AppConfig>>,
}

#[derive(Debug)]
pub struct EventContextBuilder {
    db: Option<WebDb>,
    docker: Option<WebDocker>,
    event: Option<events::Model>,
    user: Option<users::Model>,
    team: Option<event_teams::Model>,
    /// When true, resolve Team from event_team_members for the user+event.
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

    pub fn event(mut self, event: Option<events::Model>) -> Self {
        self.event = event;
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

    /// Auto-load Team membership for this user in the Event (when present).
    pub fn resolve_team(mut self) -> Self {
        self.resolve_team = true;
        self
    }

    /// Attach the static process config (needed for instance limits on launch paths).
    pub fn config(mut self, config: Arc<AppConfig>) -> Self {
        self.config = Some(config);
        self
    }

    /// Construct EventContext. Docker is required for launch/destroy/submit;
    /// pure-read handlers should prefer mode methods that only take `db` + `event`.
    pub async fn build(self) -> Result<EventContext> {
        let db = self.db.clone().context("db is required")?;
        let user = self.user.context("user is required")?;
        let event = match self.event {
            Some(e) => e,
            None => require_practice_jeopardy_event(db.get_ref())
                .await
                .map_err(|e| anyhow!("Practice Event not found: {e}"))?,
        };

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

/// Time-window status derived from event start/end (request-local).
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
    pub instance: challenge_instances::Model,
    pub challenge_name: String,
    pub nickname: String,
}
