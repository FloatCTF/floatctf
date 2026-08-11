//! 执行上下文（Phase 2 P2-1 / 计划 §5.1）。
//!
//! Precheck 与正式比赛共用底层运行时，但**隔离**约束不同：
//! - `Competition`：正式比赛路径 —— 允许写正式 round / score / awd_flag_issues 等表；
//! - `Precheck`：预检路径 —— 只读 / 隔离，不得污染正式比赛状态。
//!
//! 本 Phase（P2-1..P2-8）先落地类型与参数接线；正式 issue / judge 调用链在 Phase 3 接入。

use uuid::Uuid;

/// 执行上下文：区分预检（隔离）与正式比赛（可写）路径。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionContext {
    /// 正式比赛：写正式状态。
    Competition,
    /// 预检：只读/隔离路径，带 run_id 标记，不写正式 awd_flag_issues / score 表。
    Precheck { run_id: Uuid },
}
