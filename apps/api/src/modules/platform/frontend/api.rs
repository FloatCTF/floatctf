use crate::{
    api::prelude::*,
    core::{API_CONTRACT_VERSION, FRONTEND_RUNTIME_CONTRACT_VERSION, PLATFORM_CAPABILITIES},
    infrastructure::settings::get_setting,
};

use super::{domain::normalize_frontend_id, dto::FrontendBootstrapDto};

/// 平台设置键：当前生效的前端 ID。
pub const FRONTEND_ACTIVE_SETTING_KEY: &str = "FRONTEND_ACTIVE";

/// `GET /api/frontend` —— **未认证**公开引导元数据。
///
/// 必须在登录前可用：登录界面本身属于所选前端。因此这里没有任何认证守卫，
/// 返回内容也严格限制为公开契约字段（见 `FrontendBootstrapDto` 的注释）。
#[get("")]
pub async fn get_frontend_bootstrap(ctx: ReqCtx) -> UniResult<FrontendBootstrapDto> {
    let active = get_setting(ctx.db.get_ref(), FRONTEND_ACTIVE_SETTING_KEY)
        .await
        .ok();

    UniResponse::ok(Some(FrontendBootstrapDto {
        active_frontend: normalize_frontend_id(active.as_deref()),
        platform_version: env!("CARGO_PKG_VERSION").to_string(),
        api_contract_version: API_CONTRACT_VERSION.to_string(),
        frontend_runtime_version: FRONTEND_RUNTIME_CONTRACT_VERSION.to_string(),
        capabilities: PLATFORM_CAPABILITIES
            .iter()
            .map(|c| (*c).to_string())
            .collect(),
    }))
    .into()
}
