//! AWDP Break：flag 提交 → 一次性计分（plan §17/§18/§35）。

use sea_orm::DatabaseConnection;
use uuid::Uuid;

use crate::entity::sea_orm_active_enums::AwdpPhase;
use crate::infrastructure::settings::get_setting;
use crate::modules::event::awdp::{
    AwdpError, AwdpResult,
    domain::{break_idempotency_key, flag::awdp_flag, flag::hash_flag},
    repo::{break_repo, event_gamebox_repo, event_repo, score_repo},
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
    event_id: Uuid,
    event_gamebox_id: Uuid,
    flag: &str,
    subject: Subject,
) -> AwdpResult<BreakSubmissionResult> {
    let awdp = event_repo::require_by_event_id(db, event_id).await?;
    if awdp.phase != AwdpPhase::Break {
        return Err(AwdpError::InvalidState(format!(
            "flag submission only allowed during Break (phase={:?})",
            awdp.phase
        )));
    }
    let eg = event_gamebox_repo::require_by_id(db, event_gamebox_id).await?;
    if eg.event_id != event_id {
        return Err(AwdpError::Validation(
            "event_gamebox does not belong to this event".into(),
        ));
    }

    // 校验 flag（确定性 HMAC，双主体绑定）。
    let flag_prefix = get_setting(db, "FLAG_PREFIX")
        .await
        .unwrap_or_else(|_| "flag".into());
    let expected = awdp_flag(
        jwt_secret,
        event_id,
        event_gamebox_id,
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
        event_id,
        event_gamebox_id,
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
    let key = break_idempotency_key(event_id, event_gamebox_id, subject.user_id, subject.team_id);
    let _scored = score_repo::create_score_event(
        db,
        event_id,
        subject.user_id,
        subject.team_id,
        event_gamebox_id,
        "break",
        None,
        awdp.break_score,
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
    event_id: Uuid,
    event_gamebox_id: Uuid,
    subject: Subject,
) -> AwdpResult<bool> {
    break_repo::already_broken(
        db,
        event_id,
        event_gamebox_id,
        subject.user_id,
        subject.team_id,
    )
    .await
}
