//! AWDP tick：单写者阶段推进 + round cutoff 物化（plan §30/§31，run 中心化）。
//!
//! 不做 participant×gamebox×round 海量 scheduler task —— 全部由 ONE recurring
//! `awdp.tick`（cron 10s）驱动；evaluation 行是 domain 数据，由独立 worker 消费。
//!
//! 流程（全部幂等）：
//!   0. 自动排期：start_time 已到且尚无 run 的 AWDP 事件 → 创建 pending competition run；
//!   1. find_due_runs（next_action_at <= now，FOR UPDATE SKIP LOCKED）；
//!   2. pending → Break（start_time 到点）/ break → Fix / fix → round cutoff 物化。

use bollard::Docker;
use chrono::Utc;
use sea_orm::{DatabaseConnection, EntityTrait};
use tracing::{info, warn};

use crate::entity::{events, sea_orm_active_enums::AwdpPhase};
use crate::modules::event::awdp::{
    AwdpResult,
    domain::AwdpConfig,
    repo::{event_repo, instance_repo, round_repo, run_repo},
    service::{evaluation::materialize_official_evaluations, event_service},
};

/// 一次 tick 处理全部 due runs（最多 N 个）。
pub async fn tick_once(
    db: &DatabaseConnection,
    docker: &Docker,
    jwt_secret: &[u8],
) -> AwdpResult<TickSummary> {
    let now = Utc::now();
    let mut summary = TickSummary::default();

    // 0. 自动排期：start_time 到点且无 run 的事件 → 建 pending run（tick 后续 pending 分支转 Break）。
    let unstarted = event_repo::find_unstarted_awdp_events(db, now, 10).await?;
    for ev in unstarted {
        let config = config_for_event(db, ev.id).await?;
        match run_repo::create_competition_run(db, ev.id, &config).await {
            Ok(run) => info!(run_id = %run.id, event_id = %ev.id, "AWDP pending run auto-created"),
            Err(e) => warn!(event_id = %ev.id, error = %e, "AWDP auto-start create skipped"),
        }
    }

    // 1. due runs（next_action_at <= now）。
    let due = run_repo::find_due_runs(db, now, 10).await?;
    for run in due {
        match run.phase {
            AwdpPhase::Pending => {
                // competition run：start_time 到点才 Break（admin start 已在 handler 内立即转 Break）。
                let Some(event_id) = run.event_id else {
                    continue;
                };
                let event = events::Entity::find_by_id(event_id)
                    .one(db)
                    .await
                    .map_err(|e| crate::modules::event::awdp::AwdpError::Database(e.to_string()))?;
                let Some(event) = event else { continue };
                if event.start_time.with_timezone(&Utc) <= now {
                    let config = AwdpConfig::from_run(&run);
                    let break_ends =
                        now + chrono::Duration::seconds(config.break_duration_secs as i64);
                    match run_repo::transition_phase(
                        db,
                        run.id,
                        AwdpPhase::Pending,
                        AwdpPhase::Break,
                        run_repo::PhaseTransitionPatch {
                            started_at: Some(now),
                            break_ends_at: Some(break_ends),
                            next_action_at: Some(break_ends),
                            ..Default::default()
                        },
                    )
                    .await
                    {
                        Ok(()) => {
                            info!(run_id = %run.id, "AWDP run started (Break)");
                            summary.started += 1;
                        }
                        Err(e) => warn!(run_id = %run.id, error = %e, "AWDP start skipped"),
                    }
                }
            }
            AwdpPhase::Break => {
                // Break 到期 → Fix（CAS；重复 tick 幂等）。
                match event_service::transition_break_to_fix(db, docker, jwt_secret, run.id).await {
                    Ok(()) => {
                        info!(run_id = %run.id, "AWDP break expired → Fix");
                        summary.to_fix += 1;
                    }
                    Err(e) => warn!(run_id = %run.id, error = %e, "AWDP break→fix skipped"),
                }
            }
            AwdpPhase::Fix => {
                // round cutoff due → 物化 official evaluations + 推进 current_round。
                let processed = process_fix_round_cutoffs(db, run.id, now).await?;
                summary.round_cutoffs += processed;
            }
            AwdpPhase::Ended => {} // 已结束 run 不会进入 due 集合（find_due_runs 排除）。
        }
    }

    // 兜底：round 评估全部完成 → 标记 completed（worker 也会做，这里幂等）。
    summary.rounds_completed = round_repo::complete_finished_rounds(db).await?;

    Ok(summary)
}

/// 事件配置（ensure 默认后读取，作为 run snapshot 源）。
async fn config_for_event(db: &DatabaseConnection, event_id: uuid::Uuid) -> AwdpResult<AwdpConfig> {
    let row = event_repo::ensure_by_event_id(db, event_id, &AwdpConfig::default()).await?;
    Ok(AwdpConfig {
        break_duration_secs: row.break_duration_secs,
        fix_duration_secs: row.fix_duration_secs,
        fix_round_interval_secs: row.fix_round_interval_secs,
        break_score: row.break_score,
        fix_round_score: row.fix_round_score,
    })
}

/// 处理 Fix 阶段到达 cutoff 的回合：物化评估 + 推进 next_action_at。
/// 幂等：round 状态 pending→evaluating 只发生一次。
async fn process_fix_round_cutoffs(
    db: &DatabaseConnection,
    run_id: uuid::Uuid,
    now: chrono::DateTime<Utc>,
) -> AwdpResult<usize> {
    let mut processed = 0usize;
    // 每 tick 至多推进一轮（外层 tick 再次触发），避免同一 tick 内无限循环。
    let Some(round) = round_repo::next_pending_due_round(db, run_id, now).await? else {
        return Ok(processed);
    };

    // 标记 evaluating（CAS 由 next_pending_due_round 的状态过滤保证单次）。
    round_repo::set_status(db, round.id, "evaluating").await?;

    // 物化该 round 的全部 official evaluation（run 内已启动实例）。
    let instances = instance_repo::list_for_run(db, run_id).await?;
    let mut created = 0usize;
    for (instance, _ext) in &instances {
        if instance.runtime_state == "pending" {
            continue; // 未启动 = 未参赛
        }
        if materialize_official_evaluations(db, run_id, instance.id, round.id).await? {
            created += 1;
        }
    }
    info!(
        run_id = %run_id,
        round = round.sequence,
        evaluations = created,
        "AWDP round cutoff materialized"
    );
    processed += 1;

    // 推进：next_action_at = 下一个回合 cutoff 或 Fix 结束。
    let run = run_repo::require_by_id(db, run_id).await?;
    match round_repo::next_pending_round(db, run_id).await? {
        Some(next) => {
            run_repo::touch_tick_state(
                db,
                run_id,
                next.sequence,
                next.cutoff_at.with_timezone(&Utc),
            )
            .await?;
        }
        None => {
            // 最后一轮：标记 Ended（评估仍由 worker 完成并计分）。
            let fix_end = run
                .fix_ends_at
                .map(|t| t.with_timezone(&Utc))
                .unwrap_or_else(|| now + chrono::Duration::seconds(run.fix_duration_secs as i64));
            match run_repo::transition_phase(
                db,
                run_id,
                AwdpPhase::Fix,
                AwdpPhase::Ended,
                run_repo::PhaseTransitionPatch {
                    finished_at: Some(fix_end),
                    next_action_at: None,
                    current_round: Some(run.current_round),
                    ..Default::default()
                },
            )
            .await
            {
                Ok(()) => info!(run_id = %run_id, "AWDP fix ended (final round cutoff)"),
                Err(e) => warn!(run_id = %run_id, error = %e, "AWDP finalize skipped"),
            }
        }
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
