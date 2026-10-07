//! 对外契约版本（前端平台）。
//!
//! 三个版本是**互相独立**的概念，禁止合并成一个变量（详见
//! `docs/frontend/ARCHITECTURE.md`「版本策略」）：
//!
//! | 常量 | 含义 | 何时 +1 |
//! |---|---|---|
//! | [`API_CONTRACT_VERSION`] | 前端可见的 HTTP API 契约 major | 破坏性 API 变更（删字段 / 改语义 / 改必填） |
//! | [`FRONTEND_RUNTIME_CONTRACT_VERSION`] | 前端制品与 `mount(context)` 契约 major | 破坏 `frontend.json` 或 runtime 接口 |
//! | 平台版本（`CARGO_PKG_VERSION`） | FloatCTF 平台自身版本 | 按仓库既有发版策略 |
//!
//! **加法式变更不升 major**：新增可选字段、新增端点、新增响应字段都留在当前
//! contract major 内。只有"旧前端会因此坏掉"的变更才升 major，并且必须同步
//! `packages/frontend-runtime/src/version.ts`（`scripts/check-architecture.sh` 会
//! 断言两边一致，防止漂移）。

/// 前端可见的 HTTP API 契约 major（`GET /api/frontend` 的 `api_contract_version`）。
pub const API_CONTRACT_VERSION: &str = "1";

/// 前端运行时契约 major（`GET /api/frontend` 的 `frontend_runtime_version`）。
pub const FRONTEND_RUNTIME_CONTRACT_VERSION: &str = "1";

/// 平台真实且稳定的能力标记白名单。
///
/// `GET /api/frontend` 只允许返回这里的元素：能力列表是**公开**契约，
/// 不能随手把内部特性写进去（既有泄露风险，也会让前端探测到不存在的功能）。
pub const PLATFORM_CAPABILITIES: &[&str] = &[
    "jeopardy",
    "awd",
    "awdp",
    "discussions",
    "writeups",
    "web_terminal",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_majors_are_positive_integers() {
        for value in [API_CONTRACT_VERSION, FRONTEND_RUNTIME_CONTRACT_VERSION] {
            assert!(
                value.parse::<u32>().is_ok(),
                "contract major must be an integer string, got {value:?}"
            );
        }
    }

    #[test]
    fn capabilities_are_lowercase_identifiers() {
        for capability in PLATFORM_CAPABILITIES {
            assert!(!capability.is_empty());
            assert!(
                capability
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
                "capability must be a lowercase identifier: {capability:?}"
            );
        }
    }
}
