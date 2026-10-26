//! 开发者中心（`/api/oidc/*`，玩家 JWT）与管理员审核（`/api/admin/oidc/*`，超管 JWT）。
//!
//! 权限在服务端执行：所有者接口一律按 `owner_user_id` 过滤（他人应用一律 404，不暴露存在性）；
//! 审批/封禁/PKCE 豁免只走 `SuperAdminJwtGuard`。所有变更写审计日志（只记 client_id 等非秘密元数据）。

use actix_web::{HttpRequest, delete, get, patch, post, web};
use chrono::{DateTime, FixedOffset};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, IntoActiveModel, PaginatorTrait,
    QueryFilter, QueryOrder,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::api::extractor::auth::{SuperAdminJwtGuard, UserJwtGuard};
use crate::api::{AppError, UniResponse, UniResult};
use crate::entity::sea_orm_active_enums::{OidcClientStatus, OidcClientType, OidcTokenAuthMethod};
use crate::entity::{oidc_clients, oidc_consents, users};
use crate::infrastructure::{WebDb, WebLog};
use crate::modules::identity::oidc::{
    OidcProvider, WebOidc,
    clients::{self, AdminAction, ClientInput, OwnerAction},
    domain::{metadata, scope},
    store,
};

fn require(oidc: Option<WebOidc>) -> Result<WebOidc, AppError> {
    oidc.ok_or_else(|| AppError::NotFound("OIDC 未启用".into()))
}

fn audit(
    log: &WebLog,
    req: &HttpRequest,
    action: &str,
    message: String,
    client_id: &str,
    user: Option<uuid::Uuid>,
    admin: Option<uuid::Uuid>,
) {
    let log = log.clone();
    let req = req.clone();
    let action = action.to_string();
    let details = json!({ "client_id": client_id });
    actix_web::rt::spawn(async move {
        log.add_log(
            "INFO",
            "OIDC",
            &action,
            &message,
            details,
            user,
            admin,
            Some(&req),
        )
        .await;
    });
}

async fn rate(
    oidc: &OidcProvider,
    bucket: &str,
    key: &str,
    limit: u32,
    window: u64,
) -> Result<(), AppError> {
    match oidc.rate.hit(bucket, key, limit, window).await {
        Ok(true) => Ok(()),
        Ok(false) => Err(AppError::Forbidden("操作过于频繁，请稍后再试".into())),
        Err(e) => {
            tracing::error!(error = %e, "OIDC rate limiter unavailable (fail-closed)");
            Err(AppError::Internal("服务暂时不可用，请稍后重试".into()))
        }
    }
}

// ── DTO ─────────────────────────────────────────────────────────────────────

/// 应用视图（永不包含 secret 或其哈希）。
#[derive(Debug, Serialize)]
pub struct ClientDto {
    pub client_id: String,
    pub name: String,
    pub description: Option<String>,
    pub homepage_url: Option<String>,
    pub logo_url: Option<String>,
    pub client_type: OidcClientType,
    pub token_endpoint_auth_method: OidcTokenAuthMethod,
    pub redirect_uris: Vec<String>,
    pub allowed_scopes: Vec<String>,
    /// RP-Initiated Logout 允许的退出后回跳地址。
    pub post_logout_redirect_uris: Vec<String>,
    /// Back-Channel Logout 接收地址（只在用户明确「退出所有关联应用」时投递）。
    pub backchannel_logout_uri: Option<String>,
    pub require_pkce: bool,
    pub status: OidcClientStatus,
    pub status_reason: Option<String>,
    pub has_secret: bool,
    pub secret_rotated_at: Option<DateTime<FixedOffset>>,
    /// 轮换后旧 secret 的失效时间（为空表示没有仍有效的旧 secret）。
    pub previous_secret_expires_at: Option<DateTime<FixedOffset>>,
    /// 当前有效授权的用户数（删除/暂停前提示影响面）。
    pub active_authorizations: u64,
    pub created_at: DateTime<FixedOffset>,
    pub updated_at: DateTime<FixedOffset>,
}

impl ClientDto {
    fn from_model(m: oidc_clients::Model, active_authorizations: u64) -> Self {
        let previous_valid = m
            .previous_secret_expires_at
            .filter(|t| t.with_timezone(&chrono::Utc) > chrono::Utc::now());
        Self {
            redirect_uris: clients::redirect_uris_of(&m),
            allowed_scopes: clients::scopes_of(&m),
            post_logout_redirect_uris: clients::post_logout_redirect_uris_of(&m),
            backchannel_logout_uri: m.backchannel_logout_uri.clone(),
            client_id: m.client_id,
            name: m.name,
            description: m.description,
            homepage_url: m.homepage_url,
            logo_url: m.logo_url,
            client_type: m.client_type,
            token_endpoint_auth_method: m.token_endpoint_auth_method,
            require_pkce: m.require_pkce,
            status: m.status,
            status_reason: m.status_reason,
            has_secret: m.secret_hash.is_some(),
            secret_rotated_at: m.secret_rotated_at,
            previous_secret_expires_at: previous_valid,
            active_authorizations,
            created_at: m.created_at,
            updated_at: m.updated_at,
        }
    }
}

async fn authorization_count(
    db: &sea_orm::DatabaseConnection,
    client_pk: uuid::Uuid,
) -> Result<u64, AppError> {
    Ok(oidc_consents::Entity::find()
        .filter(oidc_consents::Column::OidcClientId.eq(client_pk))
        .filter(oidc_consents::Column::RevokedAt.is_null())
        .count(db)
        .await?)
}

async fn dto(
    db: &sea_orm::DatabaseConnection,
    m: oidc_clients::Model,
) -> Result<ClientDto, AppError> {
    let n = authorization_count(db, m.id).await?;
    Ok(ClientDto::from_model(m, n))
}

/// 创建/轮换结果：`client_secret` 只在这一次响应中出现。
#[derive(Debug, Serialize)]
pub struct ClientWithSecretDto {
    pub client: ClientDto,
    pub client_secret: Option<String>,
}

/// 接入所需的公开 OP 信息（开发者中心"复制配置"）。
#[derive(Debug, Serialize)]
pub struct ProviderInfoDto {
    pub issuer: String,
    pub discovery_url: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub userinfo_endpoint: String,
    pub jwks_uri: String,
    pub scopes_supported: Vec<&'static str>,
    pub require_client_approval: bool,
    pub allow_loopback_redirect_uris: bool,
    /// 部署开启标准退出时的 `end_session_endpoint`（否则为空）。
    pub end_session_endpoint: Option<String>,
    /// 部署是否支持 Back-Channel Logout。
    pub backchannel_logout_supported: bool,
}

#[derive(Debug, Deserialize)]
pub struct ClientInputDto {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub homepage_url: Option<String>,
    #[serde(default)]
    pub logo_url: Option<String>,
    pub redirect_uris: Vec<String>,
    pub scopes: Vec<String>,
    /// 省略 = 保持原值（旧版前端的整体 PATCH 不会清空）。
    #[serde(default)]
    pub post_logout_redirect_uris: Option<Vec<String>>,
    /// 省略 = 保持原值；空字符串 = 清除。
    #[serde(default)]
    pub backchannel_logout_uri: Option<String>,
}

impl From<ClientInputDto> for ClientInput {
    fn from(d: ClientInputDto) -> Self {
        Self {
            name: d.name,
            description: d.description,
            homepage_url: d.homepage_url,
            logo_url: d.logo_url,
            redirect_uris: d.redirect_uris,
            scopes: d.scopes,
            post_logout_redirect_uris: d.post_logout_redirect_uris,
            backchannel_logout_uri: d.backchannel_logout_uri,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateClientDto {
    pub client_type: OidcClientType,
    #[serde(default)]
    pub token_endpoint_auth_method: Option<OidcTokenAuthMethod>,
    #[serde(flatten)]
    pub input: ClientInputDto,
}

#[derive(Debug, Deserialize, Default)]
pub struct RotateSecretDto {
    /// true = 旧 secret 立即失效（怀疑泄露时使用）；默认保留配置的宽限期。
    #[serde(default)]
    pub revoke_previous: bool,
}

// ── 开发者：我的应用 ────────────────────────────────────────────────────────

/// GET /api/oidc/provider
#[get("/provider")]
pub async fn provider_info(
    _user: UserJwtGuard,
    oidc: Option<WebOidc>,
) -> UniResult<ProviderInfoDto> {
    let oidc = require(oidc)?;
    let issuer = oidc.issuer();
    Ok(UniResponse::ok(Some(ProviderInfoDto {
        issuer: issuer.to_string(),
        discovery_url: format!("{issuer}{}", metadata::PATH_DISCOVERY),
        authorization_endpoint: format!("{issuer}{}", metadata::PATH_AUTHORIZE),
        token_endpoint: format!("{issuer}{}", metadata::PATH_TOKEN),
        userinfo_endpoint: format!("{issuer}{}", metadata::PATH_USERINFO),
        jwks_uri: format!("{issuer}{}", metadata::PATH_JWKS),
        scopes_supported: scope::SUPPORTED.to_vec(),
        require_client_approval: oidc.config.require_client_approval,
        allow_loopback_redirect_uris: oidc.config.allow_loopback_redirect_uris,
        end_session_endpoint: oidc
            .config
            .backchannel_logout
            .then(|| format!("{issuer}{}", metadata::PATH_END_SESSION)),
        backchannel_logout_supported: oidc.config.backchannel_logout,
    })))
}

/// GET /api/oidc/clients
#[get("/clients")]
pub async fn list_my_clients(
    user: UserJwtGuard,
    oidc: Option<WebOidc>,
    db: WebDb,
) -> UniResult<Vec<ClientDto>> {
    require(oidc)?;
    let user = user.into_inner();
    let mut out = Vec::new();
    for m in clients::list_owned(&db, user.id).await? {
        out.push(dto(&db, m).await?);
    }
    Ok(UniResponse::ok(Some(out)))
}

/// POST /api/oidc/clients
#[post("/clients")]
pub async fn create_client(
    req: HttpRequest,
    user: UserJwtGuard,
    oidc: Option<WebOidc>,
    db: WebDb,
    log: WebLog,
    body: web::Json<CreateClientDto>,
) -> UniResult<ClientWithSecretDto> {
    let oidc = require(oidc)?;
    let user = user.into_inner();
    rate(
        &oidc,
        "oidc_client_create",
        &user.id.to_string(),
        20,
        86_400,
    )
    .await?;
    let body = body.into_inner();
    let (model, secret) = clients::create(
        &db,
        &oidc.config,
        user.id,
        body.client_type,
        body.token_endpoint_auth_method,
        &body.input.into(),
    )
    .await?;
    audit(
        &log,
        &req,
        "OIDC_CLIENT_CREATE",
        format!("{} 创建 OIDC 应用 {}", user.username, model.name),
        &model.client_id,
        Some(user.id),
        None,
    );
    Ok(UniResponse::ok(Some(ClientWithSecretDto {
        client: ClientDto::from_model(model, 0),
        client_secret: secret,
    })))
}

async fn owned(
    db: &WebDb,
    user: &users::Model,
    client_id: &str,
) -> Result<oidc_clients::Model, AppError> {
    clients::find_owned(db, user.id, client_id)
        .await?
        .ok_or_else(|| AppError::NotFound("应用不存在".into()))
}

/// GET /api/oidc/clients/{client_id}
#[get("/clients/{client_id}")]
pub async fn get_my_client(
    user: UserJwtGuard,
    oidc: Option<WebOidc>,
    db: WebDb,
    path: web::Path<String>,
) -> UniResult<ClientDto> {
    require(oidc)?;
    let user = user.into_inner();
    let m = owned(&db, &user, &path).await?;
    Ok(UniResponse::ok(Some(dto(&db, m).await?)))
}

/// PATCH /api/oidc/clients/{client_id}（整体替换可编辑字段）
#[patch("/clients/{client_id}")]
pub async fn update_my_client(
    req: HttpRequest,
    user: UserJwtGuard,
    oidc: Option<WebOidc>,
    db: WebDb,
    log: WebLog,
    path: web::Path<String>,
    body: web::Json<ClientInputDto>,
) -> UniResult<ClientDto> {
    let oidc = require(oidc)?;
    let user = user.into_inner();
    let m = owned(&db, &user, &path).await?;
    let m = clients::update(&db, &oidc.config, m, &body.into_inner().into()).await?;
    audit(
        &log,
        &req,
        "OIDC_CLIENT_UPDATE",
        format!("{} 修改 OIDC 应用 {}", user.username, m.name),
        &m.client_id,
        Some(user.id),
        None,
    );
    Ok(UniResponse::ok(Some(dto(&db, m).await?)))
}

/// POST /api/oidc/clients/{client_id}/secret —— 轮换 client secret（新值只返回一次）
#[post("/clients/{client_id}/secret")]
pub async fn rotate_my_secret(
    req: HttpRequest,
    user: UserJwtGuard,
    oidc: Option<WebOidc>,
    db: WebDb,
    log: WebLog,
    path: web::Path<String>,
    body: Option<web::Json<RotateSecretDto>>,
) -> UniResult<ClientWithSecretDto> {
    let oidc = require(oidc)?;
    let user = user.into_inner();
    rate(&oidc, "oidc_secret_rotate", &user.id.to_string(), 20, 3600).await?;
    let m = owned(&db, &user, &path).await?;
    let revoke_previous = body.map(|b| b.revoke_previous).unwrap_or(false);
    let grace = if revoke_previous {
        0
    } else {
        oidc.config.secret_rotation_grace_secs
    };
    let (m, secret) = clients::rotate_secret(&db, m, grace).await?;
    audit(
        &log,
        &req,
        "OIDC_SECRET_ROTATE",
        format!(
            "{} 轮换 OIDC 应用 {} 的 secret（旧 secret 宽限 {grace}s）",
            user.username, m.name
        ),
        &m.client_id,
        Some(user.id),
        None,
    );
    Ok(UniResponse::ok(Some(ClientWithSecretDto {
        client: dto(&db, m).await?,
        client_secret: Some(secret),
    })))
}

async fn owner_transition(
    req: HttpRequest,
    user: UserJwtGuard,
    oidc: Option<WebOidc>,
    db: WebDb,
    log: WebLog,
    client_id: String,
    action: OwnerAction,
) -> UniResult<ClientDto> {
    require(oidc)?;
    let user = user.into_inner();
    let m = owned(&db, &user, &client_id).await?;
    let next = clients::owner_transition(&m.status, action).map_err(AppError::InvalidState)?;
    let m = clients::set_status(&db, m, next, None, None).await?;
    let (code, verb) = match action {
        OwnerAction::Disable => ("OIDC_CLIENT_DISABLE", "暂停"),
        OwnerAction::Enable => ("OIDC_CLIENT_ENABLE", "恢复"),
    };
    audit(
        &log,
        &req,
        code,
        format!("{} {verb} OIDC 应用 {}", user.username, m.name),
        &m.client_id,
        Some(user.id),
        None,
    );
    Ok(UniResponse::ok(Some(dto(&db, m).await?)))
}

/// POST /api/oidc/clients/{client_id}/disable
#[post("/clients/{client_id}/disable")]
pub async fn disable_my_client(
    req: HttpRequest,
    user: UserJwtGuard,
    oidc: Option<WebOidc>,
    db: WebDb,
    log: WebLog,
    path: web::Path<String>,
) -> UniResult<ClientDto> {
    owner_transition(
        req,
        user,
        oidc,
        db,
        log,
        path.into_inner(),
        OwnerAction::Disable,
    )
    .await
}

/// POST /api/oidc/clients/{client_id}/enable
#[post("/clients/{client_id}/enable")]
pub async fn enable_my_client(
    req: HttpRequest,
    user: UserJwtGuard,
    oidc: Option<WebOidc>,
    db: WebDb,
    log: WebLog,
    path: web::Path<String>,
) -> UniResult<ClientDto> {
    owner_transition(
        req,
        user,
        oidc,
        db,
        log,
        path.into_inner(),
        OwnerAction::Enable,
    )
    .await
}

/// DELETE /api/oidc/clients/{client_id}（授权、令牌、授权码级联删除）
#[delete("/clients/{client_id}")]
pub async fn delete_my_client(
    req: HttpRequest,
    user: UserJwtGuard,
    oidc: Option<WebOidc>,
    db: WebDb,
    log: WebLog,
    path: web::Path<String>,
) -> UniResult<()> {
    require(oidc)?;
    let user = user.into_inner();
    let m = owned(&db, &user, &path).await?;
    let (client_id, name) = (m.client_id.clone(), m.name.clone());
    oidc_clients::Entity::delete_by_id(m.id)
        .exec(db.get_ref())
        .await?;
    audit(
        &log,
        &req,
        "OIDC_CLIENT_DELETE",
        format!("{} 删除 OIDC 应用 {name}", user.username),
        &client_id,
        Some(user.id),
        None,
    );
    Ok(UniResponse::ok_none())
}

// ── 用户：已授权应用 ────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct AuthorizationDto {
    pub client_id: String,
    pub client_name: String,
    pub homepage_url: Option<String>,
    pub client_status: OidcClientStatus,
    pub scopes: Vec<String>,
    pub granted_at: DateTime<FixedOffset>,
    pub updated_at: DateTime<FixedOffset>,
    pub last_used_at: Option<DateTime<FixedOffset>>,
}

/// GET /api/oidc/authorizations
#[get("/authorizations")]
pub async fn list_my_authorizations(
    user: UserJwtGuard,
    oidc: Option<WebOidc>,
    db: WebDb,
) -> UniResult<Vec<AuthorizationDto>> {
    require(oidc)?;
    let user = user.into_inner();
    let rows = store::list_user_consents(&db, user.id).await?;
    Ok(UniResponse::ok(Some(
        rows.into_iter()
            .map(|(c, client)| AuthorizationDto {
                client_id: client.client_id,
                client_name: client.name,
                homepage_url: client.homepage_url,
                client_status: client.status,
                scopes: scope::from_json(&c.scopes),
                granted_at: c.granted_at,
                updated_at: c.updated_at,
                last_used_at: c.last_used_at,
            })
            .collect(),
    )))
}

/// DELETE /api/oidc/authorizations/{client_id} —— 撤销授权：已签发 access token 立即失效
#[delete("/authorizations/{client_id}")]
pub async fn revoke_my_authorization(
    req: HttpRequest,
    user: UserJwtGuard,
    oidc: Option<WebOidc>,
    db: WebDb,
    log: WebLog,
    path: web::Path<String>,
) -> UniResult<()> {
    require(oidc)?;
    let user = user.into_inner();
    let client = clients::find_by_client_id(&db, &path)
        .await?
        .ok_or_else(|| AppError::NotFound("授权不存在".into()))?;
    if !store::revoke_consent(&db, user.id, client.id).await? {
        return Err(AppError::NotFound("授权不存在".into()));
    }
    audit(
        &log,
        &req,
        "OIDC_CONSENT_REVOKE",
        format!("{} 撤销对 {} 的授权", user.username, client.name),
        &client.client_id,
        Some(user.id),
        None,
    );
    Ok(UniResponse::ok_none())
}

// ── 管理员：审核 ────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct AdminClientDto {
    #[serde(flatten)]
    pub client: ClientDto,
    pub owner_user_id: uuid::Uuid,
    pub owner_username: Option<String>,
    pub reviewed_at: Option<DateTime<FixedOffset>>,
}

#[derive(Debug, Deserialize)]
pub struct AdminListQuery {
    #[serde(default)]
    pub status: Option<OidcClientStatus>,
}

/// GET /api/admin/oidc/clients?status=pending
#[get("/clients")]
pub async fn admin_list_clients(
    _admin: SuperAdminJwtGuard,
    oidc: Option<WebOidc>,
    db: WebDb,
    query: web::Query<AdminListQuery>,
) -> UniResult<Vec<AdminClientDto>> {
    require(oidc)?;
    let mut select = oidc_clients::Entity::find().order_by_desc(oidc_clients::Column::CreatedAt);
    if let Some(status) = query.into_inner().status {
        select = select.filter(oidc_clients::Column::Status.eq(status));
    }
    let rows = select
        .find_also_related(users::Entity)
        .all(db.get_ref())
        .await?;
    let mut out = Vec::with_capacity(rows.len());
    for (m, owner) in rows {
        let owner_user_id = m.owner_user_id;
        let reviewed_at = m.reviewed_at;
        out.push(AdminClientDto {
            client: dto(&db, m).await?,
            owner_user_id,
            owner_username: owner.map(|o| o.username),
            reviewed_at,
        });
    }
    Ok(UniResponse::ok(Some(out)))
}

#[derive(Debug, Deserialize, Default)]
pub struct ReviewDto {
    #[serde(default)]
    pub reason: Option<String>,
}

#[allow(clippy::too_many_arguments)]
async fn admin_transition(
    req: HttpRequest,
    admin: SuperAdminJwtGuard,
    oidc: Option<WebOidc>,
    db: WebDb,
    log: WebLog,
    client_id: String,
    body: Option<web::Json<ReviewDto>>,
    action: AdminAction,
) -> UniResult<ClientDto> {
    require(oidc)?;
    let admin = admin.into_inner();
    let m = clients::find_by_client_id(&db, &client_id)
        .await?
        .ok_or_else(|| AppError::NotFound("应用不存在".into()))?;
    let next = clients::admin_transition(&m.status, action).map_err(AppError::InvalidState)?;
    let reason = body.and_then(|b| b.into_inner().reason);
    let m = clients::set_status(&db, m, next, Some(admin.id), reason).await?;
    let code = match action {
        AdminAction::Approve => "OIDC_CLIENT_APPROVE",
        AdminAction::Reject => "OIDC_CLIENT_REJECT",
        AdminAction::Suspend => "OIDC_CLIENT_SUSPEND",
        AdminAction::Reinstate => "OIDC_CLIENT_REINSTATE",
    };
    audit(
        &log,
        &req,
        code,
        format!("管理员 {:?} OIDC 应用 {}", action, m.name),
        &m.client_id,
        None,
        Some(admin.id),
    );
    Ok(UniResponse::ok(Some(dto(&db, m).await?)))
}

/// POST /api/admin/oidc/clients/{client_id}/approve
#[post("/clients/{client_id}/approve")]
pub async fn admin_approve(
    req: HttpRequest,
    admin: SuperAdminJwtGuard,
    oidc: Option<WebOidc>,
    db: WebDb,
    log: WebLog,
    path: web::Path<String>,
    body: Option<web::Json<ReviewDto>>,
) -> UniResult<ClientDto> {
    admin_transition(
        req,
        admin,
        oidc,
        db,
        log,
        path.into_inner(),
        body,
        AdminAction::Approve,
    )
    .await
}

/// POST /api/admin/oidc/clients/{client_id}/reject
#[post("/clients/{client_id}/reject")]
pub async fn admin_reject(
    req: HttpRequest,
    admin: SuperAdminJwtGuard,
    oidc: Option<WebOidc>,
    db: WebDb,
    log: WebLog,
    path: web::Path<String>,
    body: Option<web::Json<ReviewDto>>,
) -> UniResult<ClientDto> {
    admin_transition(
        req,
        admin,
        oidc,
        db,
        log,
        path.into_inner(),
        body,
        AdminAction::Reject,
    )
    .await
}

/// POST /api/admin/oidc/clients/{client_id}/suspend
#[post("/clients/{client_id}/suspend")]
pub async fn admin_suspend(
    req: HttpRequest,
    admin: SuperAdminJwtGuard,
    oidc: Option<WebOidc>,
    db: WebDb,
    log: WebLog,
    path: web::Path<String>,
    body: Option<web::Json<ReviewDto>>,
) -> UniResult<ClientDto> {
    admin_transition(
        req,
        admin,
        oidc,
        db,
        log,
        path.into_inner(),
        body,
        AdminAction::Suspend,
    )
    .await
}

/// POST /api/admin/oidc/clients/{client_id}/reinstate
#[post("/clients/{client_id}/reinstate")]
pub async fn admin_reinstate(
    req: HttpRequest,
    admin: SuperAdminJwtGuard,
    oidc: Option<WebOidc>,
    db: WebDb,
    log: WebLog,
    path: web::Path<String>,
    body: Option<web::Json<ReviewDto>>,
) -> UniResult<ClientDto> {
    admin_transition(
        req,
        admin,
        oidc,
        db,
        log,
        path.into_inner(),
        body,
        AdminAction::Reinstate,
    )
    .await
}

#[derive(Debug, Deserialize)]
pub struct PkcePolicyDto {
    pub require_pkce: bool,
}

/// PATCH /api/admin/oidc/clients/{client_id}/pkce —— 仅机密客户端可豁免 PKCE（豁免后必须用 nonce）
#[patch("/clients/{client_id}/pkce")]
pub async fn admin_set_pkce(
    req: HttpRequest,
    admin: SuperAdminJwtGuard,
    oidc: Option<WebOidc>,
    db: WebDb,
    log: WebLog,
    path: web::Path<String>,
    body: web::Json<PkcePolicyDto>,
) -> UniResult<ClientDto> {
    require(oidc)?;
    let admin = admin.into_inner();
    let m = clients::find_by_client_id(&db, &path)
        .await?
        .ok_or_else(|| AppError::NotFound("应用不存在".into()))?;
    if m.client_type == OidcClientType::Public && !body.require_pkce {
        return Err(AppError::Validation("公开客户端必须强制 PKCE".into()));
    }
    let mut active = m.into_active_model();
    active.require_pkce = Set(body.require_pkce);
    active.updated_at = Set(chrono::Utc::now().fixed_offset());
    let m = active.update(db.get_ref()).await?;
    audit(
        &log,
        &req,
        "OIDC_CLIENT_PKCE",
        format!(
            "管理员设置 OIDC 应用 {} require_pkce={}",
            m.name, m.require_pkce
        ),
        &m.client_id,
        None,
        Some(admin.id),
    );
    Ok(UniResponse::ok(Some(dto(&db, m).await?)))
}
