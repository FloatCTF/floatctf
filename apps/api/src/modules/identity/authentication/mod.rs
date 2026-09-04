//! 登录、注册、会话与口令重置。

use crate::api::prelude::*;
use crate::core::security::jwt::{Role, gen_jwt_token, validate_jwt};
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

/// POST /api/users/session
#[post("/session")]
pub async fn user_login(ctx: ReqCtx, ulr: Json<UserLoginRequest>) -> UniResult<String> {
    let ulr = ulr.into_inner();

    match Users::find()
        .filter(users::Column::Username.eq(ulr.username))
        .one(ctx.db.get_ref())
        .await?
    {
        Some(user) => {
            let verified = {
                let parsed_hash = PasswordHash::new(&user.password).map_err(|e| {
                    AppError::Internal(format!("Failed to new the PasswordHash: {e}"))
                })?;
                Argon2::default()
                    .verify_password(ulr.password.as_bytes(), &parsed_hash)
                    .is_ok()
            };

            if verified {
                ctx.log
                    .add_log(
                        "INFO",
                        "AUTH",
                        "LOGIN",
                        format!("{} 登陆成功", user.username).as_str(),
                        json!([]),
                        user.id.into(),
                        None,
                        Some(&ctx.req),
                    )
                    .await;
                let jwt = gen_jwt_token(user.id, Role::User, None)
                    .map_err(|e| AppError::BadRequest(e.to_string()))?;

                UniResponse::ok(jwt.into()).into()
            } else {
                ctx.log
                    .add_log(
                        "ERROR",
                        "AUTH",
                        "LOGIN",
                        format!("{} 登陆失败", user.username).as_str(),
                        json!([]),
                        user.id.into(),
                        None,
                        Some(&ctx.req),
                    )
                    .await;
                AppError::Unauthorized.into()
            }
        }
        None => AppError::Unauthorized.into(),
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
        username: Set(cur.username),
        password: Set(hashed_password),
        email: Set(cur.email),
        nickname: Set(cur.nickname),
        ..Default::default()
    };

    let user = new_user.insert(ctx.db.get_ref()).await?;
    ctx.log
        .add_log(
            "INFO",
            "AUTH",
            "REGISTER",
            format!("{} 注册成功", user.username).as_str(),
            json!({}),
            user.id.into(),
            None,
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

    let user = user.ok_or_else(|| AppError::BadRequest("User not found".to_string()))?;

    let main_url = get_setting(ctx.db.get_ref(), "MAIN_URL")
        .await
        .map_err(|e| AppError::BadRequest(format!("Failed to get MAIN_URL: {}", e)))?;

    let token = gen_jwt_token(user.id, Role::ResetAccount, Some(10))
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
            None,
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
    if claim.role != Role::ResetAccount {
        return AppError::BadRequest("Invalid token".to_string()).into();
    }

    let user_id = claim.sub;
    let user = Users::find_by_id(user_id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or_else(|| AppError::BadRequest("User not found".to_string()))?;

    let user_id = user.id;
    let username = user.username.clone();
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

    m_user.update(ctx.db.get_ref()).await?;

    ctx.log
        .add_log(
            "INFO",
            "AUTH",
            "RESET_PASSWORD",
            format!("{} 重置密码成功", username).as_str(),
            json!({}),
            user_id.into(),
            None,
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok_none().into()
}
