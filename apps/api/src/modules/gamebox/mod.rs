//! 共享 GameBox 库（与赛制无关）。
//!
//! 从 `modules/event/awd/service/*` 迁出的真正共享 capability：
//! 包解析、import 管线、库 repo、镜像钉扎与健康检查探针。
//!
//! 依赖方向：`AWD` / `AWDP` 都依赖本模块；本模块**不**依赖任何 family。
//! 禁止在本模块引入赛事语义（round / phase / score / patch）。

pub mod error;
pub mod healthcheck;
pub mod identity;
pub mod import;
pub mod library;
pub mod package;

pub use error::{GameboxError, GameboxResult};
pub use import::{
    BUILD_STATUS_BUILDING, BUILD_STATUS_FAILED, BUILD_STATUS_READY, GameBoxScanItem,
    ImportGameBoxResult,
};
pub use library::{
    GameBoxIdentityPatch, effective_image_ref_from_gamebox, unique_safe_name,
    update_gamebox_identity_checked, validate_identity_safe_name,
};
