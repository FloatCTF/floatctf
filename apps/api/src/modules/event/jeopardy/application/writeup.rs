//! Own writeup URL resolution by Purpose × ParticipantMode.

use anyhow::{Result, anyhow};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

use crate::entity::sea_orm_active_enums::{EventPurpose, ParticipantMode};
use crate::entity::{event_team_members, event_writeup, events, users};
use crate::infrastructure::WebDb;
use crate::modules::event::jeopardy::domain::policy::JeopardyPolicy;

pub async fn own_writeup_file_url(
    db: &WebDb,
    event: &events::Model,
    user: &users::Model,
) -> Result<Option<String>> {
    if event.family != crate::entity::sea_orm_active_enums::EventFamily::Jeopardy {
        return Ok(None);
    }

    let policy = JeopardyPolicy::from_event(event).map_err(|e| anyhow!(e))?;
    match policy.purpose() {
        EventPurpose::Practice => Ok(None),
        EventPurpose::Competition => match policy.participant_mode() {
            ParticipantMode::Individual => {
                let wp = event_writeup::Entity::find()
                    .filter(event_writeup::Column::EventId.eq(event.id))
                    .filter(event_writeup::Column::UserId.eq(user.id))
                    .one(db.get_ref())
                    .await?;
                Ok(wp.map(|w| w.file_url))
            }
            ParticipantMode::Team => {
                let team_id = event_team_members::Entity::find()
                    .filter(event_team_members::Column::UserId.eq(user.id))
                    .filter(event_team_members::Column::EventId.eq(event.id))
                    .one(db.get_ref())
                    .await?
                    .ok_or_else(|| anyhow!("This member has no team!"))?
                    .team_id;

                let wp = event_writeup::Entity::find()
                    .filter(event_writeup::Column::EventId.eq(event.id))
                    .filter(event_writeup::Column::TeamId.eq(team_id))
                    .one(db.get_ref())
                    .await?;
                Ok(wp.map(|w| w.file_url))
            }
        },
    }
}
