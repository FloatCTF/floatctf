//! AWD SSE 流构建器（选手端与管理端共享）。
//!
//! 提取广播订阅、SSE 帧格式化、keepalive、响应头等公共实现，
//! 避免选手路由与管理路由重复传输逻辑。

use actix_web::{HttpResponse, web};
use uuid::Uuid;

/// 构建 AWD Event SSE 流响应。
///
/// 订阅 `BroadcastEventPublisher`，按 `event_id` 过滤事件，
/// 以 `data: {json}\n\n` 帧格式输出，附带 25s keepalive 注释。
///
/// 调用方负责认证与授权；本函数仅构建流。
pub fn build_awd_event_stream(
    hub: &crate::infrastructure::realtime::BroadcastEventPublisher,
    event_id: Uuid,
) -> HttpResponse {
    let rx = hub.subscribe();
    let keepalive_interval = std::time::Duration::from_secs(25);

    let body = futures_util::stream::unfold(
        (rx, false, event_id, keepalive_interval),
        |(mut rx, primed, event_id, keepalive_interval)| async move {
            if !primed {
                return Some((
                    Ok::<_, actix_web::Error>(web::Bytes::from(": connected\n\n")),
                    (rx, true, event_id, keepalive_interval),
                ));
            }
            loop {
                tokio::select! {
                    recv = rx.recv() => {
                        match recv {
                            Ok(ev) => {
                                if ev.event_id != event_id {
                                    continue;
                                }
                                match serde_json::to_string(&ev) {
                                    Ok(json) => {
                                        return Some((
                                            Ok(web::Bytes::from(format!("data: {json}\n\n"))),
                                            (rx, true, event_id, keepalive_interval),
                                        ));
                                    }
                                    Err(_) => continue,
                                }
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                                let payload = serde_json::json!({
                                    "type": "stream.lagged",
                                    "event_id": event_id,
                                });
                                return Some((
                                    Ok(web::Bytes::from(format!("data: {payload}\n\n"))),
                                    (rx, true, event_id, keepalive_interval),
                                ));
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
                        }
                    }
                    _ = tokio::time::sleep(keepalive_interval) => {
                        return Some((
                            Ok(web::Bytes::from(": keepalive\n\n")),
                            (rx, true, event_id, keepalive_interval),
                        ));
                    }
                }
            }
        },
    );

    HttpResponse::Ok()
        .insert_header((actix_web::http::header::CONTENT_TYPE, "text/event-stream"))
        .insert_header((actix_web::http::header::CACHE_CONTROL, "no-cache"))
        .insert_header((actix_web::http::header::CONNECTION, "keep-alive"))
        .insert_header((
            actix_web::http::header::HeaderName::from_static("x-accel-buffering"),
            actix_web::http::header::HeaderValue::from_static("no"),
        ))
        .streaming(body)
}