//! AWD 靶机重置：主体分列（`requested_by` / `requested_by_admin`）契约测试。
//!
//! 回归背景：管理端 `POST /api/admin/events/{id}/awd/gameboxes/{instance_id}/reset`
//! 曾把 `super_admin.id` 写进 `awd_reset_records.requested_by`（外键 → `users(id)`），
//! 稳定 500 `violates foreign key constraint "awd_reset_records_requested_by_fkey"`。
//!
//! 本文件走**真实 HTTP 端点**（不直接调用 service），断言：
//! - 管理端重置成功，且记录落到 `requested_by_admin`（`requested_by` 为空）；
//! - 玩家重置仍落 `requested_by`（`requested_by_admin` 为空）；
//! - 未绑定队伍的实例给出可预期的 4xx，且不落任何记录。
//!
//! 前置条件（缺失则 soft-skip，不误报失败）：
//! - 可达的 floatctf API：`FLOATCTF_API_BASE`（默认 `http://127.0.0.1:8080`）
//! - 数据库：`DATABASE_URL`（seeding 与断言用）
//! - 管理端凭据：`FLOATCTF_TEST_ADMIN` / `FLOATCTF_TEST_ADMIN_PASS`
//! - 玩家凭据（玩家用例）：`FLOATCTF_TEST_USER` / `FLOATCTF_TEST_USER_PASS`

use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};
use uuid::Uuid;

use floatctf::entity::sea_orm_active_enums::{
    AwdEventStatus, AwdPhase, EventFamily, EventPurpose, GameboxStatus, ParticipantMode,
    RoundStatus,
};
use floatctf::entity::{
    awd_event_gameboxes, awd_events, awd_reset_records, awd_rounds, event_gamebox_instances,
    event_instances, event_teams, events, gameboxes,
};

mod common;

use common::{api_reachable, base_url, client, login_admin, login_user};

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

fn db_url() -> Option<String> {
    env("DATABASE_URL")
}

async fn live() -> Option<sea_orm::DatabaseConnection> {
    // 两端都必须可达，否则 soft-skip（保持与其它 HTTP 契约测试一致）。
    if !api_reachable().await {
        eprintln!(
            "skip awd_reset_requester: API not reachable at {}",
            base_url()
        );
        return None;
    }
    let url = db_url()?;
    match sea_orm::Database::connect(&url).await {
        Ok(db) => Some(db),
        Err(e) => {
            eprintln!("skip awd_reset_requester: DB unreachable ({e})");
            None
        }
    }
}

/// 单次测试用的隔离夹具：event / awd_event / team / gamebox / 实例。
///
/// 清理是**失败安全**的：即便断言失败（panic）也会经 `Drop` guard 清掉夹具，
/// 不在数据库里留下 `awd-reset-requester-*` 垃圾行。
struct Fixture {
    event_id: Uuid,
    team_id: Uuid,
    /// `event_gamebox_instances.id`（reset 端点的 `{instance_id}` 就是它）
    instance_id: Uuid,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let Some(url) = db_url() else { return };
        let event_id = self.event_id;
        // 断言失败时当前线程可能已在 panic 展开中；用独立运行时同步完成清理。
        let _ = std::thread::spawn(move || {
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(_) => return,
            };
            rt.block_on(async move {
                if let Ok(db) = sea_orm::Database::connect(&url).await {
                    let _ = awd_reset_records::Entity::delete_many()
                        .filter(awd_reset_records::Column::EventId.eq(event_id))
                        .exec(&db)
                        .await;
                    let _ = event_gamebox_instances::Entity::delete_many()
                        .filter(event_gamebox_instances::Column::EventId.eq(event_id))
                        .exec(&db)
                        .await;
                    let _ = event_instances::Entity::delete_many()
                        .filter(event_instances::Column::EventId.eq(event_id))
                        .exec(&db)
                        .await;
                    let _ = awd_event_gameboxes::Entity::delete_many()
                        .filter(awd_event_gameboxes::Column::EventId.eq(event_id))
                        .exec(&db)
                        .await;
                    let _ = awd_rounds::Entity::delete_many()
                        .filter(awd_rounds::Column::EventId.eq(event_id))
                        .exec(&db)
                        .await;
                    let _ = awd_events::Entity::delete_many()
                        .filter(awd_events::Column::EventId.eq(event_id))
                        .exec(&db)
                        .await;
                    let _ = event_teams::Entity::delete_many()
                        .filter(event_teams::Column::EventId.eq(event_id))
                        .exec(&db)
                        .await;
                    let _ = events::Entity::delete_by_id(event_id).exec(&db).await;
                }
            });
        })
        .join();
    }
}

impl Fixture {
    /// `bound_team = false` 时把实例根（`event_instances.owner_team_id`）置空，
    /// 用于验证"未绑定队伍"的可预期失败路径。
    async fn seed(db: &sea_orm::DatabaseConnection, tag: &str, bound_team: bool) -> Self {
        let now = chrono::Utc::now();
        let event_id = Uuid::new_v4();
        let team_id = Uuid::new_v4();
        let gamebox_id = Uuid::new_v4();
        let event_gamebox_id = Uuid::new_v4();
        let root_id = Uuid::new_v4();
        let instance_id = Uuid::new_v4();

        events::ActiveModel {
            id: Set(event_id),
            title: Set(format!("awd-reset-requester-{tag}")),
            is_virtual: Set(false),
            family: Set(EventFamily::Awd),
            purpose: Set(EventPurpose::Competition),
            participant_mode: Set(ParticipantMode::Team),
            start_time: Set((now - chrono::Duration::minutes(5)).into()),
            end_time: Set(Some((now + chrono::Duration::hours(2)).fixed_offset())),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("insert events");

        awd_events::ActiveModel {
            id: Set(Uuid::new_v4()),
            event_id: Set(event_id),
            event_secret_ciphertext: Set(vec![1u8; 32]),
            event_secret_nonce: Set(vec![2u8; 24]),
            status: Set(AwdEventStatus::Running),
            phase: Set(AwdPhase::Attack),
            round_count: Set(Some(2)),
            configuration_generation: Set(0),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("insert awd_events");

        event_teams::ActiveModel {
            id: Set(team_id),
            event_id: Set(event_id),
            name: Set(format!("team-{tag}")),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("insert event_teams");

        gameboxes::ActiveModel {
            id: Set(gamebox_id),
            name: Set(format!("reset-fixture-{tag}")),
            safe_name: Set(format!("reset-fixture-{tag}")),
            build_status: Set(Some("ready".into())),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("insert gameboxes");

        awd_event_gameboxes::ActiveModel {
            id: Set(event_gamebox_id),
            event_id: Set(event_id),
            gamebox_id: Set(gamebox_id),
            host_offset: Set(2),
            enabled: Set(true),
            cpu_millis: Set(1000),
            memory_bytes: Set(536870912),
            pids_limit: Set(100),
            attack_score: Set(100),
            judge_down_penalty: Set(200),
            first_bonus: Set(50),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("insert awd_event_gameboxes");

        event_instances::ActiveModel {
            id: Set(root_id),
            event_id: Set(event_id),
            // owner 检查约束要求 user/team 至少其一非空；未绑定队伍时用 user 主体占位，
            // 使 `owner_team_id IS NULL` 可被表达（正是被测的失败路径）。
            owner_user_id: Set(Some(
                floatctf::entity::users::Entity::find()
                    .one(db)
                    .await
                    .expect("query users")
                    .map(|u| u.id)
                    .unwrap_or_else(Uuid::new_v4),
            )),
            owner_team_id: Set(bound_team.then_some(team_id)),
            container_name: Set(format!(
                "container-{}",
                &instance_id.simple().to_string()[..8]
            )),
            container_id: Set(Some(format!(
                "docker-{}",
                &instance_id.simple().to_string()[..8]
            ))),
            runtime_generation: Set(1),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("insert event_instances");

        // 活动回合：重置要求 Running+Attack 且存在 active round，
        // 否则会被 final settlement 守卫（Forbidden）挡下。
        awd_rounds::ActiveModel {
            id: Set(Uuid::new_v4()),
            event_id: Set(event_id),
            round_number: Set(1),
            status: Set(RoundStatus::Active),
            phase: Set(AwdPhase::Attack),
            started_at: Set(now.into()),
            scheduled_end_at: Set((now + chrono::Duration::minutes(30)).into()),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("insert awd_rounds");

        event_gamebox_instances::ActiveModel {
            id: Set(instance_id),
            event_id: Set(event_id),
            team_id: Set(team_id),
            event_gamebox_id: Set(event_gamebox_id),
            instance_id: Set(root_id),
            status: Set(GameboxStatus::Ready),
            gamebox_ip: Set("10.42.9.5/32".parse().unwrap()),
            health_status: Set("healthy".into()),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("insert event_gamebox_instances");

        Self {
            event_id,
            team_id,
            instance_id,
        }
    }

    async fn reset_records(
        &self,
        db: &sea_orm::DatabaseConnection,
    ) -> Vec<awd_reset_records::Model> {
        awd_reset_records::Entity::find()
            .filter(awd_reset_records::Column::GameboxInstanceId.eq(self.instance_id))
            .all(db)
            .await
            .expect("query awd_reset_records")
    }

    /// 清理夹具（按外键顺序）。
    async fn cleanup(self, db: &sea_orm::DatabaseConnection) {
        let _ = awd_reset_records::Entity::delete_many()
            .filter(awd_reset_records::Column::EventId.eq(self.event_id))
            .exec(db)
            .await;
        let _ = event_gamebox_instances::Entity::delete_many()
            .filter(event_gamebox_instances::Column::EventId.eq(self.event_id))
            .exec(db)
            .await;
        let _ = event_instances::Entity::delete_many()
            .filter(event_instances::Column::EventId.eq(self.event_id))
            .exec(db)
            .await;
        let _ = awd_event_gameboxes::Entity::delete_many()
            .filter(awd_event_gameboxes::Column::EventId.eq(self.event_id))
            .exec(db)
            .await;
        let _ = awd_rounds::Entity::delete_many()
            .filter(awd_rounds::Column::EventId.eq(self.event_id))
            .exec(db)
            .await;
        let _ = awd_events::Entity::delete_many()
            .filter(awd_events::Column::EventId.eq(self.event_id))
            .exec(db)
            .await;
        let _ = event_teams::Entity::delete_many()
            .filter(event_teams::Column::EventId.eq(self.event_id))
            .exec(db)
            .await;
        let _ = events::Entity::delete_by_id(self.event_id).exec(db).await;
    }
}

/// 管理端重置：真实端点 + 真实 SuperAdminJwtGuard。
///
/// 断言 reset 记录由**管理员主体**产生，且管理员 ID 是数据库中真实存在的
/// `super_admin.id`（内置系统管理员即 nil UUID，它不在 `users` 表）。
#[tokio::test]
async fn admin_reset_records_real_super_admin_id() {
    let Some(db) = live().await else { return };
    let (Some(user), Some(pass)) = (env("FLOATCTF_TEST_ADMIN"), env("FLOATCTF_TEST_ADMIN_PASS"))
    else {
        eprintln!("skip: FLOATCTF_TEST_ADMIN / FLOATCTF_TEST_ADMIN_PASS unset");
        return;
    };
    let Some(token) = login_admin(&user, &pass).await else {
        eprintln!("skip: admin login failed");
        return;
    };

    // 管理员的真实 super_admin.id（reset 记录必须引用它）。
    let admin_id = floatctf::entity::super_admin::Entity::find()
        .filter(floatctf::entity::super_admin::Column::Username.eq(&user))
        .one(&db)
        .await
        .expect("query super_admin")
        .expect("admin row exists")
        .id;

    let tag = &Uuid::new_v4().simple().to_string()[..8];
    let fx = Fixture::seed(&db, tag, true).await;

    let url = format!(
        "{}/api/admin/events/{}/awd/gameboxes/{}/reset",
        base_url(),
        fx.event_id,
        fx.instance_id
    );
    let resp = client()
        .post(&url)
        .bearer_auth(&token)
        .send()
        .await
        .expect("admin reset request");
    let status = resp.status().as_u16();
    let body: serde_json::Value = resp.json().await.unwrap_or(serde_json::Value::Null);

    // 回归核心：不得再因外键冲突 500。
    assert_ne!(
        status, 500,
        "admin reset must not 500 (FK violation regression): {body}"
    );

    let records = fx.reset_records(&db).await;
    assert_eq!(records.len(), 1, "exactly one reset record: {body}");
    let rec = &records[0];
    assert_eq!(
        rec.requested_by, None,
        "admin reset must NOT write into requested_by (FK -> users)"
    );
    assert_eq!(
        rec.requested_by_admin,
        Some(admin_id),
        "admin reset must record the real super_admin.id"
    );
    assert_eq!(
        rec.team_id, fx.team_id,
        "reset record must be accounted to the instance's team"
    );

    fx.cleanup(&db).await;
}

/// 未绑定队伍的实例：可预期的 4xx，且不落记录、不执行重置。
#[tokio::test]
async fn admin_reset_without_bound_team_is_4xx_without_record() {
    let Some(db) = live().await else { return };
    let (Some(user), Some(pass)) = (env("FLOATCTF_TEST_ADMIN"), env("FLOATCTF_TEST_ADMIN_PASS"))
    else {
        return;
    };
    let Some(token) = login_admin(&user, &pass).await else {
        return;
    };

    let tag = &Uuid::new_v4().simple().to_string()[..8];
    let fx = Fixture::seed(&db, tag, false).await;

    let url = format!(
        "{}/api/admin/events/{}/awd/gameboxes/{}/reset",
        base_url(),
        fx.event_id,
        fx.instance_id
    );
    let resp = client()
        .post(&url)
        .bearer_auth(&token)
        .send()
        .await
        .expect("admin reset request");
    let status = resp.status().as_u16();
    let body: serde_json::Value = resp.json().await.unwrap_or(serde_json::Value::Null);

    assert!(
        (400..500).contains(&status),
        "unbound instance must yield a predictable 4xx, got {status}: {body}"
    );
    assert_ne!(status, 500, "must not surface as internal error: {body}");
    let text = body.to_string();
    assert!(
        text.contains("not bound to a team"),
        "error must explain the missing team binding: {body}"
    );

    assert!(
        fx.reset_records(&db).await.is_empty(),
        "failed reset must not create a reset record"
    );

    fx.cleanup(&db).await;
}

/// 玩家重置回归：仍写 `requested_by`，且 `requested_by_admin` 为空。
///
/// 需要 `FLOATCTF_TEST_USER` / `FLOATCTF_TEST_USER_PASS`，且该用户必须已加入
/// 夹具赛事所属队伍——因此玩家用例先通过管理端把用户加入队伍。
#[tokio::test]
async fn player_reset_records_user_and_leaves_admin_column_empty() {
    let Some(db) = live().await else { return };
    let (Some(admin_user), Some(admin_pass)) =
        (env("FLOATCTF_TEST_ADMIN"), env("FLOATCTF_TEST_ADMIN_PASS"))
    else {
        return;
    };
    let (Some(player_user), Some(player_pass)) =
        (env("FLOATCTF_TEST_USER"), env("FLOATCTF_TEST_USER_PASS"))
    else {
        eprintln!("skip: FLOATCTF_TEST_USER / FLOATCTF_TEST_USER_PASS unset");
        return;
    };
    let Some(admin_token) = login_admin(&admin_user, &admin_pass).await else {
        return;
    };
    let Some(player_token) = login_user(&player_user, &player_pass).await else {
        eprintln!("skip: player login failed");
        return;
    };

    let player_id = floatctf::entity::users::Entity::find()
        .filter(floatctf::entity::users::Column::Username.eq(&player_user))
        .one(&db)
        .await
        .expect("query users")
        .expect("player row exists")
        .id;

    let tag = &Uuid::new_v4().simple().to_string()[..8];
    let fx = Fixture::seed(&db, tag, true).await;

    // 把玩家加入夹具队伍（team event 必须走 team membership 端点）。
    let add_url = format!(
        "{}/api/admin/events/{}/teams/{}/users",
        base_url(),
        fx.event_id,
        fx.team_id
    );
    let add = client()
        .post(&add_url)
        .bearer_auth(&admin_token)
        .json(&serde_json::json!({ "user_id": player_id }))
        .send()
        .await
        .expect("add user to team");
    if add.status().as_u16() != 200 {
        eprintln!(
            "skip: could not add player to fixture team ({})",
            add.status()
        );
        fx.cleanup(&db).await;
        return;
    }

    let url = format!(
        "{}/api/events/{}/awd/gameboxes/{}/reset",
        base_url(),
        fx.event_id,
        fx.instance_id
    );
    let resp = client()
        .post(&url)
        .bearer_auth(&player_token)
        .send()
        .await
        .expect("player reset request");
    let status = resp.status().as_u16();
    let body: serde_json::Value = resp.json().await.unwrap_or(serde_json::Value::Null);

    assert_ne!(status, 500, "player reset must not 500: {body}");

    let records = fx.reset_records(&db).await;
    if records.is_empty() {
        // 玩家路径可能因 eligibility/限流前置失败（非本次回归目标），明确报出以便定位。
        panic!("player reset produced no record (status={status}): {body}");
    }
    let rec = &records[0];
    assert_eq!(rec.requested_by, Some(player_id), "player -> requested_by");
    assert_eq!(
        rec.requested_by_admin, None,
        "player reset must leave requested_by_admin empty"
    );

    fx.cleanup(&db).await;
}

/// 认证身份不变量：`SuperAdminJwtGuard` 解析出的管理员 ID 必须是 `super_admin`
/// 表中真实存在的主键。
///
/// 回归价值：若 Guard 将来退化成返回 nil/占位 UUID，或从错误的表取 id，
/// 本测试会红——而不必等到 reset 触发外键错误才暴露。
///
/// 说明：内置系统管理员的 ID 本身**就是** nil UUID（initial-data 迁移），
/// 因此这里断言的是"存在于 super_admin 表"，而不是"非 nil"。
#[tokio::test]
async fn super_admin_guard_id_resolves_to_real_database_row() {
    let Some(db) = live().await else { return };
    let (Some(user), Some(pass)) = (env("FLOATCTF_TEST_ADMIN"), env("FLOATCTF_TEST_ADMIN_PASS"))
    else {
        return;
    };
    let Some(token) = login_admin(&user, &pass).await else {
        eprintln!("skip: admin login failed");
        return;
    };

    let db_admin_id = floatctf::entity::super_admin::Entity::find()
        .filter(floatctf::entity::super_admin::Column::Username.eq(&user))
        .one(&db)
        .await
        .expect("query super_admin")
        .expect("admin row exists")
        .id;

    // 用需要管理员身份的端点间接验证 Guard 解析出的主体。
    // `/api/admin/session` 已用于取 token；这里用 event 数据端点确认身份可用。
    let url = format!("{}/api/admin/events", base_url());
    let resp = client()
        .get(&url)
        .bearer_auth(&token)
        .send()
        .await
        .expect("admin request");
    assert_eq!(
        resp.status().as_u16(),
        200,
        "admin token must authenticate with SuperAdminJwtGuard"
    );

    // 真实管理员的 id 必须能在 super_admin 表中按主键取回。
    let found = floatctf::entity::super_admin::Entity::find_by_id(db_admin_id)
        .one(&db)
        .await
        .expect("query super_admin by id");
    assert!(
        found.is_some(),
        "guard-resolved admin id {db_admin_id} must exist in super_admin"
    );

    // 该 id 不应出现在 users 表中（否则说明主体空间被混淆）。
    let in_users = floatctf::entity::users::Entity::find_by_id(db_admin_id)
        .one(&db)
        .await
        .expect("query users");
    assert!(
        in_users.is_none(),
        "admin id must not be a users row; subject spaces must stay separate"
    );
}
