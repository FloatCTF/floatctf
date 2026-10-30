//! 登录、注册、会话与口令重置。

use crate::api::prelude::*;
use crate::core::security::jwt::{
    TokenPurpose, gen_password_reset_token, gen_session_token, validate_jwt,
};
use crate::entity::sea_orm_active_enums::AccountStatus;
use crate::entity::{prelude::Users, users};

use argon2::{
    Argon2, PasswordHash, PasswordVerifier,
    password_hash::{PasswordHasher, SaltString, rand_core::OsRng},
};

#[derive(Debug, Deserialize, Serialize)]
pub struct UserLoginRequest {
    username: String,
    password: String,
}

/// 校验玩家用户名 + 口令（Argon2）。
///
/// 返回 `(Some(user), true)` 表示通过；`(Some(user), false)` 表示用户存在但口令错误；
/// `(None, false)` 表示用户不存在。平台登录接口与 OIDC 第一方登录页共用这一实现。
pub(crate) async fn verify_user_credentials(
    db: &sea_orm::DatabaseConnection,
    username: &str,
    password: &str,
) -> Result<(Option<users::Model>, bool), AppError> {
    let Some(user) = Users::find()
        .filter(users::Column::Username.eq(username))
        .one(db)
        .await?
    else {
        return Ok((None, false));
    };
    let parsed_hash = PasswordHash::new(&user.password)
        .map_err(|e| AppError::Internal(format!("Failed to new the PasswordHash: {e}")))?;
    let verified = Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok();
    Ok((Some(user), verified))
}

/// POST `/api/session` **与** `POST /api/users/session`（同一个 handler）。
///
/// v2 统一入口是 `/api/session`；`/api/users/session` 作为别名保留到 Wave 10。
/// 两条路径**复用同一个 handler**，因此不可能出现「某一条登录路径行为不同」的分叉缺陷
/// （这正是统一账户要消灭的那类问题）。
///
/// 响应体与引入 OIDC 前逐字一致（data 为平台 JWT）。OIDC Provider 启用时额外下发
/// 第一方 SSO Cookie，使已登录 FloatCTF 的浏览器授权第三方应用时无需再次输入密码；
/// Cookie 建立失败不影响平台登录本身。
#[post("/session")]
pub async fn user_login(
    ctx: ReqCtx,
    oidc: Option<crate::modules::identity::oidc::WebOidc>,
    ulr: Json<UserLoginRequest>,
) -> Result<actix_web::HttpResponse, AppError> {
    let ulr = ulr.into_inner();

    match verify_user_credentials(ctx.db.get_ref(), &ulr.username, &ulr.password).await? {
        (Some(user), true) => {
            // 凭据正确但账号被禁用 → **403**（而不是 401）：
            // 用户需要知道「账号存在但被停用」，重新登录不会解决问题。
            if user.status != AccountStatus::Active {
                ctx.log
                    .add_log(
                        "ERROR",
                        "AUTH",
                        "LOGIN_DISABLED",
                        format!("{} 账号已禁用，拒绝登录", user.username).as_str(),
                        json!([]),
                        user.id.into(),
                        Some(&ctx.req),
                    )
                    .await;
                return Err(AppError::Forbidden("账号已被禁用".to_string()));
            }

            ctx.log
                .add_log(
                    "INFO",
                    "AUTH",
                    "LOGIN",
                    format!("{} 登陆成功", user.username).as_str(),
                    json!([]),
                    user.id.into(),
                    Some(&ctx.req),
                )
                .await;

            // 令牌只带身份与凭据版本；授权信息由每个请求从 DB 解析。
            let jwt = gen_session_token(user.id, user.token_version)
                .map_err(|e| AppError::BadRequest(e.to_string()))?;

            let mut response = actix_web::HttpResponse::Ok();
            if let Some(oidc) = oidc {
                match crate::modules::identity::oidc::api::session::establish_after_password_login(
                    ctx.db.get_ref(),
                    &oidc,
                    &ctx.req,
                    &user,
                )
                .await
                {
                    Ok(cookies) => {
                        for cookie in cookies {
                            response.cookie(cookie);
                        }
                    }
                    Err(e) => warn!(error = %e, "SSO session not established after login"),
                }
            }

            Ok(response.json(UniResponse::ok(Some(jwt))))
        }
        (Some(user), false) => {
            ctx.log
                .add_log(
                    "ERROR",
                    "AUTH",
                    "LOGIN",
                    format!("{} 登陆失败", user.username).as_str(),
                    json!([]),
                    user.id.into(),
                    Some(&ctx.req),
                )
                .await;
            Err(AppError::Unauthorized)
        }
        (None, _) => Err(AppError::Unauthorized),
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateUserRequest {
    username: String,
    nickname: String,
    password: String,
    email: String,
}

/// POST /api/users
#[post("")]
pub async fn create_user(ctx: ReqCtx, cur: Json<CreateUserRequest>) -> UniResult<String> {
    let cur = cur.into_inner();

    super::validate_account_fields(&cur.username, &cur.password, &cur.email)?;

    let hashed_password = {
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();

        let password_hash = argon2
            .hash_password(cur.password.as_bytes(), &salt)
            .map_err(|e| AppError::BadRequest(format!("{}", e.to_string())))?
            .to_string();

        password_hash
    };

    let new_user = users::ActiveModel {
        username: Set(cur.username.trim().to_string()),
        password: Set(hashed_password),
        email: Set(cur.email.trim().to_string()),
        nickname: Set(cur.nickname),
        ..Default::default()
    };

    let user = match new_user.insert(ctx.db.get_ref()).await {
        Ok(user) => user,
        Err(error) if crate::api::is_unique_violation(&error.to_string()) => {
            return Err(crate::api::unique_violation_message(&error.to_string()));
        }
        Err(error) => return Err(error.into()),
    };
    ctx.log
        .add_log(
            "INFO",
            "AUTH",
            "REGISTER",
            format!("{} 注册成功", user.username).as_str(),
            json!({}),
            user.id.into(),
            Some(&ctx.req),
        )
        .await;
    UniResponse::ok(
        "User created successfully, please login "
            .to_string()
            .into(),
    )
    .into()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ResetPasswordRequest {
    pub email: Option<String>,
    pub username: Option<String>,
}

/// POST /api/users/reset_password
#[post("/reset_password")]
pub async fn send_reset_email(ctx: ReqCtx, rpr: Json<ResetPasswordRequest>) -> UniResult<()> {
    let rpr = rpr.into_inner();

    if rpr.email.is_none() && rpr.username.is_none() {
        return AppError::BadRequest("Email or username is required".to_string()).into();
    }
    let email = none_if_empty(rpr.email);
    let username = none_if_empty(rpr.username);

    let user = match (email, username) {
        (Some(email), _) => {
            Users::find()
                .filter(users::Column::Email.eq(email))
                .one(ctx.db.get_ref())
                .await?
        }
        (_, Some(username)) => {
            Users::find()
                .filter(users::Column::Username.eq(username))
                .one(ctx.db.get_ref())
                .await?
        }
        _ => return AppError::BadRequest("Email or username required".into()).into(),
    };

    let user = user.ok_or_else(|| AppError::BadRequest("User 不存在".to_string()))?;

    let main_url = get_setting(ctx.db.get_ref(), "MAIN_URL")
        .await
        .map_err(|e| AppError::BadRequest(format!("Failed to get MAIN_URL: {}", e)))?;

    let token = gen_password_reset_token(user.id, user.token_version)
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    let reset_link = format!("{}/reset_password?token={}", main_url, token);
    let to = user.email;

    let html_body = format!(
        r#"
        <p>您好，</p>
        <p>请点击下方按钮重置密码（10 分钟内有效）</p>
        <p><a href="{0}" style="color:#4a90e2;font-weight:bold;">点击这里重置密码</a></p>
        <p>如果不是您发起的重置请求，请忽略此邮件。</p>
        "#,
        reset_link
    );
    send_email(&ctx.db, &[&to], None, "重置密码", &html_body)
        .await
        .map_err(|e| AppError::BadRequest(format!("Failed to send email: {}", e)))?;

    ctx.log
        .add_log(
            "INFO",
            "AUTH",
            "RESET_PASSWORD_REQUEST",
            format!("{} 请求重置密码", user.username).as_str(),
            json!({}),
            user.id.into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok_none().into()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ResetPasswordOption {
    pub password: String,
    pub confirmed_password: String,
}

#[derive(Debug, Deserialize)]
pub struct TokenQuery {
    pub token: String,
}

/// POST /api/users/reset?token=...
#[post("/reset")]
pub async fn reset_password(
    ctx: ReqCtx,
    token: Query<TokenQuery>,
    rpo: Json<ResetPasswordOption>,
) -> UniResult<()> {
    let rpo = rpo.into_inner();
    if rpo.password != rpo.confirmed_password {
        return AppError::BadRequest("Passwords do not match".to_string()).into();
    }

    let token = token.token.clone();
    let claim = validate_jwt(token).map_err(|e| AppError::BadRequest(e.to_string()))?;
    // 用途必须**恰好**是口令重置：会话令牌不得用来重置口令
    // （否则一个被盗的会话令牌就能永久接管账号）。
    if claim.purpose != TokenPurpose::PasswordReset {
        return AppError::BadRequest("Invalid token".to_string()).into();
    }

    let user_id = claim.sub;
    let user = Users::find_by_id(user_id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or_else(|| AppError::BadRequest("User 不存在".to_string()))?;

    // 版本必须匹配：改密后旧的重置令牌立即失效（也避免同一封邮件被重复使用）
    if user.token_version != claim.token_version {
        return AppError::BadRequest("Invalid token".to_string()).into();
    }

    let user_id = user.id;
    let username = user.username.clone();
    let next_token_version = user.token_version.saturating_add(1);
    let mut m_user = user.into_active_model();
    let hashed_password = {
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();

        let password_hash = argon2
            .hash_password(rpo.password.as_bytes(), &salt)
            .map_err(|e| AppError::BadRequest(format!("{}", e.to_string())))?
            .to_string();

        password_hash
    };

    m_user.password = Set(hashed_password);
    // 改密即吊销：自增 token_version 使**所有**已签发的会话令牌与重置令牌立即失效。
    // 这是「改密后其他设备上的登录必须掉线」的实现（v1 完全没有这个能力）。
    m_user.token_version = Set(next_token_version);

    m_user.update(ctx.db.get_ref()).await?;

    ctx.log
        .add_log(
            "INFO",
            "AUTH",
            "RESET_PASSWORD",
            format!("{} 重置密码成功", username).as_str(),
            json!({}),
            user_id.into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok_none().into()
}
