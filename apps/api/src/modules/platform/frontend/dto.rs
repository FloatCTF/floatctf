use serde::Serialize;

/// `GET /api/frontend` 响应体 —— **只**包含安全、稳定、可公开的引导元数据。
///
/// 刻意不包含：任意设置项、密钥、文件系统路径、注册表内容、前端资产路径、
/// 内部版本号之外的任何内部信息。该端点**必须**在登录前可用，因为登录 UX 本身
/// 属于所选前端。
#[derive(Debug, Serialize)]
pub struct FrontendBootstrapDto {
    /// 平台设置 `FRONTEND_ACTIVE`（非法值已回落 `default`）。
    pub active_frontend: String,
    /// 平台版本（`CARGO_PKG_VERSION`）。
    pub platform_version: String,
    /// 前端可见的 HTTP API 契约 major。
    pub api_contract_version: String,
    /// 前端运行时（制品 / `mount`）契约 major。
    pub frontend_runtime_version: String,
    /// 真实且稳定的平台能力标记。
    pub capabilities: Vec<String>,
}
