//! Request/audit logging service (`LogService` / `WebLog`).

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
    pub async fn add_log(
        &self,
        level: &str,
        category: &str,
        action: &str,
        message: &str,
        details: Value,
        user_id: Option<Uuid>,
        superadmin_id: Option<Uuid>,
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
            user_id: Set(user_id),
            superadmin_id: Set(superadmin_id),
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
            r#type: Set(event.r#type.clone()),
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
