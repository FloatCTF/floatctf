//! Competition mode registry — dispatch to mode services by EventType.

use anyhow::{Result, anyhow};
use uuid::Uuid;

use crate::{
    entity::{events, instances, sea_orm_active_enums::EventType, users},
    infrastructure::WebDb,
};

use super::common::domain::capability::EventCapabilities;
use super::jeopardy::application::{
    common as jeopardy_common,
    context::{EventContext, ModeInstanceResult, SubmitFlagRequest},
};
use super::jeopardy::domain::scoreboard::ScoreboardItem;
use super::jeopardy::domain::trend::TrendItem;
use super::jeopardy::modes::{
    JeopardyPracticeServices, JeopardySingleServices, JeopardyTeamServices,
};

/// Mode service entry points for all competition types.
#[derive(Clone, Default)]
pub struct EventModuleRegistry {
    pub jeopardy_practice: JeopardyPracticeServices,
    pub jeopardy_single: JeopardySingleServices,
    pub jeopardy_team: JeopardyTeamServices,
}

impl EventModuleRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn capabilities_for(event_type: &EventType) -> EventCapabilities {
        EventCapabilities::for_event_type(event_type)
    }

    /// Resolve Jeopardy mode services by event type (AWD is separate stack).
    pub fn for_event_type(&self, event_type: &EventType) -> Option<JeopardyModeHandle<'_>> {
        match event_type {
            EventType::JeopardyPractice => {
                Some(JeopardyModeHandle::Practice(&self.jeopardy_practice))
            }
            EventType::JeopardySingle => Some(JeopardyModeHandle::Single(&self.jeopardy_single)),
            EventType::JeopardyTeam => Some(JeopardyModeHandle::Team(&self.jeopardy_team)),
            EventType::AwdTeam => None,
        }
    }

    fn unsupported_awd(op: &str) -> anyhow::Error {
        anyhow!(
            "UnsupportedForEventType: AWD events do not support '{op}' via the generic Event API; use /api/events/{{id}}/awd"
        )
    }

    pub async fn submit_flag(&self, ctx: &EventContext, sfr: SubmitFlagRequest) -> Result<()> {
        match &ctx.event.r#type {
            EventType::JeopardyPractice => {
                self.jeopardy_practice.submit_from_context(ctx, &sfr).await
            }
            EventType::JeopardySingle => {
                let instance_id = sfr.instance_id.ok_or(anyhow!("no instance_id"))?;
                self.jeopardy_single
                    .submit_flag(ctx, instance_id, &sfr.flag)
                    .await
            }
            EventType::JeopardyTeam => {
                let instance_id = sfr.instance_id.ok_or(anyhow!("no instance_id"))?;
                self.jeopardy_team
                    .submit_flag(ctx, instance_id, &sfr.flag)
                    .await
            }
            EventType::AwdTeam => Err(Self::unsupported_awd("submit flag via /api/submit/flag")),
        }
    }

    pub async fn launch_instance(
        &self,
        ctx: &EventContext,
        challenge_id: Uuid,
    ) -> Result<instances::Model> {
        match &ctx.event.r#type {
            EventType::JeopardyPractice => {
                self.jeopardy_practice
                    .launch_from_context(ctx, challenge_id)
                    .await
            }
            EventType::JeopardySingle => {
                self.jeopardy_single
                    .launch_instance(ctx, challenge_id)
                    .await
            }
            EventType::JeopardyTeam => self.jeopardy_team.launch_instance(ctx, challenge_id).await,
            EventType::AwdTeam => Err(Self::unsupported_awd("launch_instance")),
        }
    }

    pub async fn destroy_instance(&self, ctx: &EventContext, instance_id: Uuid) -> Result<()> {
        match &ctx.event.r#type {
            EventType::AwdTeam => Err(Self::unsupported_awd("destroy_instance")),
            _ => {
                jeopardy_common::destroy_instance(&ctx.db, &ctx.docker, instance_id, &ctx.user)
                    .await
            }
        }
    }

    pub async fn get_instances(&self, ctx: &EventContext) -> Result<Vec<ModeInstanceResult>> {
        match &ctx.event.r#type {
            EventType::JeopardyPractice => self.jeopardy_practice.get_instances(ctx).await,
            EventType::JeopardySingle => self.jeopardy_single.get_instances(ctx).await,
            EventType::JeopardyTeam => self.jeopardy_team.get_instances(ctx).await,
            EventType::AwdTeam => Err(Self::unsupported_awd("get_instances")),
        }
    }

    pub async fn get_instance_by_challenge_id(
        &self,
        ctx: &EventContext,
        challenge_id: Uuid,
    ) -> Result<instances::Model> {
        match &ctx.event.r#type {
            EventType::JeopardyPractice => {
                self.jeopardy_practice
                    .get_instance_by_challenge_id(ctx, challenge_id)
                    .await
            }
            EventType::JeopardySingle => {
                self.jeopardy_single
                    .get_instance_by_challenge_id(ctx, challenge_id)
                    .await
            }
            EventType::JeopardyTeam => {
                self.jeopardy_team
                    .get_instance_by_challenge_id(ctx, challenge_id)
                    .await
            }
            EventType::AwdTeam => Err(Self::unsupported_awd("get_instance_by_challenge_id")),
        }
    }

    pub async fn get_scoreboard(
        &self,
        db: &WebDb,
        event: &events::Model,
    ) -> Result<Vec<ScoreboardItem>> {
        match &event.r#type {
            EventType::JeopardyPractice => self.jeopardy_practice.get_scoreboard(db, event).await,
            EventType::JeopardySingle => self.jeopardy_single.get_scoreboard(db, event).await,
            EventType::JeopardyTeam => self.jeopardy_team.get_scoreboard(db, event).await,
            EventType::AwdTeam => Err(Self::unsupported_awd("get_scoreboard")),
        }
    }

    pub async fn get_trend(&self, db: &WebDb, event: &events::Model) -> Result<Vec<TrendItem>> {
        match &event.r#type {
            EventType::JeopardyPractice => self.jeopardy_practice.get_trend(db, event).await,
            EventType::JeopardySingle => self.jeopardy_single.get_trend(db, event).await,
            EventType::JeopardyTeam => self.jeopardy_team.get_trend(db, event).await,
            EventType::AwdTeam => Err(Self::unsupported_awd("get_trend")),
        }
    }

    pub async fn challenge_solve_status(
        &self,
        db: &WebDb,
        event: &events::Model,
        user: &users::Model,
        challenge_id: Uuid,
    ) -> Result<(bool, u64)> {
        match &event.r#type {
            EventType::JeopardyPractice => {
                self.jeopardy_practice
                    .challenge_solve_status(db.get_ref(), event.id, challenge_id, user.id)
                    .await
            }
            EventType::JeopardySingle => {
                self.jeopardy_single
                    .challenge_solve_status(db.get_ref(), event.id, challenge_id, user.id)
                    .await
            }
            EventType::JeopardyTeam => {
                self.jeopardy_team
                    .challenge_solve_status(db.get_ref(), event.id, challenge_id, user.id)
                    .await
            }
            EventType::AwdTeam => Err(Self::unsupported_awd("challenge_solve_status")),
        }
    }

    pub async fn own_writeup_file_url(
        &self,
        db: &WebDb,
        event: &events::Model,
        user: &users::Model,
    ) -> Result<Option<String>> {
        match &event.r#type {
            EventType::JeopardyPractice => {
                self.jeopardy_practice
                    .own_writeup_file_url(db, event, user)
                    .await
            }
            EventType::JeopardySingle => {
                self.jeopardy_single
                    .own_writeup_file_url(db, event, user)
                    .await
            }
            EventType::JeopardyTeam => {
                self.jeopardy_team
                    .own_writeup_file_url(db, event, user)
                    .await
            }
            EventType::AwdTeam => Ok(None),
        }
    }
}

/// Borrowed handle to a Jeopardy mode service bag.
pub enum JeopardyModeHandle<'a> {
    Practice(&'a JeopardyPracticeServices),
    Single(&'a JeopardySingleServices),
    Team(&'a JeopardyTeamServices),
}

/// Alias used by plan §6.
pub type EventServices = EventModuleRegistry;

/// Construct a registry for non-HTTP call sites (services without request extractors).
///
/// Prefer `web::Data<EventModuleRegistry>` / `AppState.event_registry` in handlers —
/// both are the same zero-config `Default` bag of mode services.
pub fn event_registry() -> EventModuleRegistry {
    EventModuleRegistry::new()
}

#[cfg(test)]
mod e9_boundary_tests {
    use super::*;
    use crate::entity::sea_orm_active_enums::EventType;
    use crate::modules::event::common::domain::capability::ParticipantMode;

    #[test]
    fn all_event_types_have_capabilities() {
        for et in [
            EventType::JeopardyPractice,
            EventType::JeopardySingle,
            EventType::JeopardyTeam,
            EventType::AwdTeam,
        ] {
            let caps = EventModuleRegistry::capabilities_for(&et);
            match et {
                EventType::AwdTeam => {
                    assert_eq!(caps.participant_mode, ParticipantMode::Team);
                    assert!(caps.supports_gameboxes);
                    assert!(caps.supports_wireguard);
                    assert!(!caps.supports_standard_flag_submission);
                    assert!(!caps.supports_instances);
                }
                EventType::JeopardyTeam => {
                    assert_eq!(caps.participant_mode, ParticipantMode::Team);
                    assert!(caps.supports_instances);
                    assert!(caps.supports_standard_flag_submission);
                    assert!(!caps.supports_wireguard);
                }
                EventType::JeopardyPractice | EventType::JeopardySingle => {
                    assert_eq!(caps.participant_mode, ParticipantMode::Individual);
                    assert!(caps.supports_instances);
                    assert!(caps.supports_standard_flag_submission);
                }
            }
        }
    }

    #[test]
    fn registry_resolves_jeopardy_modes_not_awd() {
        let reg = EventModuleRegistry::new();
        assert!(reg.for_event_type(&EventType::JeopardyPractice).is_some());
        assert!(reg.for_event_type(&EventType::JeopardySingle).is_some());
        assert!(reg.for_event_type(&EventType::JeopardyTeam).is_some());
        assert!(reg.for_event_type(&EventType::AwdTeam).is_none());
    }

    #[test]
    fn app_state_field_type_is_default_constructible() {
        // Ensures AppState can hold registry without external deps.
        let _ = EventModuleRegistry::new();
        let _ = event_registry();
    }
}
