//! GameBox library service + 单一 runtime spec resolver。
//!
//! 本服务承载：
//!   1. 全局 GameBox 身份：identity 元数据更新、safe_name 校验。
//!   2. `resolve_event_gamebox_spec` —— Deploy / Reset / Recovery / Precheck / Judge
//!      唯一共享的 effective config 解析入口（pinned Revision + EventGameBox 覆盖）。
//!   3. `build_gamebox_runtime_spec` —— 从 resolved spec 组装 fcmc::GameBoxSpec。
//!
//! 镜像 pin 规则：
//!   prefer `revision.image_repo_digest` if Some, else `image_id` (LocalOnly),
//!   else fall back to `image_ref` (tag). Ready revisions must have at least one pin.

use sea_orm::DatabaseConnection;
use uuid::Uuid;

use crate::entity::{
    awd_event_gameboxes, awd_event_networks, awd_events, awd_team_networks, gamebox_revisions,
    gameboxes,
};
use crate::modules::event::awd_team::{
    AwdError, AwdResult,
    domain::{slugify, validate_safe_name},
    repo::{
        event_gamebox_repo, gamebox_lib_repo,
        gamebox_revision_repo::{self, BUILD_STATUS_READY},
    },
};

// ---------------------------------------------------------------------------
// GameBox 身份
// ---------------------------------------------------------------------------

/// 更新 GameBox 身份元数据（name/category/description/hidden）。不含镜像/配置。
pub async fn update_gamebox_identity(
    db: &DatabaseConnection,
    gamebox_id: Uuid,
    name: Option<String>,
    category: Option<String>,
    description: Option<String>,
    hidden: Option<bool>,
) -> AwdResult<gameboxes::Model> {
    gamebox_lib_repo::update_gamebox_identity(
        db,
        gamebox_id,
        gamebox_lib_repo::GameBoxIdentityPatch {
            name,
            category,
            description,
            hidden,
        },
    )
    .await
    .map_err(|e| AwdError::Database(e.to_string()))?
    .ok_or_else(|| AwdError::NotFound("GameBox not found".into()))
}

/// safe_name 生成 + 去重（仅用于 admin 手动创建身份场景；import 不走 -2 后缀）。
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

/// Validate an explicit safe_name (no auto-suffix).
pub fn validate_identity_safe_name(safe_name: &str) -> AwdResult<()> {
    validate_safe_name(safe_name).map_err(AwdError::Validation)
}

// ---------------------------------------------------------------------------
// 单一 resolver
// ---------------------------------------------------------------------------

/// EventGameBox + GameBox identity + pinned Revision 的 effective runtime spec。
/// 所有 Deploy / Reset / Recovery / Precheck / Judge 必须经它解析。
#[derive(Debug, Clone)]
pub struct ResolvedGameBoxRuntimeSpec {
    pub event_gamebox: awd_event_gameboxes::Model,
    pub gamebox: gameboxes::Model,
    pub revision: gamebox_revisions::Model,
    /// SSH 用户名（来自 pinned revision）。
    pub username: String,
    pub effective_cpu_millis: i64,
    pub effective_memory_bytes: i64,
    pub effective_pids_limit: i64,
    /// Application readiness probes (HTTP/TCP list). NOT Docker HealthcheckSpec.
    pub effective_healthchecks_json: serde_json::Value,
    pub effective_judge_timeout_secs: Option<i32>,
    pub effective_judge_retry_interval_secs: Option<i32>,
}

/// Runtime image pin:
/// `image_repo_digest` (full `repo@sha256:…`) > `image_id` (LocalOnly `sha256:…`) > `image_ref` tag.
pub fn effective_image_ref_from_revision(revision: &gamebox_revisions::Model) -> AwdResult<String> {
    if let Some(ref d) = revision.image_repo_digest {
        if !d.is_empty() {
            return Ok(d.clone());
        }
    }
    if let Some(ref id) = revision.image_id {
        if !id.is_empty() {
            return Ok(id.clone());
        }
    }
    if let Some(ref r) = revision.image_ref {
        if !r.is_empty() {
            // Tag-only is a last resort; ready revisions should have id or digest.
            if revision.build_status == BUILD_STATUS_READY {
                return Err(AwdError::Validation(format!(
                    "ready revision {} has no image pin (image_repo_digest/image_id)",
                    revision.id
                )));
            }
            return Ok(r.clone());
        }
    }
    Err(AwdError::Validation(format!(
        "revision {} has no usable image reference",
        revision.id
    )))
}

impl ResolvedGameBoxRuntimeSpec {
    pub fn effective_image_ref(&self) -> AwdResult<String> {
        effective_image_ref_from_revision(&self.revision)
    }

    pub fn judge_script_content(&self) -> Option<&str> {
        self.revision.judge_script_content.as_deref()
    }

    pub fn judge_args_json(&self) -> Option<&serde_json::Value> {
        self.revision.judge_args_json.as_ref()
    }
}

/// 从 EventGameBox 解析 effective runtime spec（pinned Revision + 赛事覆盖）。
pub async fn resolve_event_gamebox_spec(
    db: &DatabaseConnection,
    event_gamebox_id: Uuid,
) -> AwdResult<ResolvedGameBoxRuntimeSpec> {
    let eg = event_gamebox_repo::find_event_gamebox_by_id(db, event_gamebox_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound("EventGameBox not found".into()))?;

    let gamebox = event_gamebox_repo::find_gamebox_identity(db, eg.gamebox_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound("GameBox identity not found".into()))?;

    let revision = gamebox_revision_repo::find_by_id(db, eg.gamebox_revision_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound("GameBox revision not found".into()))?;

    if revision.build_status != BUILD_STATUS_READY {
        return Err(AwdError::Validation(format!(
            "GameBox revision {} is not ready (status={})",
            revision.id, revision.build_status
        )));
    }

    // Ensure at least one image pin exists.
    let _ = effective_image_ref_from_revision(&revision)?;

    let effective_healthchecks_json = eg
        .healthcheck_override_json
        .clone()
        .unwrap_or_else(|| revision.healthchecks_json.clone());

    Ok(ResolvedGameBoxRuntimeSpec {
        username: revision.username.clone(),
        effective_cpu_millis: eg.cpu_millis,
        effective_memory_bytes: eg.memory_bytes,
        effective_pids_limit: eg.pids_limit,
        effective_healthchecks_json,
        effective_judge_timeout_secs: eg.judge_timeout_secs.or(revision.judge_timeout_secs),
        effective_judge_retry_interval_secs: eg
            .judge_retry_interval_secs
            .or(revision.judge_retry_interval_secs),
        event_gamebox: eg,
        gamebox,
        revision,
    })
}

// ---------------------------------------------------------------------------
// 共享 fcmc spec 构建（Deploy / Reset 同一路径）
// ---------------------------------------------------------------------------

/// 从 resolved spec 组装 fcmc::GameBoxSpec。
/// Docker-level healthcheck 不从新 manifest 写入（始终 None）；
/// Application healthchecks 由 readiness probe 服务单独使用。
#[allow(clippy::too_many_arguments)]
pub fn build_gamebox_runtime_spec(
    resolved: &ResolvedGameBoxRuntimeSpec,
    awd_event: &awd_events::Model,
    event_network: &awd_event_networks::Model,
    instance_id: Uuid,
    event_gamebox_id: Uuid,
    team_id: Uuid,
    container_name: &str,
    gamebox_ip: &str,
    network_name: &str,
    password: String,
    runtime_generation: i64,
) -> AwdResult<fcmc::GameBoxSpec> {
    Ok(fcmc::GameBoxSpec {
        event_id: awd_event.event_id,
        team_id,
        event_gamebox_id,
        instance_id,
        runtime_generation,
        container_name: container_name.to_string(),
        image_ref: resolved.effective_image_ref()?,
        network_name: network_name.to_string(),
        fixed_ip: gamebox_ip.to_string(),
        username: resolved.username.clone(),
        password,
        cpu_millis: resolved.effective_cpu_millis,
        memory_bytes: resolved.effective_memory_bytes,
        pids_limit: resolved.effective_pids_limit,
        // Docker HC not from new package manifest (HTTP/TCP app probes are separate).
        healthcheck: None,
        extra_hosts: vec![
            format!("flagserver:{}", event_network.flagserver_ip.ip()),
            format!("judgeserver:{}", event_network.judgeserver_ip.ip()),
        ],
        labels: std::collections::HashMap::new(),
    })
}

/// 解密队伍 SSH 密码（team-level 凭据：一队一个密码）。
pub async fn decrypt_team_ssh_password(
    crypto: &crate::modules::event::awd_team::crypto::AwdCrypto,
    event_id: Uuid,
    team_net: &awd_team_networks::Model,
) -> AwdResult<String> {
    use crate::modules::event::awd_team::crypto::EncryptedBlob;
    let blob = EncryptedBlob {
        ciphertext: team_net.ssh_password_ciphertext.clone(),
        nonce: team_net.ssh_password_nonce.clone(),
        key_version: team_net.key_version,
    };
    let aad =
        crate::modules::event::awd_team::crypto::AwdCrypto::build_aad(event_id, "ssh_password");
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
    use sea_orm::prelude::DateTimeWithTimeZone;

    fn dummy_revision(
        repo_digest: Option<&str>,
        image_id: Option<&str>,
        image_ref: Option<&str>,
        status: &str,
    ) -> gamebox_revisions::Model {
        gamebox_revisions::Model {
            id: Uuid::nil(),
            gamebox_id: Uuid::nil(),
            version: "1.0.0".into(),
            revision_number: 1,
            source_toml: String::new(),
            spec_json: serde_json::json!({}),
            spec_digest: "a".into(),
            package_digest: "b".into(),
            image_ref: image_ref.map(str::to_string),
            image_id: image_id.map(str::to_string),
            image_repo_digest: repo_digest.map(str::to_string),
            username: "ctf".into(),
            recommended_cpu_millis: 1000,
            recommended_memory_bytes: 512 * 1024 * 1024,
            recommended_pids_limit: 100,
            healthchecks_json: serde_json::json!([]),
            judge_script_name: None,
            judge_script_content: None,
            judge_args_json: None,
            judge_timeout_secs: None,
            judge_retry_interval_secs: None,
            build_status: status.into(),
            build_error: None,
            created_at: DateTimeWithTimeZone::from(chrono::Utc::now()),
        }
    }

    #[test]
    fn pinned_image_prefers_repo_digest() {
        let r = dummy_revision(
            Some("floatctf/gameboxes/ttt1@sha256:abc"),
            Some("sha256:local"),
            Some("floatctf/gameboxes/ttt1:1.0.0"),
            BUILD_STATUS_READY,
        );
        assert_eq!(
            effective_image_ref_from_revision(&r).unwrap(),
            "floatctf/gameboxes/ttt1@sha256:abc"
        );
    }

    #[test]
    fn pinned_image_falls_back_to_image_id_local_only() {
        let r = dummy_revision(
            None,
            Some("sha256:localid"),
            Some("floatctf/gameboxes/ttt1:1.0.0"),
            BUILD_STATUS_READY,
        );
        assert_eq!(
            effective_image_ref_from_revision(&r).unwrap(),
            "sha256:localid"
        );
    }

    #[test]
    fn ready_without_pin_errors() {
        let r = dummy_revision(
            None,
            None,
            Some("floatctf/gameboxes/ttt1:1.0.0"),
            BUILD_STATUS_READY,
        );
        assert!(effective_image_ref_from_revision(&r).is_err());
    }
}
