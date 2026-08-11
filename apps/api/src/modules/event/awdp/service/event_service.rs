//! AWDP run 生命周期：Break→Fix 切换（plan §19，run 中心化）。

use bollard::Docker;
use chrono::Utc;
use sea_orm::DatabaseConnection;
use uuid::Uuid;

use crate::entity::sea_orm_active_enums::AwdpPhase;
use crate::modules::event::awdp::{
    AwdpResult,
    domain::AwdpConfig,
    repo::{instance_repo, round_repo, run_repo},
    service::runtime,
};

/// Break → Fix：
///   1. CAS 阶段迁移（Fix 时间戳 + 预生成回合时间线 + next_action_at = 首个 cutoff）；
///   2. 所有已启动实例 reset 到 pristine（Break writable layer 清除；端点/逻辑实例保留）。
pub async fn transition_break_to_fix(
    db: &DatabaseConnection,
    docker: &Docker,
    jwt_secret: &[u8],
    run_id: Uuid,
) -> AwdpResult<()> {
    let run = run_repo::require_by_id(db, run_id).await?;
    let config = AwdpConfig::from_run(&run);
    config.validate()?;

    let now = Utc::now();
    let fix_ends = now + chrono::Duration::seconds(config.fix_duration_secs as i64);
    let first_cutoff = now + chrono::Duration::seconds(config.fix_round_interval_secs as i64);

    // CAS：Break → Fix。
    run_repo::transition_phase(
        db,
        run_id,
        AwdpPhase::Break,
        AwdpPhase::Fix,
        run_repo::PhaseTransitionPatch {
            fix_started_at: Some(now),
            fix_ends_at: Some(fix_ends),
            current_round: Some(0),
            next_action_at: Some(first_cutoff),
            ..Default::default()
        },
    )
    .await?;

    // 确定性预生成回合时间线（幂等）。
    round_repo::materialize_rounds(db, run_id).await?;

    // 全部已启动实例 reset 到 pristine（保留 logical instance + 端点分配）。
    reset_all_run_instances(db, docker, jwt_secret, run_id).await?;
    Ok(())
}

/// run 下全部已启动实例 reset 到 pristine（Break→Fix / 管理端 / 玩家 run reset）。
pub async fn reset_all_run_instances(
    db: &DatabaseConnection,
    docker: &Docker,
    jwt_secret: &[u8],
    run_id: Uuid,
) -> AwdpResult<usize> {
    let rows = instance_repo::list_for_run(db, run_id).await?;
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
