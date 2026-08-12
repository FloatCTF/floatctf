//! awdp_runs 仓储：AWDP 生命周期根（Practice 与 Competition 共用引擎状态）。
//!
//! run 承载：phase 状态机、配置快照 ×5、timing、current_round/total_rounds、next_action_at。
//! 阶段迁移（CAS）、tick 扫描（FOR UPDATE SKIP LOCKED）都收敛在本仓储。

use chrono::{DateTime, Utc};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseConnection,
    EntityTrait, QueryFilter, QueryOrder, QuerySelect, TransactionTrait, sea_query::LockType,
};
use uuid::Uuid;

use crate::entity::{awdp_runs, events, gameboxes, sea_orm_active_enums::AwdpPhase, users};
use crate::modules::event::awdp::{
    AwdpError, AwdpResult,
    domain::{AwdpConfig, AwdpPhaseExt},
};

pub async fn find_by_id<C: ConnectionTrait>(
    db: &C,
    run_id: Uuid,
) -> Result<Option<awdp_runs::Model>, sea_orm::DbErr> {
    awdp_runs::Entity::find_by_id(run_id).one(db).await
}

pub async fn require_by_id<C: ConnectionTrait>(
    db: &C,
    run_id: Uuid,
) -> AwdpResult<awdp_runs::Model> {
    find_by_id(db, run_id)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?
        .ok_or_else(|| AwdpError::NotFound(format!("awdp run {run_id} not found")))
}

/// 幂等确保 AWDP 练习系统虚拟赛事 `AWDPlusPractice` 存在（练习模块单挂载点）。
/// system_key = 'awdp-practice'（固定，唯一）；固定主键 `EVENT_PRACTICE_AWDP`。
/// 虚拟 event：family=awdp, purpose=practice, participant_mode=individual，
/// hidden=true + is_virtual=true（不出现在赛事列表）。
pub async fn ensure_practice_event(db: &DatabaseConnection) -> AwdpResult<Uuid> {
    use crate::core::system_ids::{EVENT_PRACTICE_AWDP, EVENT_PRACTICE_AWDP_SYSTEM_KEY};
    let system_key = EVENT_PRACTICE_AWDP_SYSTEM_KEY;

    let find = || async {
        events::Entity::find()
            .filter(events::Column::SystemKey.eq(system_key))
            .one(db)
            .await
            .map(|e| e.map(|e| e.id))
    };
    if let Some(id) = find()
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?
    {
        return Ok(id);
    }

    let now = Utc::now().fixed_offset();
    let model = events::ActiveModel {
        id: Set(EVENT_PRACTICE_AWDP),
        family: Set(crate::entity::sea_orm_active_enums::EventFamily::Awdp),
        purpose: Set(crate::entity::sea_orm_active_enums::EventPurpose::Practice),
        participant_mode: Set(crate::entity::sea_orm_active_enums::ParticipantMode::Individual),
        system_key: Set(Some(system_key.to_string())),
        title: Set("AWDPlusPractice".into()),
        description: Set(Some(
            "AWDP 练习（虚拟赛事）：练习模块 gamebox 统一挂载点".into(),
        )),
        hidden: Set(true),
        allow_join: Set(false),
        start_time: Set(now),
        end_time: Set(None),
        rules: Set(String::new()),
        flag_prefix: Set(None),
        is_virtual: Set(true),
        created_at: Set(now),
        updated_at: Set(now),
    };
    match model.insert(db).await {
        Ok(m) => {
            // 新库首建：把当前全部可训练的 gamebox 批量挂载到 AWDPlusPractice
            // （练习模块 gamebox 都挂载到这个虚拟赛事；后续新增 gamebox 由
            // start_training 的 ensure_mounted 按需补挂）。
            if let Err(e) = mount_all_trainable_gameboxes(db, m.id).await {
                tracing::warn!(event_id = %m.id, error = %e, "batch mount practice gameboxes skipped");
            }
            Ok(m.id)
        }
        // 并发 ensure：system_key 唯一冲突 → 重新查询。
        Err(_) => find()
            .await
            .map_err(|e| AwdpError::Database(e.to_string()))?
            .ok_or_else(|| AwdpError::Database("virtual event create race".into())),
    }
}

/// 批量挂载当前全部可训练 gamebox 到 `AWDPlusPractice`（练习模块全量挂载）。
async fn mount_all_trainable_gameboxes(
    db: &DatabaseConnection,
    event_id: Uuid,
) -> AwdpResult<usize> {
    use crate::modules::gamebox::BUILD_STATUS_READY;
    let gbs = gameboxes::Entity::find()
        .filter(gameboxes::Column::Hidden.eq(false))
        .filter(gameboxes::Column::BuildStatus.eq(BUILD_STATUS_READY))
        .filter(gameboxes::Column::AwdpSourceArtifactKey.is_not_null())
        .all(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
    let mut n = 0usize;
    for gb in gbs {
        match crate::modules::event::awdp::repo::event_gamebox_repo::ensure_mounted(
            db, event_id, gb.id,
        )
        .await
        {
            Ok(_) => n += 1,
            Err(e) => {
                tracing::warn!(gamebox_id = %gb.id, error = %e, "mount trainable gamebox skipped")
            }
        }
    }
    Ok(n)
}

/// 创建 practice run：gamebox × owner_user（个人练习，统一挂 `AWDPlusPractice` 虚拟 event）。
/// 创建即 phase=Break（started_at / break_ends_at / next_action_at 由 snapshot 计算）。
/// 同 user+gamebox 已有 active run → Conflict（幂等判定在 service 层完成）。
pub async fn create_practice_run(
    db: &DatabaseConnection,
    gamebox_id: Uuid,
    owner_user_id: Uuid,
    config: &AwdpConfig,
) -> AwdpResult<awdp_runs::Model> {
    config.validate()?;
    let event_id = ensure_practice_event(db).await?;
    let now = Utc::now();
    let break_ends = now + chrono::Duration::seconds(config.break_duration_secs as i64);
    let model = awdp_runs::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        gamebox_id: Set(Some(gamebox_id)),
        owner_user_id: Set(Some(owner_user_id)),
        owner_team_id: Set(None),
        phase: Set(AwdpPhase::Break),
        break_duration_secs: Set(config.break_duration_secs),
        fix_duration_secs: Set(config.fix_duration_secs),
        fix_round_interval_secs: Set(config.fix_round_interval_secs),
        break_score: Set(config.break_score),
        fix_round_score: Set(config.fix_round_score),
        started_at: Set(Some(now.into())),
        break_ends_at: Set(Some(break_ends.into())),
        current_round: Set(0),
        total_rounds: Set(config.total_rounds()),
        next_action_at: Set(Some(break_ends.into())),
        created_at: Set(now.into()),
        updated_at: Set(now.into()),
        ..Default::default()
    };
    match model.insert(db).await {
        Ok(m) => Ok(m),
        Err(e) if e.to_string().contains("awdp_runs_event_active_uidx") => Err(
            AwdpError::Conflict("该 GameBox 已有一个进行中的训练 run".into()),
        ),
        Err(e) => Err(AwdpError::Database(e.to_string())),
    }
}

/// 创建 competition run：Event 级共享（participant 主体在域表）。
/// phase=pending，next_action_at=events.start_time（tick 到点自动 Break；admin start 立即 Break）。
/// 幂等：已有 active run 直接返回；已结束 event 拒绝重复启动。
pub async fn create_competition_run(
    db: &DatabaseConnection,
    event_id: Uuid,
    config: &AwdpConfig,
) -> AwdpResult<awdp_runs::Model> {
    config.validate()?;

    if let Some(existing) = find_active_competition_for_event(db, event_id).await? {
        return Ok(existing);
    }
    if list_for_event(db, event_id)
        .await?
        .iter()
        .any(|r| r.phase == AwdpPhase::Ended)
    {
        return Err(AwdpError::InvalidState(
            "该事件已经结束，不能重新启动".into(),
        ));
    }

    let start_time = events::Entity::find_by_id(event_id)
        .one(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?
        .ok_or_else(|| AwdpError::NotFound("event not found".into()))?
        .start_time
        .with_timezone(&Utc);

    let now = Utc::now();
    let model = awdp_runs::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        gamebox_id: Set(None),
        owner_user_id: Set(None),
        owner_team_id: Set(None),
        phase: Set(AwdpPhase::Pending),
        break_duration_secs: Set(config.break_duration_secs),
        fix_duration_secs: Set(config.fix_duration_secs),
        fix_round_interval_secs: Set(config.fix_round_interval_secs),
        break_score: Set(config.break_score),
        fix_round_score: Set(config.fix_round_score),
        current_round: Set(0),
        total_rounds: Set(config.total_rounds()),
        next_action_at: Set(Some(start_time.into())),
        created_at: Set(now.into()),
        updated_at: Set(now.into()),
        ..Default::default()
    };
    match model.insert(db).await {
        Ok(m) => Ok(m),
        // 并发幂等：active unique 冲突 → 返回既有 active run。
        Err(e) if e.to_string().contains("awdp_runs_event_active_uidx") => {
            find_active_competition_for_event(db, event_id)
                .await?
                .ok_or_else(|| AwdpError::Internal("competition run missing after conflict".into()))
        }
        Err(e) => Err(AwdpError::Database(e.to_string())),
    }
}

/// 同 user+gamebox 的 active practice run（pending/break/fix）。
pub async fn find_active_practice_for<C: ConnectionTrait>(
    db: &C,
    gamebox_id: Uuid,
    owner_user_id: Uuid,
) -> AwdpResult<Option<awdp_runs::Model>> {
    use sea_orm::sea_query::Condition;
    Ok(awdp_runs::Entity::find()
        .filter(awdp_runs::Column::GameboxId.eq(gamebox_id))
        .filter(awdp_runs::Column::OwnerUserId.eq(owner_user_id))
        .filter(
            Condition::any()
                .add(awdp_runs::Column::Phase.eq(AwdpPhase::Pending))
                .add(awdp_runs::Column::Phase.eq(AwdpPhase::Break))
                .add(awdp_runs::Column::Phase.eq(AwdpPhase::Fix)),
        )
        .one(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?)
}

/// 事件的 active competition run（pending/break/fix）。
pub async fn find_active_competition_for_event<C: ConnectionTrait>(
    db: &C,
    event_id: Uuid,
) -> AwdpResult<Option<awdp_runs::Model>> {
    use sea_orm::sea_query::Condition;
    Ok(awdp_runs::Entity::find()
        .filter(awdp_runs::Column::EventId.eq(event_id))
        .filter(
            Condition::any()
                .add(awdp_runs::Column::Phase.eq(AwdpPhase::Pending))
                .add(awdp_runs::Column::Phase.eq(AwdpPhase::Break))
                .add(awdp_runs::Column::Phase.eq(AwdpPhase::Fix)),
        )
        .one(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?)
}

/// 事件的全部 run（管理端 inspect；含 ended 历史）。
pub async fn list_for_event<C: ConnectionTrait>(
    db: &C,
    event_id: Uuid,
) -> AwdpResult<Vec<awdp_runs::Model>> {
    awdp_runs::Entity::find()
        .filter(awdp_runs::Column::EventId.eq(event_id))
        .order_by_asc(awdp_runs::Column::CreatedAt)
        .all(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))
}

/// 用户的全部 practice run（“我的训练历史”）。
pub async fn list_for_owner(
    db: &DatabaseConnection,
    owner_user_id: Uuid,
) -> AwdpResult<Vec<awdp_runs::Model>> {
    awdp_runs::Entity::find()
        .filter(awdp_runs::Column::OwnerUserId.eq(owner_user_id))
        .order_by_desc(awdp_runs::Column::CreatedAt)
        .all(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))
}

/// 阶段迁移要同步写入的时间戳/进度字段。
#[derive(Debug, Default, Clone)]
pub struct PhaseTransitionPatch {
    pub started_at: Option<DateTime<Utc>>,
    pub break_ends_at: Option<DateTime<Utc>>,
    pub fix_started_at: Option<DateTime<Utc>>,
    pub fix_ends_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub current_round: Option<i32>,
    pub next_action_at: Option<DateTime<Utc>>,
}

/// 阶段迁移：lock → can_transition_to → CAS → 原子 UPDATE（单写者）。
pub async fn transition_phase(
    db: &DatabaseConnection,
    run_id: Uuid,
    expected: AwdpPhase,
    target: AwdpPhase,
    patch: PhaseTransitionPatch,
) -> AwdpResult<()> {
    expected
        .can_transition_to(target.clone())
        .map_err(AwdpError::InvalidState)?;

    let txn = db
        .begin()
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;

    let current = awdp_runs::Entity::find_by_id(run_id)
        .lock(LockType::Update)
        .one(&txn)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?
        .ok_or_else(|| AwdpError::NotFound("awdp run not found".into()))?;

    // CAS：并发 phase 迁移拒绝。
    if current.phase != expected {
        txn.rollback()
            .await
            .map_err(|e| AwdpError::Database(e.to_string()))?;
        return Err(AwdpError::Conflict(format!(
            "concurrent AWDP phase transition: current={:?} expected={expected:?}",
            current.phase
        )));
    }

    let now = Utc::now();
    let mut am: awdp_runs::ActiveModel = current.clone().into();
    am.phase = Set(target);
    if let Some(v) = patch.started_at {
        am.started_at = Set(Some(v.into()));
    }
    if let Some(v) = patch.break_ends_at {
        am.break_ends_at = Set(Some(v.into()));
    }
    if let Some(v) = patch.fix_started_at {
        am.fix_started_at = Set(Some(v.into()));
    }
    if let Some(v) = patch.fix_ends_at {
        am.fix_ends_at = Set(Some(v.into()));
    }
    if let Some(v) = patch.finished_at {
        am.finished_at = Set(Some(v.into()));
    }
    if let Some(v) = patch.current_round {
        am.current_round = Set(v);
    }
    if let Some(v) = patch.next_action_at {
        am.next_action_at = Set(Some(v.into()));
    }
    am.updated_at = Set(now.into());
    am.update(&txn)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;

    txn.commit()
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
    Ok(())
}

/// 轻量更新 next_action_at / current_round（tick 推进用）。
pub async fn touch_tick_state(
    db: &DatabaseConnection,
    run_id: Uuid,
    current_round: i32,
    next_action_at: DateTime<Utc>,
) -> AwdpResult<()> {
    let now = Utc::now();
    let mut am: awdp_runs::ActiveModel = awdp_runs::Entity::find_by_id(run_id)
        .one(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?
        .ok_or_else(|| AwdpError::NotFound("awdp run not found".into()))?
        .into();
    am.current_round = Set(current_round);
    am.next_action_at = Set(Some(next_action_at.into()));
    am.updated_at = Set(now.into());
    am.update(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
    Ok(())
}

/// tick 扫描：due runs（next_action_at <= now AND phase <> ended）FOR UPDATE SKIP LOCKED。
pub async fn find_due_runs(
    db: &DatabaseConnection,
    now: DateTime<Utc>,
    limit: u64,
) -> AwdpResult<Vec<awdp_runs::Model>> {
    use sea_orm::{QuerySelect, sea_query::LockBehavior};

    let rows = awdp_runs::Entity::find()
        .filter(awdp_runs::Column::NextActionAt.lte(now))
        .filter(awdp_runs::Column::Phase.ne(AwdpPhase::Ended))
        .order_by_asc(awdp_runs::Column::NextActionAt)
        .limit(limit)
        .lock_with_behavior(LockType::Update, LockBehavior::SkipLocked)
        .all(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
    Ok(rows)
}

/// run 的 event（域表 team 复合 FK → event_teams(event_id,id) 需要；practice 也是虚拟 event）。
pub async fn event_id_for_team_fk(db: &DatabaseConnection, run_id: Uuid) -> AwdpResult<Uuid> {
    Ok(require_by_id(db, run_id).await?.event_id)
}
