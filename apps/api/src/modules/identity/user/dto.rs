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

/// `GET /api/users/me` 的响应体。
///
/// 在账号字段之上补两项**授权**信息，供前端做权限感知导航：
/// - `permissions`：本次请求解析出的**有效权限**（多角色并集；`grants_all` 下为全集）；
/// - `grants_all`：是否持有全权角色（内置 `super-admin`）。
///
/// 用 `#[serde(flatten)]` 内联 `UsersDto`，因此**旧前端读 `id` / `username` /
/// `avatar` 的代码完全不受影响** —— 这是一次加法式变更，不是破坏性变更。
///
/// 注意：这里回传权限**不代表**前端可以据此做授权判定。它只用于隐藏/显示入口，
/// 真正的判定永远在服务端每个请求上（`RequirePermission<P>`）。前端拿到
/// `permissions` 就以为「有权限」而跳过错误处理，是错误用法。
#[derive(Debug, Serialize)]
pub struct MeDto {
    #[serde(flatten)]
    pub account: UsersDto,
    /// 有效权限的稳定字符串（顺序确定）。
    pub permissions: Vec<String>,
    /// 是否持有全权角色（`grants_all`）。
    pub grants_all: bool,
}
