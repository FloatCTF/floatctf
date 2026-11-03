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

// ============================================================================
// 前端市场（只做发现，不做安装 —— 见 market.rs 的边界说明）
// ============================================================================

use actix_web::web::Query;
use serde::Deserialize;

use crate::api::extractor::auth::RequirePermission;
use crate::core::permission::guards::SettingsReadGuard;
use crate::infrastructure::WebProxyReqwest;

use super::market::{FrontendMarket, FrontendMarketDto};

/// `GET /api/admin/frontend-market?refresh=true`
#[derive(Debug, Deserialize)]
pub struct FrontendMarketQuery {
    /// 跳过缓存，强制重新拉取 GitHub。
    #[serde(default)]
    refresh: bool,
}

/// `GET /api/admin/frontend-market` —— 列出候选前端仓库及其 release 制品元数据。
///
/// # 权限
///
/// `settings.read`。市场只读**外部**元数据、不改平台状态，因此给读权限而不是
/// `settings.manage`；真正会改变平台的是安装，而那不在 API 里（宿主的 `frontend.sh`）。
///
/// # 为什么是 `settings.*` 而不是新权限
///
/// 「前端管理」在 v2 权限表里本就归 `settings`（管理端 `/console/frontends` 用
/// `settings.manage`）。市场是它的只读辅助视图，拆一条新权限只会让 29 条变 30 条
/// 而没有任何新的授权边界。
///
/// # 出网
///
/// 走平台**唯一**的出网入口 [`WebProxyReqwest`]，因此 `OUTBOUND_PROXY` 设置对市场
/// 同样生效 —— 与训练站同步、AWDP 等其它出网行为保持一致。
#[get("")]
pub async fn get_frontend_market(
    _user: RequirePermission<SettingsReadGuard>,
    ctx: ReqCtx,
    market: actix_web::web::Data<FrontendMarket>,
    http: WebProxyReqwest,
    query: Query<FrontendMarketQuery>,
) -> UniResult<FrontendMarketDto> {
    // 拉取失败按 **500** 处理而不是 400：这是平台/上游的问题（网络、限流、GitHub 抖动），
    // 不是调用方的请求有问题。错误文案里已经带了限流提示。
    let dto = market
        .snapshot(http.get_ref(), &ctx.config.frontend_market, query.refresh)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    UniResponse::ok(Some(dto)).into()
}
