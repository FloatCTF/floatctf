//! DB-gated: challenge_revisions 不可变版本模型。
//!
//! 覆盖：insert building → mark ready/failed、version 唯一、latest ready、
//! effective_image_ref pin 顺序（RepoDigest > image_id）。不依赖 Docker。

use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};
use uuid::Uuid;

use floatctf::entity::challenges;
use floatctf::modules::challenge::build::revision_repo;

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

#[derive(Clone)]
struct RunCtx {
    safe_name: String,
    challenge_id: Uuid,
}

fn new_rev_for(
    run: &RunCtx,
    revision_number: i32,
    version: &str,
    package_digest: &str,
) -> revision_repo::NewRevision {
    revision_repo::NewRevision {
        challenge_id: run.challenge_id,
        version: version.to_string(),
        revision_number,
        source_toml: "name = \"t\"\nversion = \"1.0.0\"\n".into(),
        spec_json: serde_json::json!({"name": "t", "version": version}),
        spec_digest: "spec-digest".into(),
        package_digest: package_digest.to_string(),
        flag_type: "dynamic".into(),
        static_flag_value: None,
        container_port: Some(80),
        recommended_cpu_millis: 500,
        recommended_memory_bytes: 268_435_456,
        recommended_pids_limit: 100,
        attachment_path: None,
        attachment_name: None,
        attachment_size: None,
        attachment_sha256: None,
        image_ref: Some(format!("floatctf/challenges/{}:{version}", run.safe_name)),
    }
}

#[tokio::test]
async fn revision_lifecycle_version_unique_and_pin_order() {
    let Some(db) = connect_or_skip().await else {
        return;
    };

    let base = format!("rev-{}", Uuid::new_v4().simple());
    let challenge = challenges::ActiveModel {
        name: Set(base.clone()),
        safe_name: Set(base.clone()),
        category: Set("Web".into()),
        description: Set("d".into()),
        hidden: Set(false),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("seed challenge");
    let run = RunCtx {
        safe_name: base,
        challenge_id: challenge.id,
    };

    // 1) building → ready（LocalOnly：仅 image_id）
    let rev = revision_repo::insert_building(&db, new_rev_for(&run, 1, "1.0.0", "abc"))
        .await
        .expect("insert building");
    assert_eq!(rev.build_status, "building");
    let ready = revision_repo::mark_ready(
        &db,
        rev.id,
        format!("floatctf/challenges/{}:1.0.0", run.safe_name),
        "sha256:local".into(),
        None,
    )
    .await
    .expect("mark ready");
    assert_eq!(ready.build_status, "ready");
    // LocalOnly：pin 回退到 image_id
    assert_eq!(
        revision_repo::effective_image_ref(&ready).unwrap(),
        "sha256:local"
    );

    // 2) 同 version 同 package_digest → 幂等返回同一行
    let again = revision_repo::find_by_challenge_and_version(&db, challenge.id, "1.0.0")
        .await
        .expect("find by version")
        .expect("exists");
    assert_eq!(again.id, rev.id);
    assert_eq!(again.package_digest, "abc");

    // 3) latest ready（同一 challenge 有两个版本时取 ready 的）
    let rev2 = revision_repo::insert_building(&db, new_rev_for(&run, 2, "1.1.0", "def"))
        .await
        .expect("insert v1.1.0");
    assert_eq!(rev2.build_status, "building");
    let latest = revision_repo::find_latest_ready(&db, challenge.id)
        .await
        .expect("latest ready")
        .expect("v1.0.0 is ready");
    assert_eq!(latest.version, "1.0.0");

    // 4) RepoDigest > image_id
    let ready2 = revision_repo::mark_ready(
        &db,
        rev2.id,
        format!("floatctf/challenges/{}:1.1.0", run.safe_name),
        "sha256:local2".into(),
        Some(format!(
            "registry.example/challenges/{}:1.1.0@sha256:repo",
            run.safe_name
        )),
    )
    .await
    .expect("mark ready v1.1.0");
    assert_eq!(
        revision_repo::effective_image_ref(&ready2).unwrap(),
        format!(
            "registry.example/challenges/{}:1.1.0@sha256:repo",
            run.safe_name
        )
    );

    // 5) failed revision 保留诊断
    let rev3 = revision_repo::insert_building(&db, new_rev_for(&run, 3, "2.0.0", "ghi"))
        .await
        .expect("insert v2.0.0");
    let failed = revision_repo::mark_failed(&db, rev3.id, "BUILD_FAILED: boom".into())
        .await
        .expect("mark failed");
    assert_eq!(failed.build_status, "failed");
    assert_eq!(failed.build_error.as_deref(), Some("BUILD_FAILED: boom"));

    // 6) 清理
    challenges::Entity::delete_by_id(challenge.id)
        .exec(&db)
        .await
        .expect("cleanup challenge");
}
