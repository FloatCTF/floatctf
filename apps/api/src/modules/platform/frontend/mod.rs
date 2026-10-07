//! 可插拔前端的公开引导端点（`GET /api/frontend`）。
//!
//! 后端在这里**只**知道两件事（见 docs/frontend/ARCHITECTURE.md）：
//! 1. 当前生效的前端标识（动态设置 `FRONTEND_ACTIVE`）
//! 2. 平台版本、API/运行时契约版本与真实能力标记
//!
//! 它**不知道**路由、页面、布局、登录页、侧栏——UI 结构完全由前端自己拥有。
//! 前端安装/版本解析由文件系统注册表 + `scripts/frontend.sh` 负责，与本模块无关。

pub mod api;
pub mod domain;
pub mod dto;

pub use dto::FrontendBootstrapDto;
