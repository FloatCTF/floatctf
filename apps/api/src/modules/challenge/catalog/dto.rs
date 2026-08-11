//! Challenge catalog DTOs（单版本：identity 直接承载当前 package 字段）。
//!
//! `static_flag_value` 属于 secret，普通转换（`From<&challenges::Model>`）一律置
//! `None`，仅 admin 列表/详情接口显式填充后返回。

use sea_orm::entity::prelude::DateTimeWithTimeZone;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::entity::challenges;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChallengeAttachmentDto {
    /// File name (display).
    pub name: String,
    /// Relative path inside the package (e.g. `attachment/src.zip`) — used to build the download href.
    pub path: String,
    pub size: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChallengesDto {
    pub id: Uuid,
    pub name: String,
    pub safe_name: String,
    pub category: String,
    pub description: String,
    pub hidden: bool,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
    /// 当前版本（无 package 时为 None）。
    pub version: Option<String>,
    /// building | ready | failed（无 package 时为 None）。
    pub build_status: Option<String>,
    /// 当前版本镜像 tag（admin 可见）。
    pub image_ref: Option<String>,
    pub attachment: Option<ChallengeAttachmentDto>,
    /// 容器端口（Some 表示需要 docker 运行时）。
    pub container_port: Option<i32>,
    /// 推荐资源（赛事部署默认值）。
    pub recommended_cpu_millis: i64,
    pub recommended_memory_bytes: i64,
    pub recommended_pids_limit: i64,
    /// 静态 flag 明文（secret）。仅 admin 接口填充；其他路径恒为 None。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub static_flag_value: Option<String>,
}

impl ChallengesDto {
    /// admin 专用：填充静态 flag 明文（secret）。
    pub fn with_static_flag_value(mut self, v: Option<String>) -> Self {
        self.static_flag_value = v;
        self
    }
}

impl From<challenges::Model> for ChallengesDto {
    fn from(m: challenges::Model) -> Self {
        Self::from(&m)
    }
}

impl From<&challenges::Model> for ChallengesDto {
    fn from(m: &challenges::Model) -> Self {
        Self {
            id: m.id,
            name: m.name.clone(),
            safe_name: m.safe_name.clone(),
            category: m.category.clone(),
            description: m.description.clone(),
            hidden: m.hidden,
            created_at: m.created_at,
            updated_at: m.updated_at,
            version: m.version.clone(),
            build_status: m.build_status.clone(),
            image_ref: m.image_ref.clone(),
            container_port: m.container_port,
            recommended_cpu_millis: m.recommended_cpu_millis,
            recommended_memory_bytes: m.recommended_memory_bytes,
            recommended_pids_limit: m.recommended_pids_limit,
            static_flag_value: None, // secret：普通路径不返回
            attachment: m
                .attachment_name
                .clone()
                .map(|name| ChallengeAttachmentDto {
                    name,
                    path: m.attachment_path.clone().unwrap_or_default(),
                    size: m.attachment_size,
                }),
        }
    }
}
