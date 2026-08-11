//! AWD 赛事配置仓储。

use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseConnection,
    EntityTrait, QueryFilter, QuerySelect, TransactionTrait,
};
use uuid::Uuid;

use crate::entity::{
    awd_events,
    sea_orm_active_enums::{AwdEventStatus, AwdPhase},
};
use crate::modules::event::awd::{
    AwdError, AwdResult,
    domain::{AwdEventStatusExt, AwdPhaseExt},
};

/// 状态切换时需要与 `status` **同事务原子写入**的附属字段。
///
/// Phase 0 引入：`transition_event` 是 AWD event status 的**唯一修改入口**，
/// 任何附带字段（verified_* / paused_phase / finished_at / started_at …）
/// 都必须通过本结构体携带，禁止在 service 层先写 status 再补字段。
#[derive(Debug, Default, Clone)]
pub struct TransitionPatch {
    /// 与状态切换同时写入的 phase（如 Paused 时置 `Pause`、Running 时恢复前 phase）。
    pub phase: Option<AwdPhase>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub verified_revision: Option<String>,
    pub verified_at: Option<chrono::DateTime<chrono::Utc>>,
    /// 暂停前所处的比赛阶段（resume 时恢复，P0-1b 迁移新增列）。
    pub paused_phase: Option<AwdPhase>,
    pub pause_remaining_secs: Option<i32>,
    pub finished_at: Option<chrono::DateTime<chrono::Utc>>,
    /// 置空 verified 标记（配置变更 / 重检时清除）。
    pub clear_verified: bool,
    /// 已验证配置代数（P2-9：Precheck 成功时记录 configuration_generation）。
    pub verified_generation: Option<i64>,
}

impl TransitionPatch {
    /// Verified：写入 verified_revision + verified_at（与状态同事务）。
    pub fn verified(revision: &str) -> Self {
        Self {
            verified_revision: Some(revision.to_string()),
            verified_at: Some(chrono::Utc::now()),
            ..Default::default()
        }
    }

    /// Verified：附配置代数（P2-9）。调用方先读 configuration_generation 再传入。
    pub fn verified_with_generation(revision: &str, generation: i64) -> Self {
        Self {
            verified_revision: Some(revision.to_string()),
            verified_at: Some(chrono::Utc::now()),
            verified_generation: Some(generation),
            ..Default::default()
        }
    }

    /// Finished：写入 finished_at（与状态同事务）。
    pub fn finished() -> Self {
        Self {
            finished_at: Some(chrono::Utc::now()),
            ..Default::default()
        }
    }

    /// Paused：置 phase=Pause、记录暂停前 phase 与剩余秒数。
    pub fn paused(paused_from: AwdPhase, remaining_secs: i32) -> Self {
        Self {
            phase: Some(AwdPhase::Pause),
            paused_phase: Some(paused_from),
            pause_remaining_secs: Some(remaining_secs),
            ..Default::default()
        }
    }

    /// Running：恢复 phase（resume/start 用）。
    pub fn running(phase: AwdPhase) -> Self {
        Self {
            phase: Some(phase),
            ..Default::default()
        }
    }

    /// 配置变更：回到 Configuring 并清除 verified 标记。
    pub fn config_changed() -> Self {
        Self {
            clear_verified: true,
            ..Default::default()
        }
    }
}

/// 已实例化仓储（新调用点优先）。
pub struct AwdEventRepository {
    db: DatabaseConnection,
}

impl AwdEventRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn find_by_event_id(
        &self,
        event_id: Uuid,
    ) -> Result<Option<awd_events::Model>, sea_orm::DbErr> {
        find_by_event_id(&self.db, event_id).await
    }

    pub async fn find_active_events(&self) -> Result<Vec<awd_events::Model>, sea_orm::DbErr> {
        find_active_events(&self.db).await
    }

    pub async fn update_status(
        &self,
        id: Uuid,
        status: AwdEventStatus,
    ) -> Result<(), sea_orm::DbErr> {
        update_status(&self.db, id, status).await
    }

    pub async fn update_phase(&self, id: Uuid, phase: AwdPhase) -> Result<(), sea_orm::DbErr> {
        update_phase(&self.db, id, phase).await
    }

    /// 状态机唯一入口：CAS 语义 + 转移校验 + 附属字段原子写入（Phase 0）。
    pub async fn transition(
        &self,
        id: Uuid,
        expected_status: AwdEventStatus,
        target_status: AwdEventStatus,
        patch: TransitionPatch,
    ) -> AwdResult<()> {
        transition_event(&self.db, id, expected_status, target_status, patch).await
    }

    pub async fn mark_verified(&self, id: Uuid, revision: &str) -> Result<(), sea_orm::DbErr> {
        mark_verified(&self.db, id, revision).await
    }

    pub async fn clear_verified(&self, id: Uuid) -> Result<(), sea_orm::DbErr> {
        clear_verified(&self.db, id).await
    }
}

/// 旧调用点使用的向后兼容名称。
pub type EventRepo<'a> = AwdEventRepositoryRef<'a>;

/// 借用式仓储（无需时避免克隆 `DatabaseConnection`）。
pub struct AwdEventRepositoryRef<'a> {
    pub db: &'a DatabaseConnection,
}

impl<'a> AwdEventRepositoryRef<'a> {
    pub fn new(db: &'a DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn find_by_event_id(
        &self,
        event_id: Uuid,
    ) -> Result<Option<awd_events::Model>, sea_orm::DbErr> {
        find_by_event_id(self.db, event_id).await
    }
}

// ── Connection-generic helpers (usable inside transactions) ──

pub async fn find_by_event_id<C: ConnectionTrait + Send>(
    db: &C,
    event_id: Uuid,
) -> Result<Option<awd_events::Model>, sea_orm::DbErr> {
    awd_events::Entity::find()
        .filter(awd_events::Column::EventId.eq(event_id))
        .one(db)
        .await
}

pub async fn find_active_events<C: ConnectionTrait + Send>(
    db: &C,
) -> Result<Vec<awd_events::Model>, sea_orm::DbErr> {
    // 验收 #11 范围扩展：除 Running/Paused（完整恢复 gamebox+WG+round），
    // 覆盖比赛前状态（Verified/Deploying/Deployed/Prechecking）——这些阶段崩溃
    // 会留下半管理资源，重启后同样需要 gamebox/WG/firewall 收敛（reconcile 幂等）；
    // DeployFailed/NetworkError/StartBlocked 保持不动（冻结/失败态，交给管理员或
    // Start Gate 判定）。
    awd_events::Entity::find()
        .filter(awd_events::Column::Status.is_in([
            AwdEventStatus::Running,
            AwdEventStatus::Paused,
            AwdEventStatus::Verified,
            AwdEventStatus::Deploying,
            AwdEventStatus::Deployed,
            AwdEventStatus::Prechecking,
        ]))
        .all(db)
        .await
}

/// 状态机唯一入口（Phase 0 核心）。
///
/// 语义：
/// 1. **锁**：事务内 `SELECT ... FOR UPDATE` 锁定目标行（嵌套事务自动退化为 savepoint）。
/// 2. **转移合法性**：读锁内当前状态，经 `can_transition_to` 校验；非法跳变直接拒绝。
/// 3. **CAS 并发保护**：当前状态必须等于调用方 `expected_status`，否则 `Conflict`。
///    两台 API 同时 finish/pause 时，后到者拿到锁后读到新状态 → 拒绝。
/// 4. **附属字段原子写入**：`TransitionPatch` 所有字段与 status 在同一 ActiveModel UPDATE 中提交。
pub async fn transition_event<C>(
    db: &C,
    id: Uuid,
    expected_status: AwdEventStatus,
    target_status: AwdEventStatus,
    patch: TransitionPatch,
) -> AwdResult<()>
where
    C: ConnectionTrait + TransactionTrait + Send,
{
    use sea_orm::sea_query::LockType;

    let txn = db
        .begin()
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    // 1. 锁定目标行（防并发状态跳转；嵌套事务自动退化为 savepoint）
    let event = awd_events::Entity::find_by_id(id)
        .lock(LockType::Update)
        .one(&txn)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound("AWD event not found".into()))?;

    // 2. 转移合法性
    event
        .status
        .can_transition_to(target_status.clone())
        .map_err(AwdError::InvalidState)?;

    // 3. CAS：当前状态必须等于调用方预期
    if event.status != expected_status {
        let _ = txn.rollback().await;
        return Err(AwdError::Conflict(format!(
            "concurrent status transition: event {} expected {:?} but actual is {:?}",
            id, expected_status, event.status
        )));
    }

    // 4. ActiveModel 原子更新（enum 列由 Set 正确绑定 Postgres enum）
    active_model_from_patch(id, target_status, patch)
        .update(&txn)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    txn.commit()
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;
    Ok(())
}

/// 由 id/target/patch 构造 ActiveModel（未设字段 NotSet，UPDATE 不触碰）。
fn active_model_from_patch(
    id: Uuid,
    target_status: AwdEventStatus,
    patch: TransitionPatch,
) -> awd_events::ActiveModel {
    let now_ts: chrono::DateTime<chrono::FixedOffset> = chrono::Utc::now().into();
    let mut active: awd_events::ActiveModel = awd_events::ActiveModel {
        id: Set(id),
        status: Set(target_status),
        updated_at: Set(now_ts),
        ..Default::default()
    };
    if let Some(phase) = patch.phase {
        active.phase = Set(phase);
    }
    if let Some(ts) = patch.started_at {
        active.started_at = Set(Some(ts.into()));
    }
    if let Some(rev) = patch.verified_revision {
        active.verified_revision = Set(Some(rev));
    }
    if let Some(ts) = patch.verified_at {
        active.verified_at = Set(Some(ts.into()));
    }
    if let Some(ph) = patch.paused_phase {
        active.paused_phase = Set(Some(ph));
    }
    if let Some(secs) = patch.pause_remaining_secs {
        active.pause_remaining_secs = Set(Some(secs));
    }
    if let Some(ts) = patch.finished_at {
        active.finished_at = Set(Some(ts.into()));
    }
    if patch.clear_verified {
        active.verified_at = Set(None);
        active.verified_revision = Set(None);
        active.verified_generation = Set(None);
    }
    if let Some(gen_val) = patch.verified_generation {
        active.verified_generation = Set(Some(gen_val));
    }
    active
}

/// 配置代数 +1（P2-9/P2-10）：所有影响 runtime 的配置写入口调用。
/// 不改变状态；仅递增 configuration_generation（会使 verified_generation 失配 → StartBlocked）。
pub async fn touch_configuration<C: ConnectionTrait + Send>(
    db: &C,
    id: Uuid,
) -> Result<(), sea_orm::DbErr> {
    let event = awd_events::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| sea_orm::DbErr::RecordNotFound("AWD event not found".to_string()))?;
    let next = event.configuration_generation + 1;
    let active: awd_events::ActiveModel = awd_events::ActiveModel {
        id: Set(id),
        configuration_generation: Set(next),
        updated_at: Set(chrono::Utc::now().into()),
        ..Default::default()
    };
    active.update(db).await?;
    Ok(())
}

/// 守卫版 `update_status`：最终防线。
///
/// 即使上层 service 忘记走 `transition_event`，repo 层也会校验
/// `can_transition_to` 并拒绝非法跳变（`DbErr::Custom`）。
pub async fn update_status<C>(
    db: &C,
    id: Uuid,
    status: AwdEventStatus,
) -> Result<(), sea_orm::DbErr>
where
    C: ConnectionTrait + TransactionTrait + Send,
{
    let event = awd_events::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| sea_orm::DbErr::RecordNotFound("AWD event not found".to_string()))?;

    transition_event(db, id, event.status.clone(), status, Default::default())
        .await
        .map_err(|e| sea_orm::DbErr::Custom(e.to_string()))
}

/// 守卫版 `update_phase`：phase 只能按 `AwdPhaseExt::can_transition_to` 转移。
pub async fn update_phase<C>(db: &C, id: Uuid, phase: AwdPhase) -> Result<(), sea_orm::DbErr>
where
    C: ConnectionTrait + TransactionTrait + Send,
{
    use sea_orm::sea_query::LockType;

    let txn = db.begin().await?;

    let event = awd_events::Entity::find_by_id(id)
        .lock(LockType::Update)
        .one(&txn)
        .await?
        .ok_or_else(|| sea_orm::DbErr::RecordNotFound("AWD event not found".to_string()))?;

    event
        .phase
        .can_transition_to(phase.clone())
        .map_err(sea_orm::DbErr::Custom)?;

    let now_ts: chrono::DateTime<chrono::FixedOffset> = chrono::Utc::now().into();
    let active: awd_events::ActiveModel = awd_events::ActiveModel {
        id: Set(id),
        phase: Set(phase),
        updated_at: Set(now_ts),
        ..Default::default()
    };
    active.update(&txn).await?;

    txn.commit().await?;
    Ok(())
}

/// 守卫版 `mark_verified`：仅允许从 `Prechecking` 进入 `Verified`，
/// verified_revision + verified_at 与状态同事务写入。
pub async fn mark_verified<C>(db: &C, id: Uuid, revision: &str) -> Result<(), sea_orm::DbErr>
where
    C: ConnectionTrait + TransactionTrait + Send,
{
    transition_event(
        db,
        id,
        AwdEventStatus::Prechecking,
        AwdEventStatus::Verified,
        TransitionPatch::verified(revision),
    )
    .await
    .map_err(|e| sea_orm::DbErr::Custom(e.to_string()))
}

/// 守卫版 `clear_verified`：仅允许从 `Verified` / `StartBlocked` 回到 `Configuring`。
pub async fn clear_verified<C>(db: &C, id: Uuid) -> Result<(), sea_orm::DbErr>
where
    C: ConnectionTrait + TransactionTrait + Send,
{
    let event = awd_events::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| sea_orm::DbErr::RecordNotFound("AWD event not found".to_string()))?;

    if !matches!(
        event.status,
        AwdEventStatus::Verified | AwdEventStatus::StartBlocked
    ) {
        return Err(sea_orm::DbErr::Custom(format!(
            "clear_verified requires Verified/StartBlocked, current is {:?}",
            event.status
        )));
    }

    transition_event(
        db,
        id,
        event.status.clone(),
        AwdEventStatus::Configuring,
        TransitionPatch::config_changed(),
    )
    .await
    .map_err(|e| sea_orm::DbErr::Custom(e.to_string()))
}
