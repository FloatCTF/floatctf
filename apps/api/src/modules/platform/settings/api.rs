use std::collections::HashMap;

use super::dto::SettingsDto;
use crate::{
    api::{dto::DeleteItemsRequest, prelude::*},
    entity::{sea_orm_active_enums::SettingValueType, settings},
    infrastructure::settings::{resolve_setting_value, resolve_value_with_map},
    modules::platform::frontend::api::FRONTEND_ACTIVE_SETTING_KEY,
    modules::platform::frontend::domain::is_safe_frontend_id,
};

/// `FRONTEND_ACTIVE` 的值必须是安全前端 ID。
///
/// 这个值会出现在**未认证**的 `GET /api/frontend` 响应里，也是浏览器解析本地注册表
/// 的依据。写入非法值（路径穿越、大写、空串）不会造成越权（后端会回落 `default`），
/// 但会让"设置看起来改成功了、前端却还是旧的"这种故障极难排查，所以在写入时就拒绝。
fn validate_setting_value(key: &str, value: &str) -> Result<(), AppError> {
    if key == FRONTEND_ACTIVE_SETTING_KEY && !is_safe_frontend_id(value) {
        return Err(AppError::BadRequest(format!(
            "{} 必须是安全前端 ID（[a-z0-9][a-z0-9._-]*，最长 64 字符）: {}",
            FRONTEND_ACTIVE_SETTING_KEY, value
        )));
    }
    Ok(())
}

/// GET /api/admin/settings
#[get("")]
pub async fn get_settings(_user: SuperAdminJwtGuard, ctx: ReqCtx) -> UniResult<Vec<SettingsDto>> {
    let db = ctx.db.get_ref();
    let rows = settings::Entity::find()
        .order_by_desc(settings::Column::UpdatedAt)
        .all(db)
        .await?;
    let map: HashMap<String, String> = rows
        .iter()
        .map(|s| (s.key.clone(), s.value.clone()))
        .collect();
    let dtos = rows
        .into_iter()
        .map(|s| {
            let resolved = resolve_value_with_map(&s.value, &map);
            SettingsDto::from_model(s, resolved)
        })
        .collect();
    UniResponse::ok(Some(dtos)).into()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateSettingRequest {
    pub key: String,
    pub value: String,
    pub description: String,
    pub protected: bool,
    pub r#type: SettingValueType,
}

/// POST /api/admin/settings
#[post("")]
pub async fn create_setting(
    user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    csr: Json<CreateSettingRequest>,
) -> UniResult<SettingsDto> {
    let user = user.into_inner();
    let csr = csr.into_inner();
    validate_setting_value(&csr.key, &csr.value)?;

    let setting = settings::ActiveModel {
        key: Set(csr.key),
        value: Set(csr.value),
        description: Set(csr.description),
        r#type: Set(csr.r#type),
        protected: Set(csr.protected),
        ..Default::default()
    };
    let setting = setting.insert(ctx.db.get_ref()).await?;
    // 新写入的设置需立即生效：失效 Redis 缓存后再解析（resolve 会重读 map）。
    crate::infrastructure::settings::invalidate_settings_cache().await;
    let resolved_value = resolve_setting_value(ctx.db.get_ref(), &setting.value).await;

    ctx.log
        .add_log(
            "INFO",
            "SETTINGS",
            "CREATE",
            format!("{} 创建设置: {}", user.username, setting.key).as_str(),
            json!({"key": setting.key}),
            None,
            user.id.into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok(Some(SettingsDto::from_model(setting, resolved_value))).into()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PatchSettingRequest {
    pub key: Option<String>,
    pub value: Option<String>,
    pub description: Option<String>,
    pub protected: Option<bool>,
    pub r#type: Option<SettingValueType>,
}

/// PATCH /api/admin/settings/{setting_id}
#[patch("/{setting_id}")]
pub async fn patch_setting(
    user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    setting_id: Path<Uuid>,
    psr: Json<PatchSettingRequest>,
) -> UniResult<SettingsDto> {
    let user = user.into_inner();
    let setting_id = setting_id.into_inner();
    let psr = psr.into_inner();
    let setting = settings::Entity::find_by_id(setting_id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!("{} 不存在", setting_id)))?;

    // 校验用 patch 之后的最终 key/value（只改 key 不改值也要按新 key 校验）。
    let effective_key = psr.key.clone().unwrap_or_else(|| setting.key.clone());
    let effective_value = psr.value.clone().unwrap_or_else(|| setting.value.clone());
    validate_setting_value(&effective_key, &effective_value)?;

    let mut m_setting = setting.into_active_model();

    psr.key.map(|k| {
        m_setting.key = Set(k);
    });
    psr.value.map(|v| {
        m_setting.value = Set(v);
    });
    psr.description.map(|d| {
        m_setting.description = Set(d);
    });
    psr.r#type.map(|t| {
        m_setting.r#type = Set(t);
    });
    psr.protected.map(|p| {
        m_setting.protected = Set(p);
    });
    let setting = m_setting.update(ctx.db.get_ref()).await?;
    // 编辑后立即生效：失效 Redis 缓存后再解析（resolve 会重读 map）。
    crate::infrastructure::settings::invalidate_settings_cache().await;
    let resolved_value = resolve_setting_value(ctx.db.get_ref(), &setting.value).await;

    ctx.log
        .add_log(
            "INFO",
            "SETTINGS",
            "UPDATE",
            format!("{} 更新设置: {}", user.username, setting.key).as_str(),
            json!({"key": setting.key}),
            None,
            user.id.into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok(Some(SettingsDto::from_model(setting, resolved_value))).into()
}

/// DELETE /api/admin/settings
#[delete("")]
pub async fn delete_setting(
    user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    dir: Json<DeleteItemsRequest>,
) -> UniResult<u64> {
    let user = user.into_inner();
    let dir = dir.into_inner();
    let mut deleted_count = 0;
    for setting_id in dir.id_list {
        let setting = settings::Entity::find_by_id(setting_id)
            .one(ctx.db.get_ref())
            .await?;
        if let Some(setting) = setting {
            if setting.protected {
                return Err(AppError::BadRequest(format!(
                    "protected setting can not be deleted: {}",
                    setting.key
                )));
            }
            let r = setting.delete(ctx.db.get_ref()).await?;
            deleted_count += r.rows_affected;
        }
    }

    ctx.log
        .add_log(
            "INFO",
            "SETTINGS",
            "DELETE",
            format!("{} 删除 {} 条设置", user.username, deleted_count).as_str(),
            json!({"deleted_count": deleted_count}),
            None,
            user.id.into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok(deleted_count.into()).into()
}
