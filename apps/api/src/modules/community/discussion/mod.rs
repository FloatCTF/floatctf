//! Discussion CRUD for player and admin surfaces.

pub mod dto;
pub use dto::DiscussionsDto;


pub mod admin;
pub mod player;

use crate::entity::discussions;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct DiscussionWithAuthor {
    #[serde(flatten)]
    pub discussion: discussions::Model,
    pub author_nickname: String,
    pub author_avatar: Option<String>,
    pub is_liked: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateDiscussionRequest {
    pub title: String,
    pub content: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PatchDiscussionRequest {
    pub title: Option<String>,
    pub content: Option<String>,
}
