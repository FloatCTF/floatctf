//! Request-scoped application context extractor (`ReqCtx`).

use actix_web::FromRequest;
use actix_web::HttpRequest;

use crate::infrastructure::{WebDb, WebDocker, WebLog, WebRustfs};

/// Bundled request dependencies extracted from Actix app data.
pub struct ReqCtx {
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
        let db = req.app_data::<WebDb>().expect("WebDb not found").clone();
        let docker = req
            .app_data::<WebDocker>()
            .expect("WebDocker not found")
            .clone();
        let rustfs = req
            .app_data::<WebRustfs>()
            .expect("WebRustfs not found")
            .clone();
        let log = req.app_data::<WebLog>().expect("WebLog not found").clone();
        std::future::ready(Ok(Self {
            db,
            docker,
            rustfs,
            log,
            req: req.clone(),
        }))
    }
}
