//! 平台系统对象固定主键（代码侧权威定义）。
//!
//! **权威在 Rust 常量**，不设数据库全局枚举/登记表。
//! 启动 seed、幂等 ensure 负责把对应行写入业务表；运行时也可按语义键
//! （`events.system_key`、`scheduled_tasks.task_key`）查询。
//!
//! 编号风格沿用调度种子任务：`Uuid::from_u128(n)` →
//! `00000000-0000-0000-0000-00000000000n`。
//!
//! 序号 **按表独立分配**（不同表可复用同一 `n`，无跨表唯一约束）。

use uuid::Uuid;

// ── scheduled_tasks：平台启动/维护任务 ──────────────────────────────────────

/// 任务键 `system.practice.check`：启动时 ensure 系统练习赛事 `practice:jeopardy`。
pub const SCHED_CHECK_PRACTICE_EVENT: Uuid = Uuid::from_u128(0);

/// 任务键 `system.practice.clean`：清理到期/残留的题目实例。
pub const SCHED_CLEAN_INSTANCES: Uuid = Uuid::from_u128(1);

/// 任务键 `platform.rustfs.clean`：回收对象存储中未引用文件。
pub const SCHED_CLEAN_RUSTFS: Uuid = Uuid::from_u128(2);

/// 任务键 `platform.training.sync`：同步训练站（floatctf-training）静态制品。
///
/// 触发类型是 `startup`（每次平台启动对一次远端提交，未变化则直接返回），
/// 管理端 `/admin/scheduled_tasks` 可随时手工「运行」。
pub const SCHED_TRAINING_SYNC: Uuid = Uuid::from_u128(3);

// ── events：系统托管赛事 ────────────────────────────────────────────────────

/// Jeopardy 系统练习赛事主键（`system_key = practice:jeopardy`）。
///
/// 与 [`SCHED_CLEAN_INSTANCES`] 同为 `from_u128(1)`，但分属 `events` /
/// `scheduled_tasks` 两表，互不冲突。业务代码优先按 `system_key` 解析；
/// 本常量用于 ensure、运维与文档中的稳定主键。
pub const EVENT_PRACTICE_JEOPARDY: Uuid = Uuid::from_u128(1);

/// [`EVENT_PRACTICE_JEOPARDY`] 的语义键（`events.system_key` 部分唯一）。
pub const EVENT_PRACTICE_JEOPARDY_SYSTEM_KEY: &str = "practice:jeopardy";

/// AWDP 练习系统虚拟赛事主键（`system_key = awdp-practice`，练习模块单挂载点）。
pub const EVENT_PRACTICE_AWDP: Uuid = Uuid::from_u128(2);

/// [`EVENT_PRACTICE_AWDP`] 的语义键（`events.system_key` 部分唯一）。
pub const EVENT_PRACTICE_AWDP_SYSTEM_KEY: &str = "awdp-practice";

/// 平台系统调度任务种子列表：`(主键, 显示名, task_key 字符串, 触发类型)`。
///
/// `task_key` 须与 [`crate::scheduler::task_key::TaskKey`] 的入库字符串一致。
pub fn startup_scheduled_task_seeds() -> &'static [(Uuid, &'static str, &'static str, &'static str)]
{
    &[
        (
            SCHED_CHECK_PRACTICE_EVENT,
            "检查练习event",
            "system.practice.check",
            "startup",
        ),
        (
            SCHED_CLEAN_INSTANCES,
            "实例清理",
            "system.practice.clean",
            "cron",
        ),
        (
            SCHED_CLEAN_RUSTFS,
            "RUSTFS文件清理",
            "platform.rustfs.clean",
            "cron",
        ),
        (
            SCHED_TRAINING_SYNC,
            "训练站制品同步",
            "platform.training.sync",
            "startup",
        ),
    ]
}

/// 平台种子 cron 的权威表达式。实例清理每 30 秒收敛到期/failed runtime；
/// RustFS 清理保持低频，每小时运行一次。
pub fn system_task_cron_expr(task_key: &str) -> Option<&'static str> {
    match task_key {
        "system.practice.clean" => Some("*/30 * * * * *"),
        "platform.rustfs.clean" => Some("0 0 * * * *"),
        _ => None,
    }
}

/// 平台系统任务的权威超时（秒）；`None` 表示沿用引擎兜底（60s）。
///
/// 训练站同步要下载几十 MB 的外部制品，60s 兜底在慢网络（例如国内访问 GitHub）下
/// 必然被掐断——而超时只表现为任务 failed + 重试，站点并不会坏，很容易被忽略。
pub fn system_task_timeout_secs(task_key: &str) -> Option<i32> {
    match task_key {
        "platform.training.sync" => Some(600),
        _ => None,
    }
}

/// 平台系统任务（SystemTask）固定主键集合：即 [`startup_scheduled_task_seeds`] 的 id。
///
/// 管理端 `/api/admin/scheduled_tasks` 的 `kind=system` 过滤 = `protected OR 属于本集合`；
/// 引擎重复任务（awd/awdp）以 `protected=true` 标记，无需在此枚举。
pub fn scheduled_task_system_ids() -> Vec<Uuid> {
    startup_scheduled_task_seeds()
        .iter()
        .map(|(id, _, _, _)| *id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn well_known_ids_match_nil_style_strings() {
        assert_eq!(
            SCHED_CHECK_PRACTICE_EVENT.to_string(),
            "00000000-0000-0000-0000-000000000000"
        );
        assert_eq!(
            SCHED_CLEAN_INSTANCES.to_string(),
            "00000000-0000-0000-0000-000000000001"
        );
        assert_eq!(
            SCHED_CLEAN_RUSTFS.to_string(),
            "00000000-0000-0000-0000-000000000002"
        );
        assert_eq!(
            SCHED_TRAINING_SYNC.to_string(),
            "00000000-0000-0000-0000-000000000003"
        );
        assert_eq!(
            EVENT_PRACTICE_JEOPARDY.to_string(),
            "00000000-0000-0000-0000-000000000001"
        );
        // 不同表复用同一 u128 序号是有意设计
        assert_eq!(EVENT_PRACTICE_JEOPARDY, SCHED_CLEAN_INSTANCES);
    }

    #[test]
    fn startup_seed_list_covers_four_platform_tasks() {
        let seeds = startup_scheduled_task_seeds();
        assert_eq!(seeds.len(), 4);
        assert_eq!(seeds[0].0, SCHED_CHECK_PRACTICE_EVENT);
        assert_eq!(seeds[1].0, SCHED_CLEAN_INSTANCES);
        assert_eq!(seeds[2].0, SCHED_CLEAN_RUSTFS);
        assert_eq!(seeds[3].0, SCHED_TRAINING_SYNC);
        // task_key 必须能被解析回枚举，否则引擎校验会在启动时失败
        for (_, _, task_key, trigger_type) in seeds {
            assert!(
                task_key.parse::<crate::scheduler::TaskKey>().is_ok(),
                "{task_key} 不是已注册的任务键"
            );
            assert!(
                matches!(*trigger_type, "startup" | "cron" | "once"),
                "{task_key} 的触发类型非法：{trigger_type}"
            );
        }
    }

    #[test]
    fn scheduled_task_system_ids_matches_seed_list() {
        let ids = scheduled_task_system_ids();
        let expected: Vec<Uuid> = startup_scheduled_task_seeds()
            .iter()
            .map(|(id, _, _, _)| *id)
            .collect();
        assert_eq!(ids, expected);
        assert_eq!(ids.len(), 4);
        assert!(ids.contains(&SCHED_CHECK_PRACTICE_EVENT));
        assert!(ids.contains(&SCHED_CLEAN_INSTANCES));
        assert!(ids.contains(&SCHED_CLEAN_RUSTFS));
        assert!(ids.contains(&SCHED_TRAINING_SYNC));
    }
}
