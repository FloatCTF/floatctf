//! Discussion comments for player and admin surfaces.

pub mod dto;
pub use dto::DiscussionCommentsDto;


pub mod admin;
pub mod player;

use crate::entity::discussion_comments;
use sea_orm::entity::prelude::Uuid;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct CommentWithAuthor {
    #[serde(flatten)]
    pub comment: discussion_comments::Model,
    pub author_nickname: String,
    pub author_avatar: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateCommentRequest {
    pub content: String,
    pub parent_id: Option<Uuid>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PatchCommentRequest {
    pub content: Option<String>,
}
