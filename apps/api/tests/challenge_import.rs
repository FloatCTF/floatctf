//! DB-gated: challenge 导入的覆盖语义 + safe_name 去重。
//!
//! 复现线上 400 根因：导入 UPSERT 只针对 name，但 challenges_safe_name_key
//! 也是 UNIQUE —— 批量包里某题的 safe_name 撞上已存在行（不同 name）时
//! 直接 duplicate key。修复后：同名 → 覆盖更新（保留原 safe_name）；
//! 新名但 safe_name 被占 → 追加 -2/-3 去重。
//!
//! 需要可达的 PostgreSQL（soft-skip）。

use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};
use uuid::Uuid;

use floatctf::entity::challenges;
use floatctf::modules::challenge::build::import_challenge;

fn db_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@127.0.0.1:5432/floatctf_db".into())
}

async fn connect_or_skip() -> Option<sea_orm::DatabaseConnection> {
    match sea_orm::Database::connect(&db_url()).await {
        Ok(db) => Some(db),
        Err(e) => {
            eprintln!("skip challenge_import: DB unreachable ({e})");
            None
        }
    }
}

fn meta_toml(name: &str, desc: &str) -> String {
    format!(
        r#"name = "{name}"
author = "tester@example.com"
category = "Web"
description = "{desc}"

[flag]
value = "flag{{{name}}}"
env_var = "FLAG"
"#
    )
}

async fn find_by_safe_name(
    db: &sea_orm::DatabaseConnection,
    safe_name: &str,
) -> Option<challenges::Model> {
    challenges::Entity::find()
        .filter(challenges::Column::SafeName.eq(safe_name))
        .one(db)
        .await
        .expect("query challenge by safe_name")
}

#[tokio::test]
async fn import_overwrite_same_name_and_dedup_safe_name() {
    let Some(db) = connect_or_skip().await else {
        return;
    };

    let base = format!("imp-ovw-{}", Uuid::new_v4().simple());
    // 种子行的 safe_name 带下划线，碰撞行用 '/'（generate_safe_name 会把 '/' 替换成 '_'）
    let name_a = format!("{}_x", base);
    let collision_name = format!("{}/x", base);
    let seed_id = Uuid::new_v4();
    challenges::ActiveModel {
        id: Set(seed_id),
        name: Set(name_a.clone()),
        category: Set("Web".into()),
        description: Set("old".into()),
        safe_name: Set(name_a.clone()),
        toml_str: Set(meta_toml(&name_a, "old")),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("seed existing challenge");

    // 1) 同名 → 覆盖更新，保留原 safe_name，行 id 不变
    let updated = import_challenge(&db, meta_toml(&name_a, "v2"))
        .await
        .expect("overwrite import should succeed");
    assert_eq!(updated.id, seed_id, "同名覆盖必须复用原行");
    assert_eq!(updated.description, "v2");
    assert_eq!(updated.safe_name, name_a, "覆盖导入不得改 safe_name");

    // 2) 新名但 generate_safe_name 撞上现有 safe_name → 去重为 -2
    //    ".../x" -> '/' 被替换为 '_'，与 name_a 的 safe_name（"..._x"）相同
    let inserted = import_challenge(&db, meta_toml(&collision_name, "deduped"))
        .await
        .expect("collision import should succeed");
    assert_ne!(inserted.id, seed_id, "safe_name 冲突时必须是新行");
    assert_eq!(inserted.safe_name, format!("{}-2", name_a));
    assert_eq!(inserted.name, collision_name);

    // 3) 再次导入同名 → 覆盖，不再新增行
    let again = import_challenge(&db, meta_toml(&collision_name, "v3"))
        .await
        .expect("re-import should succeed");
    assert_eq!(again.id, inserted.id);
    assert_eq!(again.description, "v3");

    // 清理
    for sn in [&name_a, &format!("{}-2", name_a)] {
        if let Some(row) = find_by_safe_name(&db, sn).await {
            challenges::Entity::delete_by_id(row.id)
                .exec(&db)
                .await
                .ok();
        }
    }
}
