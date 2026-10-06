//! 通用工具类调度处理器（如清理未使用对象存储文件）。

use async_trait::async_trait;
use tracing::warn;

use crate::{
    entity::scheduled_tasks,
    infrastructure::{WebDb, WebRustfs},
    scheduler::{TaskHandler, TaskKey},
};

/// `platform.rustfs.clean`（小时级、protected）。
///
/// ⚠️ **当前是未实现的空操作**：它不列举、不删除任何 RustFS 对象。
///
/// 修复前的两处误导（都已改掉）：
///   1. 日志把处理器名硬编码成 `CleanRunningInstancesHandler` —— 排查时会把你带到一个
///      完全无关的实现；
///   2. 第二行日志写着「scheduler task is running」，配合 `scheduled_tasks` 里
///      status=success 的记录，会让人以为「对象存储每小时都在清理」。
///
/// 要真正实现清理，必须先建立「对象 key ↔ DB 引用」的完整映射（头像/题目附件/RustFS
/// 私有对象/GameBox source artifact 等），再配合年龄下限删除；漏掉任何一类引用都会
/// 误删线上数据。在此之前保持 no-op 并如实告警。
pub struct CleanUnusedRustFSFilesHandler {
    pub db: WebDb,
    pub rustfs: WebRustfs,
}

#[async_trait]
impl TaskHandler for CleanUnusedRustFSFilesHandler {
    fn trigger_type(&self) -> &'static str {
        "cron"
    }
    fn task_key(&self) -> TaskKey {
        TaskKey::CleanRustfs
    }
    async fn run(&self, _task: scheduled_tasks::Model) -> anyhow::Result<()> {
        warn!(
            task_key = %self.task_key(),
            "未实现的空操作：本次未清理任何 RustFS 对象（磁盘增长需另行实现清理逻辑）"
        );
        Ok(())
    }
}
