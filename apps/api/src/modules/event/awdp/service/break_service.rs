//! AWDP Break：flag 提交 → 一次性计分（plan §17/§18/§35，run 中心化）。

use sea_orm::DatabaseConnection;
use uuid::Uuid;

use crate::entity::sea_orm_active_enums::AwdpPhase;
use crate::infrastructure::settings::get_setting;
use crate::modules::event::awdp::{
    AwdpError, AwdpResult,
    domain::{
        break_idempotency_key,
        flag::{awdp_flag, hash_flag},
    },
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

/// 解析 Break flag（GameBox → JudgeServer `/flag` → FloatCTF internal API，plan §8/§9）。
///
/// `event_id`：请求所属赛事（JudgeServer 转发时携带；决定解析哪个 data 网络）。
/// `source_ip`（真实 TCP peer）是**唯一**身份输入——GameBox 不提供任何身份参数。
/// 验证链：
///   1. 赛事 data 网络 inspect → 找到持有 source_ip 的容器（current physical attachment）；
///   2. 容器 → event_instances（container_name），要求 runtime_state == running；
///   3. awdp_instances → run，要求 run.phase == Break（Fix/PreparingFix/Ended 拒绝）；
///   4. 用 AWDP 确定性 flag 域派生对应 flag（HMAC 绑定 run×gamebox×subject）。
///
/// 未知 source / 非 running / 非 Break → 403/409（不给内部信息）。
pub async fn resolve_break_flag(
    db: &DatabaseConnection,
    docker: &bollard::Docker,
    jwt_secret: &[u8],
    event_id: Uuid,
    source_ip: &str,
) -> AwdpResult<String> {
    use crate::modules::event::awdp::service::network_resolve;

    // 1-2. 当前 data 网络附着 → 容器 → 运行中实例（真实 IP 是事实来源）。
    let (instance, ext) =
        network_resolve::resolve_instance_by_network_ip(db, docker, event_id, source_ip).await?;

    // 3. 实例 → run（instance 属于该 run 由 awdp_instances 1:1 保证）。
    let run = run_repo::require_by_id(db, ext.run_id).await?;
    if run.phase != AwdpPhase::Break {
        // 练习模式（gamebox_id 非空）Fix 阶段：返回固定 proof-of-exploit 标记——
        // 让 exploit（SSRF 偷 flag）真实判定漏洞是否仍可利用，且不泄露真实 flag；
        // 竞赛与非练习 run 保持仅 Break 发 flag。
        if run.gamebox_id.is_some() && run.phase == AwdpPhase::Fix {
            return Ok("flag{proof-of-exploit}".to_string());
        }
        // Break 以外（Fix/PreparingFix/Ended）：flag 不可用。
        return Err(AwdpError::Conflict(format!(
            "flag only available in Break phase (phase={:?})",
            run.phase
        )));
    }

    // 4. 派生确定性 flag。
    let flag_prefix = get_setting(db, "FLAG_PREFIX")
        .await
        .unwrap_or_else(|_| "flag".into());
    let flag = awdp_flag(
        jwt_secret,
        run.id,
        ext.gamebox_id,
        ext.owner_user_id,
        ext.owner_team_id,
        &flag_prefix,
    );
    Ok(flag)
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
