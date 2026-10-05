//! 请求级应用上下文提取器（`ReqCtx`）。

use actix_web::FromRequest;
use actix_web::HttpRequest;
use std::sync::Arc;

use crate::core::AppConfig;
use crate::infrastructure::{WebDb, WebDocker, WebLog, WebRustfs};

/// 从 Actix app data 提取的捆绑请求依赖。
pub struct ReqCtx {
    pub config: Arc<AppConfig>,
    pub db: WebDb,
    pub docker: WebDocker,
    pub rustfs: WebRustfs,
    pub log: WebLog,
    pub req: HttpRequest,
}

impl FromRequest for ReqCtx {
    type Error = actix_web::Error;
    type Future = std::future::Ready<Result<Self, Self::Error>>;
    fn from_request(req: &HttpRequest, _: &mut actix_web::dev::Payload) -> Self::Future {
        let config = req
            .app_data::<actix_web::web::Data<crate::bootstrap::AppState>>()
            .expect("AppState 不存在")
            .config
            .clone();
        let db = req.app_data::<WebDb>().expect("WebDb 不存在").clone();
        let docker = req
            .app_data::<WebDocker>()
            .expect("WebDocker 不存在")
            .clone();
        let rustfs = req
            .app_data::<WebRustfs>()
            .expect("WebRustfs 不存在")
            .clone();
        let log = req.app_data::<WebLog>().expect("WebLog 不存在").clone();
        std::future::ready(Ok(Self {
            config,
            db,
            docker,
            rustfs,
            log,
            req: req.clone(),
        }))
    }
}
