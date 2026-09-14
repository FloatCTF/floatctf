//! DB-gated: 单版本 Challenge identity 导入语义。
//!
//! 覆盖：版本门禁（严格递增）、identity 上的 package 字段 upsert、
//! effective_image_ref pin 顺序（RepoDigest > image_id）。不依赖 Docker。

use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};
use uuid::Uuid;

use floatctf::entity::challenges;
use floatctf::infrastructure::package::version_gate_reason;
use floatctf::modules::challenge::build::import_service::{
    BUILD_STATUS_READY, ImportChallengeResult,
};

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

/// 直接对 identity 做"导入"语义的字段更新（模拟 import 的 upsert 阶段，不构建镜像）。
async fn seed_challenge_package(
    db: &sea_orm::DatabaseConnection,
    challenge: &challenges::Model,
    version: &str,
    image_repo_digest: Option<&str>,
) -> challenges::Model {
    let mut am: challenges::ActiveModel = challenge.clone().into();
    am.version = Set(Some(version.to_string()));
    am.source_toml = Set(Some(format!("name = \"t\"\nversion = \"{version}\"\n")));
    am.spec_json = Set(Some(serde_json::json!({"name": "t", "version": version})));
    am.spec_digest = Set(Some("spec-digest".into()));
    am.package_digest = Set(Some(format!("pkg-{version}")));
    am.flag_type = Set(Some("dynamic".into()));
    am.static_flag_value = Set(None);
    am.container_port = Set(Some(80));
    am.recommended_cpu_millis = Set(500);
    am.recommended_memory_bytes = Set(268_435_456);
    am.recommended_pids_limit = Set(100);
    am.image_ref = Set(Some(format!(
        "floatctf/challenges/{}:{version}",
        challenge.safe_name
    )));
    am.image_id = Set(Some(format!("sha256:local-{version}")));
    am.image_repo_digest = Set(image_repo_digest.map(str::to_string));
    am.build_status = Set(Some(BUILD_STATUS_READY.to_string()));
    am.build_error = Set(None);
    am.update(db).await.expect("upsert package fields")
}

#[tokio::test]
async fn single_version_identity_upsert_and_pin_order() {
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

    // 1) 新 identity：v1.0.0 允许导入
    assert_eq!(version_gate_reason("1.0.0", None), None);

    // 2) LocalOnly：pin 回退到 image_id
    let c1 = seed_challenge_package(&db, &challenge, "1.0.0", None).await;
    assert_eq!(c1.build_status.as_deref(), Some("ready"));
    let pin = floatctf::modules::challenge::build::effective_image_ref(
        c1.image_repo_digest.as_deref(),
        c1.image_id.as_deref(),
    )
    .expect("pin");
    assert_eq!(pin, "sha256:local-1.0.0");

    // 3) RepoDigest > image_id
    let c2 = seed_challenge_package(
        &db,
        &challenge,
        "1.1.0",
        Some(&format!(
            "registry.example/challenges/{}:1.1.0@sha256:repo",
            challenge.safe_name
        )),
    )
    .await;
    let pin2 = floatctf::modules::challenge::build::effective_image_ref(
        c2.image_repo_digest.as_deref(),
        c2.image_id.as_deref(),
    )
    .expect("pin");
    assert!(pin2.contains("@sha256:repo"));

    // 4) 版本门禁：等于/小于拒绝，严格递增放行
    assert!(version_gate_reason("1.1.0", Some("1.1.0")).is_some());
    assert!(version_gate_reason("1.0.0", Some("1.1.0")).is_some());
    assert_eq!(version_gate_reason("2.0.0", Some("1.1.0")), None);

    // 5) 清理
    challenges::Entity::delete_by_id(challenge.id)
        .exec(&db)
        .await
        .expect("cleanup challenge");
}

#[allow(dead_code)]
fn _assert_result_type(_r: ImportChallengeResult) {
    // 编译期锚点：import 结果只含 identity（无 revision）。
}
