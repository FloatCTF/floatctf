//! 武器库模块——选手列表与管理端 CRUD/上传。

mod api;
mod application;
pub mod dto;
pub use dto::{CreateWeaponRequest, PatchWeaponRequest, WeaponForm, WeaponsDto};

pub use api::{configure_admin_routes, configure_player_routes};
