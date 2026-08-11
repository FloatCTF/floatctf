//! AWDP tick：单写者阶段推进 + round cutoff 物化（plan §30/§31）。
//!
//! 不做 participant×gamebox×round 海量 scheduler task —— 全部由 ONE recurring
//! `awdp.tick`（cron 10s）驱动；evaluation 行是 domain 数据，由独立 worker 消费。

use bollard::Docker;
use chrono::Utc;
use sea_orm::{DatabaseConnection, EntityTrait};
use tracing::{info, warn};

use crate::entity::sea_orm_active_enums::AwdpPhase;
use crate::modules::event::awdp::{
    AwdpResult,
    domain::AwdpConfig,
    repo::{event_repo, instance_repo, round_repo},
    service::{evaluation::materialize_official_evaluations, event_service},
};

/// 一次 tick 处理全部 due 事件（最多 N 个）。
pub async fn tick_once(
    db: &DatabaseConnection,
    docker: &Docker,
    jwt_secret: &[u8],
) -> AwdpResult<TickSummary> {
    let now = Utc::now();
    let due = event_repo::find_due_events(db, now, 10).await?;
    let mut summary = TickSummary::default();

    for ev in due {
        match ev.phase {
            AwdpPhase::Pending => {
                // pending + start due → Break。
                let event = crate::entity::events::Entity::find_by_id(ev.event_id)
                    .one(db)
                    .await
                    .map_err(|e| crate::modules::event::awdp::AwdpError::Database(e.to_string()))?;
                let Some(event) = event else { continue };
                if event.start_time.with_timezone(&Utc) <= now {
                    let config = AwdpConfig {
                        break_duration_secs: ev.break_duration_secs,
                        fix_duration_secs: ev.fix_duration_secs,
                        fix_round_interval_secs: ev.fix_round_interval_secs,
                        break_score: ev.break_score,
                        fix_round_score: ev.fix_round_score,
                    };
                    let break_ends =
                        now + chrono::Duration::seconds(config.break_duration_secs as i64);
                    match event_repo::transition_phase(
                        db,
                        ev.event_id,
                        AwdpPhase::Pending,
                        AwdpPhase::Break,
                        event_repo::PhaseTransitionPatch {
                            started_at: Some(now),
                            break_ends_at: Some(break_ends),
                            next_action_at: Some(break_ends),
                            ..Default::default()
                        },
                    )
                    .await
                    {
                        Ok(()) => {
                            info!(event_id = %ev.event_id, "AWDP event started (Break)");
                            summary.started += 1;
                        }
                        Err(e) => warn!(event_id = %ev.event_id, error = %e, "AWDP start skipped"),
                    }
                }
            }
            AwdpPhase::Break => {
                // Break 到期 → Fix（CAS；重复 tick 幂等）。
                match event_service::transition_break_to_fix(db, docker, jwt_secret, ev.event_id)
                    .await
                {
                    Ok(()) => {
                        info!(event_id = %ev.event_id, "AWDP break expired → Fix");
                        summary.to_fix += 1;
                    }
                    Err(e) => warn!(event_id = %ev.event_id, error = %e, "AWDP break→fix skipped"),
                }
            }
            AwdpPhase::Fix => {
                // round cutoff due → 物化 official evaluations + 推进 current_round。
                let processed = process_fix_round_cutoffs(db, ev.event_id, now).await?;
                summary.round_cutoffs += processed;
            }
            AwdpPhase::Ended => {} // 已结束事件不会进入 due 集合（find_due_events 排除）。
        }
    }

    // 兜底：round 评估全部完成 → 标记 completed（worker 也会做，这里幂等）。
    summary.rounds_completed = round_repo::complete_finished_rounds(db).await?;

    Ok(summary)
}

/// 处理 Fix 阶段到达 cutoff 的回合：物化评估 + 推进 next_action_at。
/// 幂等：round 状态 pending→evaluating 只发生一次。
async fn process_fix_round_cutoffs(
    db: &DatabaseConnection,
    event_id: uuid::Uuid,
    now: chrono::DateTime<Utc>,
) -> AwdpResult<usize> {
    let mut processed = 0usize;
    loop {
        let Some(round) = round_repo::next_pending_due_round(db, event_id, now).await? else {
            break;
        };

        // 标记 evaluating（CAS 由 next_pending_due_round 的状态过滤保证单次）。
        round_repo::set_status(db, round.id, "evaluating").await?;

        // 物化该 round 的全部 official evaluation（事件内已启动实例）。
        let instances = instance_repo::list_for_event(db, event_id).await?;
        let mut created = 0usize;
        for (instance, ext) in &instances {
            if instance.runtime_state == "pending" {
                continue; // 未启动 = 未参赛
            }
            if materialize_official_evaluations(db, event_id, instance.id, round.id).await? {
                created += 1;
            }
        }
        info!(
            event_id = %event_id,
            round = round.sequence,
            evaluations = created,
            "AWDP round cutoff materialized"
        );
        processed += 1;

        // 推进：next_action_at = 下一个回合 cutoff 或 Fix 结束。
        let awdp = event_repo::require_by_event_id(db, event_id).await?;
        match round_repo::next_pending_round(db, event_id).await? {
            Some(next) => {
                event_repo::touch_tick_state(
                    db,
                    event_id,
                    next.sequence,
                    next.cutoff_at.with_timezone(&Utc),
                )
                .await?;
            }
            None => {
                // 最后一轮：标记 Ended（评估仍由 worker 完成并计分）。
                let fix_end = awdp
                    .fix_ends_at
                    .map(|t| t.with_timezone(&Utc))
                    .unwrap_or_else(|| {
                        now + chrono::Duration::seconds(awdp.fix_duration_secs as i64)
                    });
                match event_repo::transition_phase(
                    db,
                    event_id,
                    AwdpPhase::Fix,
                    AwdpPhase::Ended,
                    event_repo::PhaseTransitionPatch {
                        finished_at: Some(fix_end),
                        next_action_at: None,
                        current_round: Some(awdp.current_round),
                        ..Default::default()
                    },
                )
                .await
                {
                    Ok(()) => info!(event_id = %event_id, "AWDP fix ended (final round cutoff)"),
                    Err(e) => warn!(event_id = %event_id, error = %e, "AWDP finalize skipped"),
                }
            }
        }

        // 防止极端情况下同一 tick 无限循环（每轮最多推进一轮；外层 tick 再次触发）。
        break;
    }
    Ok(processed)
}

/// 汇总信息（日志/可观测）。
#[derive(Debug, Default)]
pub struct TickSummary {
    pub started: usize,
    pub to_fix: usize,
    pub round_cutoffs: usize,
    pub rounds_completed: usize,
}
