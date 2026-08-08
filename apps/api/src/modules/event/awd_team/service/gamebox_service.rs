//! GameBox library service + 单一 runtime spec resolver（§49/§50）。
//!
//! 本服务承载：
//!   1. 全局 GameBox 库：创建 GameBox（自动 Revision 1）、编辑（→ Revision N+1，§36）、
//!      safe_name 校验/去重、spec_digest 幂等（canonical spec 未变不建新 revision）。
//!   2. `resolve_event_gamebox_spec` —— Deploy / Reset / Recovery / Precheck 唯一共享的
//!      effective config 解析入口（Revision 默认值 + EventGameBox 允许覆盖）。
//!   3. `build_gamebox_runtime_spec` —— 从 resolved spec 组装 fcmc::GameBoxSpec
//!      （image@digest、labels 绑定 logical identity、healthcheck 生效）。

use std::collections::BTreeMap;

use sea_orm::DatabaseConnection;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::entity::{
    awd_event_gameboxes, awd_events, awd_team_networks, gamebox_revisions, gameboxes,
};
use crate::modules::event::awd_team::{
    AwdError, AwdResult,
    crypto::AwdCrypto,
    domain::{slugify, validate_safe_name},
    repo::{event_gamebox_repo, gamebox_lib_repo},
};

// ---------------------------------------------------------------------------
// GameBox 库：配置与 canonical spec
// ---------------------------------------------------------------------------

/// 管理员编辑 GameBox 时提供的运行时配置（= Revision 内容）。
#[derive(Debug, Clone, Default)]
pub struct GameBoxConfig {
    pub source_toml: String,
    pub image_ref: String,
    pub image_digest: Option<String>,
    pub username: String,
    pub cpu_millis: i64,
    pub memory_bytes: i64,
    pub pids_limit: i64,
    pub healthcheck: Option<serde_json::Value>,
    pub judge_script_name: Option<String>,
    pub judge_script_content: Option<String>,
    pub judge_args_json: Option<serde_json::Value>,
    pub judge_timeout_secs: Option<i32>,
    pub judge_retry_interval_secs: Option<i32>,
}

/// canonical spec：BTreeMap 保证键序确定 → JSON 序列化确定 → digest 稳定（§7）。
pub fn canonical_spec(config: &GameBoxConfig) -> serde_json::Value {
    let mut m = BTreeMap::new();
    m.insert("image_ref".to_string(), serde_json::json!(config.image_ref));
    if let Some(d) = &config.image_digest {
        m.insert("image_digest".to_string(), serde_json::json!(d));
    }
    m.insert("username".to_string(), serde_json::json!(config.username));
    m.insert(
        "cpu_millis".to_string(),
        serde_json::json!(config.cpu_millis),
    );
    m.insert(
        "memory_bytes".to_string(),
        serde_json::json!(config.memory_bytes),
    );
    m.insert(
        "pids_limit".to_string(),
        serde_json::json!(config.pids_limit),
    );
    if let Some(h) = &config.healthcheck {
        m.insert("healthcheck".to_string(), h.clone());
    }
    if let Some(n) = &config.judge_script_name {
        m.insert("judge_script_name".to_string(), serde_json::json!(n));
    }
    if let Some(c) = &config.judge_script_content {
        m.insert("judge_script_content".to_string(), serde_json::json!(c));
    }
    if let Some(a) = &config.judge_args_json {
        m.insert("judge_args".to_string(), a.clone());
    }
    if let Some(t) = config.judge_timeout_secs {
        m.insert("judge_timeout_secs".to_string(), serde_json::json!(t));
    }
    if let Some(t) = config.judge_retry_interval_secs {
        m.insert(
            "judge_retry_interval_secs".to_string(),
            serde_json::json!(t),
        );
    }
    serde_json::Value::Object(m.into_iter().collect())
}

pub fn spec_digest(spec: &serde_json::Value) -> String {
    let mut hasher = Sha256::new();
    hasher.update(spec.to_string().as_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn validate_config(config: &GameBoxConfig) -> AwdResult<()> {
    if config.image_ref.trim().is_empty() {
        return Err(AwdError::Validation("image_ref 不能为空".into()));
    }
    if config.username.trim().is_empty() {
        return Err(AwdError::Validation("username 不能为空".into()));
    }
    if config.cpu_millis <= 0 || config.memory_bytes <= 0 || config.pids_limit <= 0 {
        return Err(AwdError::Validation("资源限制必须为正数".into()));
    }
    Ok(())
}

/// 创建 GameBox identity + Revision 1（新题入库）。
pub async fn create_gamebox_with_revision(
    db: &DatabaseConnection,
    name: String,
    safe_name: String,
    category: String,
    description: String,
    hidden: bool,
    config: GameBoxConfig,
) -> AwdResult<(gameboxes::Model, gamebox_revisions::Model)> {
    validate_safe_name(&safe_name).map_err(AwdError::Validation)?;
    validate_config(&config)?;

    if gamebox_lib_repo::find_gamebox_by_safe_name(db, &safe_name)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .is_some()
    {
        return Err(AwdError::Conflict(format!("safe_name 已存在: {safe_name}")));
    }

    let spec = canonical_spec(&config);
    let digest = spec_digest(&spec);
    let gb = gamebox_lib_repo::create_gamebox(db, name, safe_name, category, description, hidden)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    let rev = gamebox_lib_repo::create_revision(
        db,
        gb.id,
        gamebox_lib_repo::NewRevision {
            source_toml: config.source_toml,
            spec_json: spec,
            spec_digest: digest,
            image_ref: config.image_ref,
            image_digest: config.image_digest,
            username: config.username,
            default_cpu_millis: config.cpu_millis,
            default_memory_bytes: config.memory_bytes,
            default_pids_limit: config.pids_limit,
            healthcheck_json: config.healthcheck,
            judge_script_name: config.judge_script_name,
            judge_script_content: config.judge_script_content,
            judge_args_json: config.judge_args_json,
            default_judge_timeout_secs: config.judge_timeout_secs,
            default_judge_retry_interval_secs: config.judge_retry_interval_secs,
        },
    )
    .await
    .map_err(|e| AwdError::Database(e.to_string()))?
    .ok_or_else(|| AwdError::Internal("revision 1 creation unexpectedly skipped".into()))?;

    Ok((gb, rev))
}

/// 编辑 GameBox：canonical spec 变化才创建 Revision N+1（§36）；已 pin 的赛事不受影响。
/// 返回 None 表示 spec 未变化（未创建新 revision）。
pub async fn edit_gamebox_revision(
    db: &DatabaseConnection,
    gamebox_id: Uuid,
    config: GameBoxConfig,
) -> AwdResult<Option<gamebox_revisions::Model>> {
    validate_config(&config)?;
    if gamebox_lib_repo::find_gamebox_by_id(db, gamebox_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .is_none()
    {
        return Err(AwdError::NotFound("GameBox not found".into()));
    }

    let spec = canonical_spec(&config);
    let digest = spec_digest(&spec);
    gamebox_lib_repo::create_revision(
        db,
        gamebox_id,
        gamebox_lib_repo::NewRevision {
            source_toml: config.source_toml,
            spec_json: spec,
            spec_digest: digest,
            image_ref: config.image_ref,
            image_digest: config.image_digest,
            username: config.username,
            default_cpu_millis: config.cpu_millis,
            default_memory_bytes: config.memory_bytes,
            default_pids_limit: config.pids_limit,
            healthcheck_json: config.healthcheck,
            judge_script_name: config.judge_script_name,
            judge_script_content: config.judge_script_content,
            judge_args_json: config.judge_args_json,
            default_judge_timeout_secs: config.judge_timeout_secs,
            default_judge_retry_interval_secs: config.judge_retry_interval_secs,
        },
    )
    .await
    .map_err(|e| AwdError::Database(e.to_string()))
}

/// safe_name 生成 + 去重（展示名 → 唯一 safe_name）。
pub async fn unique_safe_name(db: &DatabaseConnection, display_name: &str) -> AwdResult<String> {
    let base = slugify(display_name);
    let base = if base.is_empty() {
        "gamebox".to_string()
    } else {
        base
    };
    let mut candidate = base.clone();
    let mut i = 1;
    while gamebox_lib_repo::find_gamebox_by_safe_name(db, &candidate)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .is_some()
    {
        candidate = format!("{base}-{i}");
        i += 1;
        if i > 1000 {
            return Err(AwdError::Internal("safe_name 去重溢出".into()));
        }
    }
    Ok(candidate)
}

// ---------------------------------------------------------------------------
// 单一 resolver（§49/§50）
// ---------------------------------------------------------------------------

/// EventGameBox + pin Revision + GameBox identity 的 effective runtime spec。
/// 所有 Deploy / Reset / Recovery / Precheck 必须经它解析，禁止各自拼配置。
#[derive(Debug, Clone)]
pub struct ResolvedGameBoxRuntimeSpec {
    pub event_gamebox: awd_event_gameboxes::Model,
    pub revision: gamebox_revisions::Model,
    pub gamebox: gameboxes::Model,
    pub effective_cpu_millis: i64,
    pub effective_memory_bytes: i64,
    pub effective_pids_limit: i64,
    pub effective_healthcheck_json: Option<serde_json::Value>,
    pub effective_judge_timeout_secs: Option<i32>,
    pub effective_judge_retry_interval_secs: Option<i32>,
}

/// image@digest（digest 已 pin 时），否则退回 image_ref（§8）。
pub fn pinned_image_ref(image_ref: &str, image_digest: &Option<String>) -> String {
    match image_digest {
        Some(d) if !d.is_empty() => format!("{image_ref}@{d}"),
        _ => image_ref.to_string(),
    }
}

impl ResolvedGameBoxRuntimeSpec {
    pub fn effective_image_ref(&self) -> String {
        pinned_image_ref(&self.revision.image_ref, &self.revision.image_digest)
    }

    pub fn effective_healthcheck(&self) -> Result<Option<fcmc::HealthcheckConfig>, AwdError> {
        match &self.effective_healthcheck_json {
            None => Ok(None),
            Some(v) => serde_json::from_value(v.clone())
                .map(Some)
                .map_err(|e| AwdError::Validation(format!("healthcheck JSON 非法: {e}"))),
        }
    }
}

/// 从 EventGameBox 解析 effective runtime spec（Revision 默认值 + 赛事覆盖）。
pub async fn resolve_event_gamebox_spec(
    db: &DatabaseConnection,
    event_gamebox_id: Uuid,
) -> AwdResult<ResolvedGameBoxRuntimeSpec> {
    let eg = event_gamebox_repo::find_event_gamebox_by_id(db, event_gamebox_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound("EventGameBox not found".into()))?;
    let revision = event_gamebox_repo::find_pinned_revision(db, &eg)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound("pinned GameBoxRevision not found".into()))?;
    let gamebox = event_gamebox_repo::find_gamebox_identity(db, eg.gamebox_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound("GameBox identity not found".into()))?;

    Ok(ResolvedGameBoxRuntimeSpec {
        effective_cpu_millis: eg.cpu_millis,
        effective_memory_bytes: eg.memory_bytes,
        effective_pids_limit: eg.pids_limit,
        effective_healthcheck_json: eg
            .healthcheck_override_json
            .clone()
            .or_else(|| revision.healthcheck_json.clone()),
        effective_judge_timeout_secs: eg
            .judge_timeout_secs
            .or(revision.default_judge_timeout_secs),
        effective_judge_retry_interval_secs: eg
            .judge_retry_interval_secs
            .or(revision.default_judge_retry_interval_secs),
        event_gamebox: eg,
        revision,
        gamebox,
    })
}

// ---------------------------------------------------------------------------
// 共享 fcmc spec 构建（Deploy / Reset 同一路径，§50）
// ---------------------------------------------------------------------------

/// 从 resolved spec 组装 fcmc::GameBoxSpec。纯函数（DB/加密已由调用方完成）。
/// Deploy（新建 instance）与 Reset（复用 instance）共用此路径（§50）。
#[allow(clippy::too_many_arguments)]
pub fn build_gamebox_runtime_spec(
    resolved: &ResolvedGameBoxRuntimeSpec,
    awd_event: &awd_events::Model,
    instance_id: Uuid,
    event_gamebox_id: Uuid,
    team_id: Uuid,
    container_name: &str,
    gamebox_ip: &str,
    network_name: &str,
    password: String,
    runtime_generation: i64,
) -> AwdResult<fcmc::GameBoxSpec> {
    let healthcheck = resolved.effective_healthcheck()?;
    Ok(fcmc::GameBoxSpec {
        event_id: awd_event.event_id,
        team_id,
        event_gamebox_id,
        instance_id,
        runtime_generation,
        container_name: container_name.to_string(),
        image_ref: resolved.effective_image_ref(),
        network_name: network_name.to_string(),
        fixed_ip: gamebox_ip.to_string(),
        username: resolved.revision.username.clone(),
        password,
        cpu_millis: resolved.effective_cpu_millis,
        memory_bytes: resolved.effective_memory_bytes,
        pids_limit: resolved.effective_pids_limit,
        healthcheck,
        extra_hosts: vec![
            format!("flagserver:{}", awd_event.flagserver_ip),
            format!("judgeserver:{}", awd_event.judgeserver_ip),
        ],
        labels: std::collections::HashMap::new(), // fcmc 内部按逻辑身份重打标签
    })
}

/// 解密队伍 SSH 密码（team-level 凭据，§22.1：产品契约为一队一个密码）。
pub async fn decrypt_team_ssh_password(
    crypto: &AwdCrypto,
    event_id: Uuid,
    team_net: &awd_team_networks::Model,
) -> AwdResult<String> {
    use crate::modules::event::awd_team::crypto::EncryptedBlob;
    let blob = EncryptedBlob {
        ciphertext: team_net.ssh_password_ciphertext.clone(),
        nonce: team_net.ssh_password_nonce.clone(),
        key_version: team_net.key_version,
    };
    let aad = AwdCrypto::build_aad(event_id, "ssh_password");
    let bytes = crypto
        .decrypt(&blob, &aad)
        .map_err(|e| AwdError::Crypto(e.to_string()))?;
    String::from_utf8(bytes).map_err(|e| AwdError::Crypto(e.to_string()))
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn base_config() -> GameBoxConfig {
        GameBoxConfig {
            source_toml: String::new(),
            image_ref: "registry/easy-web:v1".into(),
            image_digest: None,
            username: "ctf".into(),
            cpu_millis: 1000,
            memory_bytes: 512 * 1024 * 1024,
            pids_limit: 100,
            healthcheck: None,
            judge_script_name: None,
            judge_script_content: None,
            judge_args_json: None,
            judge_timeout_secs: None,
            judge_retry_interval_secs: None,
        }
    }

    #[test]
    fn canonical_spec_is_deterministic() {
        let c1 = canonical_spec(&base_config());
        let c2 = canonical_spec(&base_config());
        assert_eq!(c1, c2);
        assert_eq!(spec_digest(&c1), spec_digest(&c2));
    }

    #[test]
    fn spec_change_changes_digest() {
        let a = base_config();
        let mut b = base_config();
        b.cpu_millis = 2000;
        let da = spec_digest(&canonical_spec(&a));
        let db = spec_digest(&canonical_spec(&b));
        assert_ne!(da, db, "资源变化必须产生新 digest（新 revision）");
    }

    #[test]
    fn field_order_does_not_change_digest() {
        // §7：字段顺序/空格变化不产生无意义 revision —— canonical 序列化保证
        let a = base_config();
        let da = spec_digest(&canonical_spec(&a));
        let same = base_config();
        let db = spec_digest(&canonical_spec(&same));
        assert_eq!(da, db);
    }

    #[test]
    fn effective_image_ref_pins_digest() {
        assert_eq!(
            pinned_image_ref("registry/easy-web:v1", &Some("sha256:abc123".into())),
            "registry/easy-web:v1@sha256:abc123"
        );
        // digest 为空 → 退回 tag（§8.1 legacy 允许 NULL，生产前必须 pin）
        assert_eq!(
            pinned_image_ref("registry/easy-web:v1", &None),
            "registry/easy-web:v1"
        );
        assert_eq!(
            pinned_image_ref("registry/easy-web:v1", &Some(String::new())),
            "registry/easy-web:v1"
        );
    }

    #[test]
    fn validate_config_rejects_empty() {
        let mut c = base_config();
        c.image_ref = "".into();
        assert!(validate_config(&c).is_err());
        let mut c2 = base_config();
        c2.memory_bytes = 0;
        assert!(validate_config(&c2).is_err());
    }
}
