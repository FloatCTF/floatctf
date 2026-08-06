//! Jeopardy HTTP handlers (instances + flag submission).

pub mod dto;
pub use dto::InstancesDto;


pub mod instances;
pub mod submit;

use actix_web::web::{self, ServiceConfig};

/// Register instance routes under `/instances` scope.
pub fn configure_instance_routes(cfg: &mut ServiceConfig) {
    cfg.service(instances::get_instances)
        .service(instances::get_instance)
        .service(instances::launch_instance)
        .service(instances::destroy_instance);
}

/// Register submit routes under `/submit` scope.
pub fn configure_submit_routes(cfg: &mut ServiceConfig) {
    cfg.service(submit::submit_flag)
        .service(submit::submit_writeup);
}
