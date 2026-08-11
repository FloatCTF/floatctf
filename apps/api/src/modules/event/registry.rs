//! Competition mode registry — dispatch Jeopardy ops by EventMode (AWD is separate stack).

use anyhow::{Result, anyhow};
use uuid::Uuid;

use crate::{
    entity::sea_orm_active_enums::EventFamily,
    entity::{challenge_instances, events, users},
    infrastructure::WebDb,
    modules::event::common::domain::{
        capability::EventCapabilities,
        event_mode::{EventMode, JeopardyVariant},
    },
};

use super::jeopardy::application::{
    common as jeopardy_common,
    context::{EventContext, ModeInstanceResult, SubmitFlagRequest},
};
use super::jeopardy::domain::scoreboard::ScoreboardItem;
use super::jeopardy::domain::trend::TrendItem;
use super::jeopardy::modes::{
    JeopardyPracticeServices, JeopardySingleServices, JeopardyTeamServices,
};

/// Mode service entry points for Jeopardy competition types.
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

    pub fn capabilities_for(mode: &EventMode) -> EventCapabilities {
        EventCapabilities::for_mode(mode)
    }

    pub fn capabilities_for_event(event: &events::Model) -> EventCapabilities {
        Self::capabilities_for(&event.mode_unchecked())
    }

    /// Resolve Jeopardy mode services by EventMode (AWD returns None).
    pub fn for_mode(&self, mode: &EventMode) -> Option<JeopardyModeHandle<'_>> {
        match mode.jeopardy_variant()? {
            JeopardyVariant::Practice => {
                Some(JeopardyModeHandle::Practice(&self.jeopardy_practice))
            }
            JeopardyVariant::IndividualCompetition => {
                Some(JeopardyModeHandle::Single(&self.jeopardy_single))
            }
            JeopardyVariant::TeamCompetition => Some(JeopardyModeHandle::Team(&self.jeopardy_team)),
        }
    }

    pub fn for_event(&self, event: &events::Model) -> Option<JeopardyModeHandle<'_>> {
        self.for_mode(&event.mode_unchecked())
    }

    fn require_jeopardy(event: &events::Model, op: &str) -> Result<()> {
        if event.family != EventFamily::Jeopardy {
            return Err(anyhow!(
                "UnsupportedForFamily: AWD events do not support '{op}' via the generic Event API; use /api/events/{{id}}/awd"
            ));
        }
        Ok(())
    }

    fn handle_for<'a>(&'a self, event: &events::Model, op: &str) -> Result<JeopardyModeHandle<'a>> {
        Self::require_jeopardy(event, op)?;
        self.for_event(event).ok_or_else(|| {
            anyhow!(
                "UnsupportedForMode: no Jeopardy handler for event {}",
                event.id
            )
        })
    }

    pub async fn submit_flag(&self, ctx: &EventContext, sfr: SubmitFlagRequest) -> Result<()> {
        match self.handle_for(&ctx.event, "submit flag")? {
            JeopardyModeHandle::Practice(s) => s.submit_from_context(ctx, &sfr).await,
            JeopardyModeHandle::Single(s) => {
                let instance_id = sfr.instance_id.ok_or(anyhow!("no instance_id"))?;
                s.submit_flag(ctx, instance_id, &sfr.flag).await
            }
            JeopardyModeHandle::Team(s) => {
                let instance_id = sfr.instance_id.ok_or(anyhow!("no instance_id"))?;
                s.submit_flag(ctx, instance_id, &sfr.flag).await
            }
        }
    }

    pub async fn launch_instance(
        &self,
        ctx: &EventContext,
        challenge_id: Uuid,
    ) -> Result<challenge_instances::Model> {
        match self.handle_for(&ctx.event, "launch_instance")? {
            JeopardyModeHandle::Practice(s) => s.launch_from_context(ctx, challenge_id).await,
            JeopardyModeHandle::Single(s) => s.launch_instance(ctx, challenge_id).await,
            JeopardyModeHandle::Team(s) => s.launch_instance(ctx, challenge_id).await,
        }
    }

    pub async fn destroy_instance(&self, ctx: &EventContext, instance_id: Uuid) -> Result<()> {
        Self::require_jeopardy(&ctx.event, "destroy_instance")?;
        jeopardy_common::destroy_instance(&ctx.db, &ctx.docker, instance_id, &ctx.user).await
    }

    pub async fn get_instances(&self, ctx: &EventContext) -> Result<Vec<ModeInstanceResult>> {
        match self.handle_for(&ctx.event, "get_instances")? {
            JeopardyModeHandle::Practice(s) => s.get_instances(ctx).await,
            JeopardyModeHandle::Single(s) => s.get_instances(ctx).await,
            JeopardyModeHandle::Team(s) => s.get_instances(ctx).await,
        }
    }

    pub async fn get_instance_by_challenge_id(
        &self,
        ctx: &EventContext,
        challenge_id: Uuid,
    ) -> Result<challenge_instances::Model> {
        match self.handle_for(&ctx.event, "get_instance_by_challenge_id")? {
            JeopardyModeHandle::Practice(s) => {
                s.get_instance_by_challenge_id(ctx, challenge_id).await
            }
            JeopardyModeHandle::Single(s) => {
                s.get_instance_by_challenge_id(ctx, challenge_id).await
            }
            JeopardyModeHandle::Team(s) => s.get_instance_by_challenge_id(ctx, challenge_id).await,
        }
    }

    pub async fn get_scoreboard(
        &self,
        db: &WebDb,
        event: &events::Model,
    ) -> Result<Vec<ScoreboardItem>> {
        match self.handle_for(event, "get_scoreboard")? {
            JeopardyModeHandle::Practice(s) => s.get_scoreboard(db, event).await,
            JeopardyModeHandle::Single(s) => s.get_scoreboard(db, event).await,
            JeopardyModeHandle::Team(s) => s.get_scoreboard(db, event).await,
        }
    }

    pub async fn get_trend(&self, db: &WebDb, event: &events::Model) -> Result<Vec<TrendItem>> {
        match self.handle_for(event, "get_trend")? {
            JeopardyModeHandle::Practice(s) => s.get_trend(db, event).await,
            JeopardyModeHandle::Single(s) => s.get_trend(db, event).await,
            JeopardyModeHandle::Team(s) => s.get_trend(db, event).await,
        }
    }

    pub async fn challenge_solve_status(
        &self,
        db: &WebDb,
        event: &events::Model,
        user: &users::Model,
        challenge_id: Uuid,
    ) -> Result<(bool, u64)> {
        match self.handle_for(event, "challenge_solve_status")? {
            JeopardyModeHandle::Practice(s) => {
                s.challenge_solve_status(db.get_ref(), event.id, challenge_id, user.id)
                    .await
            }
            JeopardyModeHandle::Single(s) => {
                s.challenge_solve_status(db.get_ref(), event.id, challenge_id, user.id)
                    .await
            }
            JeopardyModeHandle::Team(s) => {
                s.challenge_solve_status(db.get_ref(), event.id, challenge_id, user.id)
                    .await
            }
        }
    }

    pub async fn own_writeup_file_url(
        &self,
        db: &WebDb,
        event: &events::Model,
        user: &users::Model,
    ) -> Result<Option<String>> {
        if event.family != EventFamily::Jeopardy {
            return Ok(None);
        }
        match self.handle_for(event, "own_writeup_file_url")? {
            JeopardyModeHandle::Practice(s) => s.own_writeup_file_url(db, event, user).await,
            JeopardyModeHandle::Single(s) => s.own_writeup_file_url(db, event, user).await,
            JeopardyModeHandle::Team(s) => s.own_writeup_file_url(db, event, user).await,
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
pub fn event_registry() -> EventModuleRegistry {
    EventModuleRegistry::new()
}

#[cfg(test)]
mod e9_boundary_tests {
    use super::*;
    use crate::entity::sea_orm_active_enums::ParticipantMode;
    use crate::modules::event::common::domain::event_mode::EventMode;

    #[test]
    fn all_valid_modes_have_capabilities() {
        for mode in [
            EventMode::jeopardy_practice(),
            EventMode::jeopardy_individual_competition(),
            EventMode::jeopardy_team_competition(),
            EventMode::awd_team_competition(),
        ] {
            let caps = EventModuleRegistry::capabilities_for(&mode);
            if mode.is_awd() {
                assert_eq!(caps.participant_mode, ParticipantMode::Team);
                assert!(caps.supports_gameboxes);
                assert!(caps.supports_wireguard);
                assert!(!caps.supports_standard_flag_submission);
                assert!(!caps.supports_instances);
            } else if mode.is_team() {
                assert_eq!(caps.participant_mode, ParticipantMode::Team);
                assert!(caps.supports_instances);
                assert!(caps.supports_standard_flag_submission);
                assert!(!caps.supports_wireguard);
            } else {
                assert_eq!(caps.participant_mode, ParticipantMode::Individual);
                assert!(caps.supports_instances);
                assert!(caps.supports_standard_flag_submission);
            }
        }
    }

    #[test]
    fn registry_resolves_jeopardy_modes_not_awd() {
        let reg = EventModuleRegistry::new();
        assert!(reg.for_mode(&EventMode::jeopardy_practice()).is_some());
        assert!(
            reg.for_mode(&EventMode::jeopardy_individual_competition())
                .is_some()
        );
        assert!(
            reg.for_mode(&EventMode::jeopardy_team_competition())
                .is_some()
        );
        assert!(reg.for_mode(&EventMode::awd_team_competition()).is_none());
    }

    #[test]
    fn app_state_field_type_is_default_constructible() {
        let _ = EventModuleRegistry::new();
        let _ = event_registry();
    }
}
