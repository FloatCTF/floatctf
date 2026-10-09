//! 账号响应 DTO。
//!
//! **硬性约束（安全不变量 #1）**：响应 DTO 永不包含 `password` / `password_hash`。
//! 密码字段只允许出现在**请求** DTO 里（建号 / 改号 / 改密 / 重置）。
//!
//! 历史缺陷：本模块曾直接复用 `entity::users::Model` 的 `password` 列，
//! 导致 `GET /api/admin/users` 等管理端点把 argon2id 哈希回传给客户端。
//! 因此响应结构在这里**独立定义**，不再与 DB 列同构。

use crate::entity::users;
use sea_orm::entity::prelude::{DateTimeWithTimeZone, Uuid};
use serde::Serialize;

/// 账号响应体（选手视角 / 管理端用户列表共用）。
#[derive(Debug, Serialize)]
pub struct UsersDto {
    pub id: Uuid,
    pub username: String,
    pub nickname: String,
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
            // 刻意不映射 m.password
            email: m.email,
            created_at: m.created_at,
            updated_at: m.updated_at,
            avatar: m.avatar,
        }
    }
}
