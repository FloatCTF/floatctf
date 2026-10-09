//! 管理员账号响应 DTO。
//!
//! **硬性约束（安全不变量 #1）**：响应 DTO 永不包含 `password` / `password_hash`。
//! 历史缺陷：本模块曾把 `super_admin.password`（argon2id 哈希）通过
//! `GET /api/admin/super_admin` 等端点回传。

use crate::entity::super_admin;
use sea_orm::entity::prelude::{DateTimeWithTimeZone, Uuid};
use serde::Serialize;

/// 管理员账号响应体（**不含** password）。
#[derive(Debug, Serialize)]
pub struct SuperAdminDto {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

impl From<super_admin::Model> for SuperAdminDto {
    fn from(m: super_admin::Model) -> Self {
        Self {
            id: m.id,
            username: m.username,
            // 刻意不映射 m.password
            email: m.email,
            created_at: m.created_at,
            updated_at: m.updated_at,
        }
    }
}
