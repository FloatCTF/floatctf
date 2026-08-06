//! Weapon catalog module — player listing and admin CRUD/upload.

mod api;
mod application;
pub mod dto;
pub use dto::{WeaponsDto, CreateWeaponRequest, PatchWeaponRequest, WeaponForm};

pub use api::{configure_admin_routes, configure_player_routes};
