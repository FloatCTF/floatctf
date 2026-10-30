//! 请求/审计日志服务（`LogService` / `WebLog`）。

use sea_orm::{ActiveModelTrait, ActiveValue::Set};
use serde_json::Value;
use tracing::error;
use uuid::Uuid;

use crate::entity::{event_logs, events, logs};
use crate::infrastructure::WebDb;

#[derive(Clone, Debug)]
pub struct LogService {
    pub db: WebDb,
}

impl LogService {
    pub fn new(db: WebDb) -> Self {
        Self { db }
    }
    pub fn get_client_ip(req: &actix_web::HttpRequest) -> Option<String> {
        // 1. 优先取 X-Forwarded-For 的第一个（最真实）
        if let Some(hdr) = req.headers().get("X-Forwarded-For") {
            if let Ok(s) = hdr.to_str() {
                return s.split(',').next().map(|ip| ip.trim().to_string());
            }
        }

        // 2. 其次取 X-Real-IP
        if let Some(hdr) = req.headers().get("X-Real-IP") {
            if let Ok(s) = hdr.to_str() {
                return Some(s.to_string());
            }
        }

        // 3. 最后兜底
        req.connection_info()
            .realip_remote_addr()
            .map(|s| s.to_string())
    }
    /// 写一条平台审计日志。
    ///
    /// # 为什么只有一个 `actor_user_id`
    ///
    /// v1 的签名是 `(user_id, superadmin_id)` 双列 —— 它假定「选手主体」与
    /// 「管理员主体」是两个不同的身份空间。统一账户后它们是同一个空间，
    /// 双列会让每个读取方各自决定怎么合并，而写入方可以只填一个，
    /// 于是审计查询很容易**静默漏掉**管理动作（审计漏记本身就是安全问题）。
    ///
    /// 因此 Wave 6 收敛为单一 `actor_user_id`：
    /// - `Some(id)` = 某个账号发起的动作（无论该账号是选手还是管理员）；
    /// - `None`     = 系统动作（定时任务、内部回调）或主体账号已注销。
    ///
    /// 「这是选手行为还是平台管理行为」的信息由 `category` + `action` 表达，
    /// 不再由主体列承担 —— 主体列只回答「谁」。
    pub async fn add_log(
        &self,
        level: &str,
        category: &str,
        action: &str,
        message: &str,
        details: Value,
        actor_user_id: Option<Uuid>,
        request: Option<&actix_web::HttpRequest>,
    ) {
        let ip_address = request.map(|req| Self::get_client_ip(req)).flatten();

        let log = logs::ActiveModel {
            level: Set(level.to_string()),
            category: Set(category.to_string()),
            action: Set(action.to_string()),
            message: Set(message.to_string()),
            details: Set(details),
            ip_address: Set(ip_address),
            actor_user_id: Set(actor_user_id),
            ..Default::default()
        };
        let db = self.db.get_ref().clone();
        actix_web::rt::spawn(async move {
            if let Err(e) = log.insert(&db).await {
                error!("Failed to insert log: {}", e);
            }
        });
    }
    pub async fn add_event_log(
        &self,
        event: &events::Model,
        level: &str,
        action: &str,
        details: Value,
        user_id: Option<Uuid>,
        team_id: Option<Uuid>,
        request: Option<&actix_web::HttpRequest>,
    ) {
        let ip_address = request.map(|req| Self::get_client_ip(req)).flatten();

        let log = event_logs::ActiveModel {
            event_id: Set(event.id),
            user_id: Set(user_id),
            team_id: Set(team_id),
            family: Set(event.family.clone()),
            purpose: Set(event.purpose.clone()),
            participant_mode: Set(event.participant_mode.clone()),
            level: Set(level.to_string()),
            action: Set(action.to_string()),
            details: Set(details),
            ip_address: Set(ip_address),
            ..Default::default()
        };

        let db = self.db.get_ref().clone();
        actix_web::rt::spawn(async move {
            if let Err(e) = log.insert(&db).await {
                error!("Failed to insert log: {}", e);
            }
        });
    }
}

pub type WebLog = actix_web::web::Data<LogService>;
