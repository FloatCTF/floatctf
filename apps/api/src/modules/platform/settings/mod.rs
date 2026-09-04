//! 平台键值设置管理 API。

mod api;
mod dto;

pub use api::{create_setting, delete_setting, get_settings, patch_setting};
pub use dto::SettingsDto;
