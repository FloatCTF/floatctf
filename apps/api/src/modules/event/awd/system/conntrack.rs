//! conntrack 连接跟踪清理。

use crate::modules::event::awd::{
    AwdError, AwdResult,
    system::command::{CommandRunner, conntrack_cmd},
};

/// 刷新指定 CIDR 的 conntrack 条目。
pub async fn flush_for_cidr(runner: &dyn CommandRunner, cidr: &str) -> AwdResult<()> {
    conntrack_cmd::flush_event(runner, cidr)
        .await
        .map_err(|e| AwdError::Network(format!("Conntrack flush failed for {}: {}", cidr, e)))?;
    Ok(())
}

/// 刷新整场赛事 GameBox 子网的 conntrack 条目。
pub async fn flush_event_gamebox_traffic(
    runner: &dyn CommandRunner,
    gamebox_cidr: &str,
) -> AwdResult<()> {
    flush_for_cidr(runner, gamebox_cidr).await
}
