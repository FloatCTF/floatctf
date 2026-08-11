use crate::entity::challenge_writeup;
use sea_orm::entity::prelude::{DateTimeWithTimeZone, Uuid};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ChallengeWriteupDto {
    pub id: Uuid,
    pub challenge_id: Uuid,
    pub user_id: Uuid,
    pub content: String,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

impl From<challenge_writeup::Model> for ChallengeWriteupDto {
    fn from(m: challenge_writeup::Model) -> Self {
        Self {
            id: m.id,
            challenge_id: m.challenge_id,
            user_id: m.user_id,
            content: m.content,
            created_at: m.created_at,
            updated_at: m.updated_at,
        }
    }
}

/// 全局 Writeup 列表统一条目（challenge + gamebox 合并；列表不返回正文）。
/// `writeup_type` = "challenge" | "gamebox"（内容名分别指题目/GameBox）。
#[derive(Debug, Serialize)]
pub struct UnifiedWriteupResult {
    pub id: Uuid,
    pub writeup_type: String,
    pub nickname: String,
    pub avatar: Option<String>,
    pub email: String,
    /// challenge.id 或 gamebox.id（practice run 的 gamebox）。
    pub content_id: Uuid,
    pub content_name: String,
    pub updated_at: DateTimeWithTimeZone,
}

/// 单个 Writeup 详情统一条目（challenge + gamebox 都能渲染）。
/// `writeup_type` = "challenge" | "gamebox"；gamebox 的 `id` 即 practice run 的 run_id。
#[derive(Debug, Serialize)]
pub struct UnifiedWriteupDetail {
    pub writeup_type: String,
    pub id: Uuid,
    pub content_id: Uuid,
    pub content_name: String,
    /// challenge 才有（category），gamebox 为 None。
    pub category: Option<String>,
    pub nickname: String,
    pub avatar: Option<String>,
    pub email: String,
    pub content: String,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}
