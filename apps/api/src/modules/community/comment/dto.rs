use crate::entity::discussion_comments;
use sea_orm::entity::prelude::{DateTimeWithTimeZone, Uuid};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct DiscussionCommentsDto {
    pub id: Uuid,
    pub discussion_id: Uuid,
    pub author_id: Uuid,
    pub content: String,
    pub parent_id: Option<Uuid>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

impl From<discussion_comments::Model> for DiscussionCommentsDto {
    fn from(m: discussion_comments::Model) -> Self {
        Self {
            id: m.id,
            discussion_id: m.discussion_id,
            author_id: m.author_id,
            content: m.content,
            parent_id: m.parent_id,
            created_at: m.created_at,
            updated_at: m.updated_at,
        }
    }
}
