//! AWDP Break：flag 提交 → 一次性计分（plan §17/§18/§35，run 中心化）。

use sea_orm::DatabaseConnection;
use uuid::Uuid;

use crate::entity::sea_orm_active_enums::AwdpPhase;
use crate::infrastructure::settings::get_setting;
use crate::modules::event::awdp::{
    AwdpError, AwdpResult,
    domain::{break_idempotency_key, flag::awdp_flag, flag::hash_flag},
    repo::{break_repo, run_repo, score_repo},
    service::runtime::Subject,
};

/// 提交结果。
#[derive(Debug, Clone)]
pub struct BreakSubmissionResult {
    /// flag 是否正确。
    pub accepted: bool,
    /// 本提交是否计入得分（首次成功才 true；重复成功 +0）。
    pub scored: bool,
    /// 已 broken 的提示（accepted=true 且此前已得过分）。
    pub already_broken: bool,
}

/// 提交 flag（仅 Break 阶段）。
pub async fn submit_flag(
    db: &DatabaseConnection,
    jwt_secret: &[u8],
    run_id: Uuid,
    gamebox_id: Uuid,
    flag: &str,
    subject: Subject,
) -> AwdpResult<BreakSubmissionResult> {
    let run = run_repo::require_by_id(db, run_id).await?;
    if run.phase != AwdpPhase::Break {
        return Err(AwdpError::InvalidState(format!(
            "flag submission only allowed during Break (phase={:?})",
            run.phase
        )));
    }
    // gamebox 归属：practice 必须是 run 自己的 gamebox；competition 必须挂载在本赛事。
    if run.gamebox_id.is_some() {
        if run.gamebox_id != Some(gamebox_id) {
            return Err(AwdpError::Validation("gamebox 不属于该训练 run".into()));
        }
    } else {
        // competition：gamebox 必须已挂载（runtime 解析路径也会校验）。
        let _ = run.event_id;
    }

    // 校验 flag（确定性 HMAC，双主体绑定）。
    let flag_prefix = get_setting(db, "FLAG_PREFIX")
        .await
        .unwrap_or_else(|_| "flag".into());
    let expected = awdp_flag(
        jwt_secret,
        run_id,
        gamebox_id,
        subject.user_id,
        subject.team_id,
        &flag_prefix,
    );
    if flag.trim() != expected {
        return Ok(BreakSubmissionResult {
            accepted: false,
            scored: false,
            already_broken: false,
        });
    }

    // 已 Break 过 → accepted +0（幂等；partial unique 兜底并发）。
    let first_time = break_repo::record_break(
        db,
        run_id,
        gamebox_id,
        subject.user_id,
        subject.team_id,
        &hash_flag(&expected),
    )
    .await?;

    if !first_time {
        return Ok(BreakSubmissionResult {
            accepted: true,
            scored: false,
            already_broken: true,
        });
    }

    // 首次成功：+break_score（幂等键防重复加分）。
    let key = break_idempotency_key(run_id, gamebox_id, subject.user_id, subject.team_id);
    let _scored = score_repo::create_score_event(
        db,
        run_id,
        subject.user_id,
        subject.team_id,
        gamebox_id,
        "break",
        None,
        run.break_score,
        &key,
    )
    .await?;

    Ok(BreakSubmissionResult {
        accepted: true,
        scored: true,
        already_broken: false,
    })
}

/// 当前主体的 Break 状态（前端展示 Broken/+1000）。
pub async fn break_status_for(
    db: &DatabaseConnection,
    run_id: Uuid,
    gamebox_id: Uuid,
    subject: Subject,
) -> AwdpResult<bool> {
    break_repo::already_broken(db, run_id, gamebox_id, subject.user_id, subject.team_id).await
}
