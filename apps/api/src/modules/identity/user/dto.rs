use crate::entity::users;
use sea_orm::entity::prelude::{DateTimeWithTimeZone, Uuid};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct UsersDto {
    pub id: Uuid,
    pub username: String,
    pub nickname: String,
    pub password: String,
    pub email: String,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
    pub avatar: Option<String>,
}

impl From<users::Model> for UsersDto {
    fn from(m: users::Model) -> Self {
        Self {
            id: m.id,
            username: m.username,
            nickname: m.nickname,
            password: m.password,
            email: m.email,
            created_at: m.created_at,
            updated_at: m.updated_at,
            avatar: m.avatar,
        }
    }
}
