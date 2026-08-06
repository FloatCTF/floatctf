use crate::entity::discussions;
use sea_orm::entity::prelude::{DateTimeWithTimeZone, Uuid};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct DiscussionsDto {
    pub id: Uuid,
    pub title: String,
    pub content: String,
    pub author_id: Uuid,
    pub view_count: i32,
    pub like_count: i32,
    pub comment_count: i32,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

impl From<discussions::Model> for DiscussionsDto {
    fn from(m: discussions::Model) -> Self {
        Self {
            id: m.id,
            title: m.title,
            content: m.content,
            author_id: m.author_id,
            view_count: m.view_count,
            like_count: m.like_count,
            comment_count: m.comment_count,
            created_at: m.created_at,
            updated_at: m.updated_at,
        }
    }
}
