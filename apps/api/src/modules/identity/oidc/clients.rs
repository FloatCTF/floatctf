//! OIDC 应用（client）注册模型：输入校验、创建、修改、secret 轮换、状态机。
//!
//! 权限（谁能调用）由 API 层保证：所有者接口按 `owner_user_id` 过滤，审批/封禁只走超管守卫。
//! 本模块保证数据不变量：public ⇔ `none` 且无 secret 且强制 PKCE；confidential 必有 secret。

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait,
    IntoActiveModel, QueryFilter, QueryOrder,
};
use uuid::Uuid;

use crate::api::AppError;
use crate::core::config::OidcConfig;
use crate::entity::oidc_clients;
use crate::entity::sea_orm_active_enums::{OidcClientStatus, OidcClientType, OidcTokenAuthMethod};

use super::domain::{redirect_uri, scope, secret};

pub const MAX_NAME_CHARS: usize = 64;
pub const MAX_DESCRIPTION_CHARS: usize = 500;

/// 所有者可编辑的字段。
///
/// 标准退出相关字段为 `Option`：`None` = 保持原值（旧版前端的整体 PATCH 不会把它们清空），
/// `Some` = 替换；`backchannel_logout_uri = Some("")` 表示清除。
#[derive(Debug, Clone, Default)]
pub struct ClientInput {
    pub name: String,
    pub description: Option<String>,
    pub homepage_url: Option<String>,
    pub logo_url: Option<String>,
    pub redirect_uris: Vec<String>,
    pub scopes: Vec<String>,
    pub post_logout_redirect_uris: Option<Vec<String>>,
    pub backchannel_logout_uri: Option<String>,
}

/// 校验并规范化后的输入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedInput {
    pub name: String,
    pub description: Option<String>,
    pub homepage_url: Option<String>,
    pub logo_url: Option<String>,
    pub redirect_uris: Vec<String>,
    pub scopes: Vec<String>,
    /// `None` = 保持原值。
    pub post_logout_redirect_uris: Option<Vec<String>>,
    /// `None` = 保持原值；`Some(None)` = 清除。
    pub backchannel_logout_uri: Option<Option<String>>,
}

pub fn validate_input(
    input: &ClientInput,
    allow_loopback: bool,
) -> Result<NormalizedInput, String> {
    let name = input.name.trim();
    if name.is_empty() || name.chars().count() > MAX_NAME_CHARS {
        return Err(format!(
            "应用名称长度必须在 1..={MAX_NAME_CHARS} 个字符之间"
        ));
    }
    if name.chars().any(char::is_control) {
        return Err("应用名称不得包含控制字符".into());
    }
    let description = non_empty(&input.description);
    if description
        .as_ref()
        .is_some_and(|d| d.chars().count() > MAX_DESCRIPTION_CHARS)
    {
        return Err(format!("应用简介最长 {MAX_DESCRIPTION_CHARS} 个字符"));
    }
    let homepage_url = non_empty(&input.homepage_url);
    if let Some(url) = &homepage_url {
        validate_web_url(url, allow_loopback).map_err(|e| format!("应用主页{e}"))?;
    }
    let logo_url = non_empty(&input.logo_url);
    if let Some(url) = &logo_url {
        validate_web_url(url, false).map_err(|e| format!("应用图标{e}"))?;
    }

    let mut redirect_uris: Vec<String> = Vec::new();
    for uri in &input.redirect_uris {
        let uri = uri.trim();
        redirect_uri::validate_registration(uri, allow_loopback)?;
        if !redirect_uris.iter().any(|u| u == uri) {
            redirect_uris.push(uri.to_string());
        }
    }
    if redirect_uris.is_empty() {
        return Err("至少需要一个回调地址".into());
    }
    if redirect_uris.len() > redirect_uri::MAX_REDIRECT_URIS {
        return Err(format!(
            "回调地址最多 {} 个",
            redirect_uri::MAX_REDIRECT_URIS
        ));
    }
    let scopes = scope::normalize_client_scopes(&input.scopes)?;

    let post_logout_redirect_uris = match &input.post_logout_redirect_uris {
        None => None,
        Some(list) => {
            let mut uris: Vec<String> = Vec::new();
            for uri in list {
                let uri = uri.trim();
                redirect_uri::validate_registration(uri, allow_loopback)
                    .map_err(|e| format!("退出后回跳地址：{e}"))?;
                if !uris.iter().any(|u| u == uri) {
                    uris.push(uri.to_string());
                }
            }
            if uris.len() > redirect_uri::MAX_REDIRECT_URIS {
                return Err(format!(
                    "退出后回跳地址最多 {} 个",
                    redirect_uri::MAX_REDIRECT_URIS
                ));
            }
            Some(uris)
        }
    };
    let backchannel_logout_uri = match input.backchannel_logout_uri.as_deref().map(str::trim) {
        None => None,
        Some("") => Some(None),
        Some(uri) => {
            validate_backchannel_uri(uri, allow_loopback)
                .map_err(|e| format!("后端退出通知地址{e}"))?;
            Some(Some(uri.to_string()))
        }
    };

    Ok(NormalizedInput {
        name: name.to_string(),
        description,
        homepage_url,
        logo_url,
        redirect_uris,
        scopes,
        post_logout_redirect_uris,
        backchannel_logout_uri,
    })
}

/// Back-Channel Logout 接收地址（登记期校验）：绝对 https URL，无 userinfo / fragment；
/// http 只允许 loopback 且部署显式允许。目标地址的网段策略在**投递时**按解析结果再校验（防 DNS 重绑定）。
pub fn validate_backchannel_uri(uri: &str, allow_loopback: bool) -> Result<(), String> {
    if uri.len() > redirect_uri::MAX_REDIRECT_URI_LEN {
        return Err("过长".into());
    }
    let parsed = url::Url::parse(uri).map_err(|_| "不是合法的绝对 URL".to_string())?;
    let host = parsed.host_str().ok_or("缺少主机名")?;
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("不得包含用户名或密码".into());
    }
    if parsed.fragment().is_some() {
        return Err("不得包含 #fragment".into());
    }
    match parsed.scheme() {
        "https" => Ok(()),
        "http" if allow_loopback && crate::core::config::is_loopback_host(host) => Ok(()),
        _ => Err("必须使用 https".into()),
    }
}

fn non_empty(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
}

fn validate_web_url(url: &str, allow_loopback: bool) -> Result<(), String> {
    if url.len() > redirect_uri::MAX_REDIRECT_URI_LEN {
        return Err("地址过长".into());
    }
    let parsed = url::Url::parse(url).map_err(|_| "不是合法的绝对 URL".to_string())?;
    let host = parsed.host_str().ok_or("缺少主机名")?;
    match parsed.scheme() {
        "https" => Ok(()),
        "http" if allow_loopback && crate::core::config::is_loopback_host(host) => Ok(()),
        _ => Err("必须使用 https".into()),
    }
}

/// 解析客户端类型与认证方式。
pub fn resolve_auth_method(
    client_type: &OidcClientType,
    requested: Option<OidcTokenAuthMethod>,
) -> Result<OidcTokenAuthMethod, String> {
    match (client_type, requested) {
        (OidcClientType::Public, None | Some(OidcTokenAuthMethod::None)) => {
            Ok(OidcTokenAuthMethod::None)
        }
        (OidcClientType::Public, Some(_)) => Err("公开客户端只能使用 none（不持有 secret）".into()),
        (OidcClientType::Confidential, None) => Ok(OidcTokenAuthMethod::ClientSecretBasic),
        (OidcClientType::Confidential, Some(OidcTokenAuthMethod::None)) => {
            Err("机密客户端必须使用 client_secret_basic 或 client_secret_post".into())
        }
        (OidcClientType::Confidential, Some(method)) => Ok(method),
    }
}

/// 所有者动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnerAction {
    Disable,
    Enable,
}

/// 管理员动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdminAction {
    Approve,
    Reject,
    Suspend,
    Reinstate,
}

pub fn owner_transition(
    status: &OidcClientStatus,
    action: OwnerAction,
) -> Result<OidcClientStatus, String> {
    use OidcClientStatus::*;
    match (status, action) {
        (Active, OwnerAction::Disable) => Ok(Disabled),
        (Disabled, OwnerAction::Enable) => Ok(Active),
        (Suspended, _) => Err("应用已被管理员封禁，只能由管理员恢复".into()),
        (Pending, _) => Err("应用仍在等待管理员审批".into()),
        (Rejected, _) => Err("应用审批未通过".into()),
        _ => Err("当前状态不支持该操作".into()),
    }
}

pub fn admin_transition(
    status: &OidcClientStatus,
    action: AdminAction,
) -> Result<OidcClientStatus, String> {
    use OidcClientStatus::*;
    match (status, action) {
        (Pending | Rejected, AdminAction::Approve) => Ok(Active),
        (Pending, AdminAction::Reject) => Ok(Rejected),
        (Pending | Active | Disabled, AdminAction::Suspend) => Ok(Suspended),
        (Suspended, AdminAction::Reinstate) => Ok(Active),
        _ => Err("当前状态不支持该操作".into()),
    }
}

/// 校验 client secret：当前 secret，或宽限期内的上一把 secret。
pub fn verify_secret(client: &oidc_clients::Model, presented: &str) -> bool {
    if client.client_type != OidcClientType::Confidential {
        return false;
    }
    if let Some(current) = &client.secret_hash
        && secret::verify_client_secret(presented, current)
    {
        return true;
    }
    match (
        &client.previous_secret_hash,
        &client.previous_secret_expires_at,
    ) {
        (Some(previous), Some(expires_at)) if Utc::now() < expires_at.with_timezone(&Utc) => {
            secret::verify_client_secret(presented, previous)
        }
        _ => false,
    }
}

pub fn redirect_uris_of(client: &oidc_clients::Model) -> Vec<String> {
    scope::from_json(&client.redirect_uris)
}

pub fn scopes_of(client: &oidc_clients::Model) -> Vec<String> {
    scope::from_json(&client.allowed_scopes)
}

pub fn post_logout_redirect_uris_of(client: &oidc_clients::Model) -> Vec<String> {
    scope::from_json(&client.post_logout_redirect_uris)
}

// ── 持久化 ──────────────────────────────────────────────────────────────────

/// 创建应用；机密客户端返回仅此一次可见的 secret 明文。
pub async fn create(
    db: &DatabaseConnection,
    config: &OidcConfig,
    owner_user_id: Uuid,
    client_type: OidcClientType,
    auth_method: Option<OidcTokenAuthMethod>,
    input: &ClientInput,
) -> Result<(oidc_clients::Model, Option<String>), AppError> {
    let normalized =
        validate_input(input, config.allow_loopback_redirect_uris).map_err(AppError::Validation)?;
    let auth_method =
        resolve_auth_method(&client_type, auth_method).map_err(AppError::Validation)?;
    let (secret_plain, secret_hash) = if client_type == OidcClientType::Confidential {
        let plain = secret::new_client_secret();
        let hash = secret::hash_client_secret(&plain).map_err(AppError::Internal)?;
        (Some(plain), Some(hash))
    } else {
        (None, None)
    };
    let now = Utc::now().fixed_offset();
    let status = if config.require_client_approval {
        OidcClientStatus::Pending
    } else {
        OidcClientStatus::Active
    };
    let model = oidc_clients::ActiveModel {
        id: Set(Uuid::new_v4()),
        client_id: Set(secret::new_client_id()),
        owner_user_id: Set(owner_user_id),
        name: Set(normalized.name),
        description: Set(normalized.description),
        homepage_url: Set(normalized.homepage_url),
        logo_url: Set(normalized.logo_url),
        client_type: Set(client_type),
        token_endpoint_auth_method: Set(auth_method),
        redirect_uris: Set(scope::to_json(&normalized.redirect_uris)),
        allowed_scopes: Set(scope::to_json(&normalized.scopes)),
        post_logout_redirect_uris: Set(scope::to_json(
            &normalized.post_logout_redirect_uris.unwrap_or_default(),
        )),
        backchannel_logout_uri: Set(normalized.backchannel_logout_uri.flatten()),
        require_pkce: Set(true),
        secret_hash: Set(secret_hash),
        previous_secret_hash: Set(None),
        previous_secret_expires_at: Set(None),
        secret_rotated_at: Set(None),
        status: Set(status),
        status_reason: Set(None),
        reviewed_by: Set(None),
        reviewed_at: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(db)
    .await?;
    Ok((model, secret_plain))
}

/// 修改所有者可编辑字段（不改变类型、认证方式与状态）。
pub async fn update(
    db: &DatabaseConnection,
    config: &OidcConfig,
    client: oidc_clients::Model,
    input: &ClientInput,
) -> Result<oidc_clients::Model, AppError> {
    let normalized =
        validate_input(input, config.allow_loopback_redirect_uris).map_err(AppError::Validation)?;
    let mut active = client.into_active_model();
    active.name = Set(normalized.name);
    active.description = Set(normalized.description);
    active.homepage_url = Set(normalized.homepage_url);
    active.logo_url = Set(normalized.logo_url);
    active.redirect_uris = Set(scope::to_json(&normalized.redirect_uris));
    active.allowed_scopes = Set(scope::to_json(&normalized.scopes));
    if let Some(uris) = normalized.post_logout_redirect_uris {
        active.post_logout_redirect_uris = Set(scope::to_json(&uris));
    }
    if let Some(uri) = normalized.backchannel_logout_uri {
        active.backchannel_logout_uri = Set(uri);
    }
    active.updated_at = Set(Utc::now().fixed_offset());
    Ok(active.update(db).await?)
}

/// 轮换机密客户端 secret。`grace_secs > 0` 时旧 secret 在宽限期内仍可用，`0` 表示立即作废。
pub async fn rotate_secret(
    db: &DatabaseConnection,
    client: oidc_clients::Model,
    grace_secs: i64,
) -> Result<(oidc_clients::Model, String), AppError> {
    if client.client_type != OidcClientType::Confidential {
        return Err(AppError::Validation("公开客户端没有 client secret".into()));
    }
    let plain = secret::new_client_secret();
    let hash = secret::hash_client_secret(&plain).map_err(AppError::Internal)?;
    let now = Utc::now();
    let previous = client.secret_hash.clone();
    let mut active = client.into_active_model();
    if grace_secs > 0 && previous.is_some() {
        active.previous_secret_hash = Set(previous);
        active.previous_secret_expires_at = Set(Some(
            (now + chrono::Duration::seconds(grace_secs)).fixed_offset(),
        ));
    } else {
        active.previous_secret_hash = Set(None);
        active.previous_secret_expires_at = Set(None);
    }
    active.secret_hash = Set(Some(hash));
    active.secret_rotated_at = Set(Some(now.fixed_offset()));
    active.updated_at = Set(now.fixed_offset());
    Ok((active.update(db).await?, plain))
}

pub async fn set_status(
    db: &DatabaseConnection,
    client: oidc_clients::Model,
    status: OidcClientStatus,
    reviewer: Option<Uuid>,
    reason: Option<String>,
) -> Result<oidc_clients::Model, AppError> {
    let now = Utc::now().fixed_offset();
    let mut active = client.into_active_model();
    active.status = Set(status);
    if let Some(reviewer) = reviewer {
        active.reviewed_by = Set(Some(reviewer));
        active.reviewed_at = Set(Some(now));
        active.status_reason = Set(reason.map(|r| r.trim().chars().take(500).collect()));
    }
    active.updated_at = Set(now);
    Ok(active.update(db).await?)
}

pub async fn find_by_client_id(
    db: &DatabaseConnection,
    client_id: &str,
) -> Result<Option<oidc_clients::Model>, AppError> {
    if client_id.is_empty() || client_id.len() > 128 {
        return Ok(None);
    }
    Ok(oidc_clients::Entity::find()
        .filter(oidc_clients::Column::ClientId.eq(client_id))
        .one(db)
        .await?)
}

/// 所有者视角查找：不属于该用户时返回 `None`（API 层统一 404，不暴露存在性）。
pub async fn find_owned(
    db: &DatabaseConnection,
    owner_user_id: Uuid,
    client_id: &str,
) -> Result<Option<oidc_clients::Model>, AppError> {
    Ok(find_by_client_id(db, client_id)
        .await?
        .filter(|c| c.owner_user_id == owner_user_id))
}

pub async fn list_owned(
    db: &DatabaseConnection,
    owner_user_id: Uuid,
) -> Result<Vec<oidc_clients::Model>, AppError> {
    Ok(oidc_clients::Entity::find()
        .filter(oidc_clients::Column::OwnerUserId.eq(owner_user_id))
        .order_by_desc(oidc_clients::Column::CreatedAt)
        .all(db)
        .await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> ClientInput {
        ClientInput {
            name: "  LearnOS  ".into(),
            description: Some("".into()),
            homepage_url: Some("https://learnos.example.com".into()),
            logo_url: None,
            redirect_uris: vec![
                "https://learnos.example.com/auth/callback".into(),
                "https://learnos.example.com/auth/callback".into(),
            ],
            scopes: vec!["email".into(), "openid".into()],
            ..ClientInput::default()
        }
    }

    #[test]
    fn logout_fields_are_validated_and_absent_means_unchanged() {
        let n = validate_input(&input(), false).unwrap();
        assert_eq!(n.post_logout_redirect_uris, None, "未提供 = 保持原值");
        assert_eq!(n.backchannel_logout_uri, None);

        let with = ClientInput {
            post_logout_redirect_uris: Some(vec![
                "https://learnos.example.com/auth/signed-out".into(),
                "https://learnos.example.com/auth/signed-out".into(),
            ]),
            backchannel_logout_uri: Some(
                "https://learnos.example.com/api/oidc/backchannel-logout".into(),
            ),
            ..input()
        };
        let n = validate_input(&with, false).unwrap();
        assert_eq!(
            n.post_logout_redirect_uris.unwrap(),
            vec!["https://learnos.example.com/auth/signed-out".to_string()]
        );
        assert_eq!(
            n.backchannel_logout_uri,
            Some(Some(
                "https://learnos.example.com/api/oidc/backchannel-logout".into()
            ))
        );

        let cleared = ClientInput {
            backchannel_logout_uri: Some("  ".into()),
            ..input()
        };
        assert_eq!(
            validate_input(&cleared, false)
                .unwrap()
                .backchannel_logout_uri,
            Some(None)
        );

        for bad in [
            "http://learnos.example.com/bc",
            "https://user:pw@learnos.example.com/bc",
            "https://learnos.example.com/bc#x",
            "javascript:alert(1)",
            "/relative",
        ] {
            let i = ClientInput {
                backchannel_logout_uri: Some(bad.into()),
                ..input()
            };
            assert!(validate_input(&i, false).is_err(), "{bad}");
        }
        assert!(validate_backchannel_uri("http://127.0.0.1:3000/bc", true).is_ok());
        assert!(validate_backchannel_uri("http://127.0.0.1:3000/bc", false).is_err());
        let bad_post = ClientInput {
            post_logout_redirect_uris: Some(vec!["https://*.example.com/x".into()]),
            ..input()
        };
        assert!(validate_input(&bad_post, false).is_err());
    }

    #[test]
    fn input_is_normalized() {
        let n = validate_input(&input(), false).unwrap();
        assert_eq!(n.name, "LearnOS");
        assert_eq!(n.description, None);
        assert_eq!(n.redirect_uris.len(), 1);
        assert_eq!(n.scopes, vec!["openid", "email"]);
    }

    #[test]
    fn input_rejections() {
        let mut i = input();
        i.name = " ".into();
        assert!(validate_input(&i, false).is_err());
        let mut i = input();
        i.redirect_uris = vec![];
        assert!(validate_input(&i, false).is_err());
        let mut i = input();
        i.redirect_uris = vec!["http://learnos.example.com/cb".into()];
        assert!(validate_input(&i, true).is_err());
        let mut i = input();
        i.homepage_url = Some("javascript:alert(1)".into());
        assert!(validate_input(&i, false).is_err());
        let mut i = input();
        i.logo_url = Some("http://cdn.example.com/x.png".into());
        assert!(validate_input(&i, false).is_err());
        let mut i = input();
        i.scopes = vec!["profile".into()];
        assert!(validate_input(&i, false).is_err());
        let mut i = input();
        i.redirect_uris = (0..11).map(|n| format!("https://a.example/{n}")).collect();
        assert!(validate_input(&i, false).is_err());
    }

    #[test]
    fn auth_method_matrix() {
        use OidcTokenAuthMethod as M;
        assert_eq!(
            resolve_auth_method(&OidcClientType::Public, None).unwrap(),
            M::None
        );
        assert!(resolve_auth_method(&OidcClientType::Public, Some(M::ClientSecretBasic)).is_err());
        assert_eq!(
            resolve_auth_method(&OidcClientType::Confidential, None).unwrap(),
            M::ClientSecretBasic
        );
        assert_eq!(
            resolve_auth_method(&OidcClientType::Confidential, Some(M::ClientSecretPost)).unwrap(),
            M::ClientSecretPost
        );
        assert!(resolve_auth_method(&OidcClientType::Confidential, Some(M::None)).is_err());
    }

    #[test]
    fn status_machines() {
        use OidcClientStatus::*;
        assert_eq!(
            owner_transition(&Active, OwnerAction::Disable).unwrap(),
            Disabled
        );
        assert_eq!(
            owner_transition(&Disabled, OwnerAction::Enable).unwrap(),
            Active
        );
        // 所有者不能自行解除封禁、不能绕过审批
        assert!(owner_transition(&Suspended, OwnerAction::Enable).is_err());
        assert!(owner_transition(&Pending, OwnerAction::Enable).is_err());
        assert!(owner_transition(&Rejected, OwnerAction::Enable).is_err());

        assert_eq!(
            admin_transition(&Pending, AdminAction::Approve).unwrap(),
            Active
        );
        assert_eq!(
            admin_transition(&Pending, AdminAction::Reject).unwrap(),
            Rejected
        );
        assert_eq!(
            admin_transition(&Active, AdminAction::Suspend).unwrap(),
            Suspended
        );
        assert_eq!(
            admin_transition(&Suspended, AdminAction::Reinstate).unwrap(),
            Active
        );
        assert!(admin_transition(&Active, AdminAction::Approve).is_err());
        assert!(admin_transition(&Active, AdminAction::Reinstate).is_err());
    }

    fn confidential_model(secret_hash: Option<String>) -> oidc_clients::Model {
        let now = Utc::now().fixed_offset();
        oidc_clients::Model {
            id: Uuid::new_v4(),
            client_id: "fctf_x".into(),
            owner_user_id: Uuid::new_v4(),
            name: "x".into(),
            description: None,
            homepage_url: None,
            logo_url: None,
            client_type: OidcClientType::Confidential,
            token_endpoint_auth_method: OidcTokenAuthMethod::ClientSecretBasic,
            redirect_uris: serde_json::json!([]),
            allowed_scopes: serde_json::json!(["openid"]),
            require_pkce: true,
            secret_hash,
            previous_secret_hash: None,
            previous_secret_expires_at: None,
            secret_rotated_at: None,
            status: OidcClientStatus::Active,
            status_reason: None,
            reviewed_by: None,
            reviewed_at: None,
            created_at: now,
            updated_at: now,
            post_logout_redirect_uris: serde_json::json!([]),
            backchannel_logout_uri: None,
        }
    }

    #[test]
    fn secret_verification_with_rotation_window() {
        let old = secret::new_client_secret();
        let new = secret::new_client_secret();
        let mut client = confidential_model(Some(secret::hash_client_secret(&new).unwrap()));
        client.previous_secret_hash = Some(secret::hash_client_secret(&old).unwrap());
        client.previous_secret_expires_at =
            Some((Utc::now() + chrono::Duration::seconds(60)).fixed_offset());
        assert!(verify_secret(&client, &new));
        assert!(verify_secret(&client, &old));
        // 宽限期结束后旧 secret 失效
        client.previous_secret_expires_at =
            Some((Utc::now() - chrono::Duration::seconds(1)).fixed_offset());
        assert!(!verify_secret(&client, &old));
        assert!(verify_secret(&client, &new));
        assert!(!verify_secret(&client, "fcs_guess"));
        // 公开客户端永远不接受 secret
        let mut public = confidential_model(None);
        public.client_type = OidcClientType::Public;
        assert!(!verify_secret(&public, &new));
    }
}
