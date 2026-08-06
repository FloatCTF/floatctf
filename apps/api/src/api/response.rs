use actix_web::{HttpRequest, HttpResponse, Responder, body::BoxBody};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct QueryParams {
    pub offset: Option<u64>,
    pub limit: Option<u64>,
    pub page: Option<u64>,
    pub filter: Option<String>,

    // for response
    pub total: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct UniResponse<T> {
    pub code: i32,
    pub message: String,
    pub data: Option<T>,
    pub meta: Option<QueryParams>,
}

impl<T> UniResponse<T> {
    pub fn ok(data: Option<T>) -> Self {
        Self {
            code: 0,
            message: "OK".to_string(),
            data,
            meta: None,
        }
    }

    pub fn ok_meta(data: Option<T>, meta: Option<QueryParams>) -> Self {
        Self {
            code: 0,
            message: "OK".to_string(),
            data,
            meta: meta,
        }
    }

    pub fn ok_none() -> Self {
        Self {
            code: 0,
            message: "OK".to_string(),
            data: None,
            meta: None,
        }
    }

    pub fn err(code: i32, message: String) -> Self {
        Self {
            code,
            message,
            data: None,
            meta: None,
        }
    }
}

impl<T> Responder for UniResponse<T>
where
    T: Serialize,
{
    type Body = BoxBody;
    fn respond_to(self, _req: &HttpRequest) -> HttpResponse<Self::Body> {
        HttpResponse::Ok().json(self)
    }
}
