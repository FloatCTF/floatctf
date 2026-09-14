//! Jeopardy Flag 提交统一入口（内部按 Purpose 分支）。

use anyhow::{Result, anyhow};

use crate::modules::event::jeopardy::application::{
    context::{EventContext, SubmitFlagRequest},
    participant::resolve_participant,
    submission_service::{JeopardySubmissionService, submit_practice},
};
use crate::modules::event::jeopardy::domain::policy::JeopardyPolicy;
use crate::modules::event::jeopardy::domain::solve::JeopardySubmitRequest;

/// Flag 提交唯一入口：练习 → 零分路径；竞赛 → 计分服务。
pub async fn submit_flag(ctx: &EventContext, sfr: SubmitFlagRequest) -> Result<()> {
    JeopardyPolicy::require_jeopardy_family(&ctx.event)?;
    let policy = JeopardyPolicy::from_event(&ctx.event).map_err(|e| anyhow!(e))?;
    let instance_id = sfr.instance_id.ok_or_else(|| anyhow!("no instance_id"))?;

    if !policy.contributes_to_official_score() {
        // 练习：得分恒为 0；允许复练（不插入第二行 solve）。
        let _ = policy.allows_retraining_after_solve();
        return submit_practice(
            ctx.db.get_ref(),
            ctx.docker.get_ref(),
            &ctx.user,
            instance_id,
            &sfr.flag,
        )
        .await;
    }

    // 竞赛计分路径
    ctx.should_user_joined().await?;
    ctx.should_ongoing()?;
    let participant = resolve_participant(ctx).await?;
    debug_assert_eq!(participant.subject.is_team(), policy.is_team());

    let service =
        JeopardySubmissionService::new(ctx.db.get_ref().clone(), ctx.docker.get_ref().clone());
    service
        .submit(JeopardySubmitRequest {
            event_id: ctx.event.id,
            user_id: ctx.user.id,
            instance_id,
            flag: sfr.flag,
            subject: participant.subject,
        })
        .await
}
