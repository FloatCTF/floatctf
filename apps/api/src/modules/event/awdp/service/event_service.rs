//! AWDP 事件生命周期：Break→Fix 切换（plan §19）。

use bollard::Docker;
use chrono::Utc;
use sea_orm::DatabaseConnection;
use uuid::Uuid;

use crate::entity::sea_orm_active_enums::AwdpPhase;
use crate::modules::event::awdp::{
    AwdpResult,
    domain::AwdpConfig,
    repo::{event_repo, instance_repo, round_repo},
    service::runtime,
};

/// Break → Fix：
///   1. CAS 阶段迁移（Fix 时间戳 + 预生成回合时间线 + next_action_at = 首个 cutoff）；
///   2. 所有已启动实例 reset 到 pristine（Break writable layer 清除；端点/逻辑实例保留）。
pub async fn transition_break_to_fix(
    db: &DatabaseConnection,
    docker: &Docker,
    jwt_secret: &[u8],
    event_id: Uuid,
) -> AwdpResult<()> {
    let row = event_repo::require_by_event_id(db, event_id).await?;
    let config = AwdpConfig {
        break_duration_secs: row.break_duration_secs,
        fix_duration_secs: row.fix_duration_secs,
        fix_round_interval_secs: row.fix_round_interval_secs,
        break_score: row.break_score,
        fix_round_score: row.fix_round_score,
    };
    config.validate()?;

    let now = Utc::now();
    let fix_ends = now + chrono::Duration::seconds(config.fix_duration_secs as i64);
    let first_cutoff = now + chrono::Duration::seconds(config.fix_round_interval_secs as i64);

    // CAS：Break → Fix。
    event_repo::transition_phase(
        db,
        event_id,
        AwdpPhase::Break,
        AwdpPhase::Fix,
        event_repo::PhaseTransitionPatch {
            fix_started_at: Some(now),
            fix_ends_at: Some(fix_ends),
            current_round: Some(0),
            next_action_at: Some(first_cutoff),
            ..Default::default()
        },
    )
    .await?;

    // 确定性预生成回合时间线（幂等）。
    round_repo::materialize_rounds(db, event_id).await?;

    // 全部已启动实例 reset 到 pristine（保留 logical instance + 端点分配）。
    reset_all_instances(db, docker, jwt_secret, event_id).await?;
    Ok(())
}

/// 事件下全部已启动实例 reset 到 pristine（Break→Fix / 管理端）。
pub async fn reset_all_instances(
    db: &DatabaseConnection,
    docker: &Docker,
    jwt_secret: &[u8],
    event_id: Uuid,
) -> AwdpResult<usize> {
    let rows = instance_repo::list_for_event(db, event_id).await?;
    let mut n = 0usize;
    for (instance, _ext) in rows {
        if instance.runtime_state == "pending" {
            continue;
        }
        let flag_prefix = crate::infrastructure::settings::get_setting(db, "FLAG_PREFIX")
            .await
            .unwrap_or_else(|_| "flag".into());
        let _ =
            runtime::reset_instance_unchecked(db, docker, jwt_secret, instance.id, &flag_prefix)
                .await?;
        n += 1;
    }
    Ok(n)
}
