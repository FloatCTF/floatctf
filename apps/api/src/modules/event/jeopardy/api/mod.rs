//! Jeopardy HTTP 处理器（实例与 Flag 提交）。

pub mod dto;
pub use dto::InstancesDto;

pub mod instances;
pub mod submit;

use actix_web::web::{self, ServiceConfig};

/// 在 `/instances` 作用域注册实例路由。
pub fn configure_instance_routes(cfg: &mut ServiceConfig) {
    cfg.service(instances::get_instances)
        .service(instances::get_instance)
        .service(instances::launch_instance)
        .service(instances::bulk_destroy_instances)
        .service(instances::destroy_instance);
}

/// 在 `/submit` 作用域注册提交路由。
pub fn configure_submit_routes(cfg: &mut ServiceConfig) {
    cfg.service(submit::submit_flag)
        .service(submit::submit_writeup);
}
