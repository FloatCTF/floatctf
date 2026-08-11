//! AWD GameBox 实例生命周期服务。
//!
//! 关键路径：
//!   1. 管理端挂载 EventGameBox
//!   2. `resolve_event_gamebox_spec` —— Deploy / Reset / Recovery / Precheck / Judge
//!      统一解析运行规格
//!
//! 镜像钉扎优先级：
//!   优先 `gamebox.image_repo_digest`（若有），否则 `image_id`（仅本地），
//!   再否则回退 `image_ref`（tag）。就绪的 gamebox 至少要有一种钉扎。
//!
//! 共享部分（identity 更新 / safe_name / 镜像钉扎 / package / import / healthcheck）
//! 已迁至 `modules::gamebox`，本文件仅保留 AWD 赛事语义。

use sea_orm::DatabaseConnection;
use uuid::Uuid;

use crate::entity::{
    awd_event_gameboxes, awd_event_networks, awd_events, awd_team_networks, gameboxes,
};
use crate::modules::event::awd::{AwdError, AwdResult, repo::event_gamebox_repo};
use crate::modules::gamebox::{BUILD_STATUS_READY, effective_image_ref_from_gamebox};

// ---------------------------------------------------------------------------
// 单一 resolver
// ---------------------------------------------------------------------------

/// EventGameBox + GameBox identity 的 effective runtime spec（单版本）。
/// 所有 Deploy / Reset / Recovery / Precheck / Judge 必须经它解析。
#[derive(Debug, Clone)]
pub struct ResolvedGameBoxRuntimeSpec {
    pub event_gamebox: awd_event_gameboxes::Model,
    pub gamebox: gameboxes::Model,
    /// SSH 用户名（来自 GameBox 当前版本）。
    pub username: String,
    pub effective_cpu_millis: i64,
    pub effective_memory_bytes: i64,
    pub effective_pids_limit: i64,
    /// Application readiness probes (HTTP/TCP list). NOT Docker HealthcheckSpec.
    pub effective_healthchecks_json: serde_json::Value,
    pub effective_judge_timeout_secs: Option<i32>,
    pub effective_judge_retry_interval_secs: Option<i32>,
}

/// 运行时镜像钉扎：
/// `image_repo_digest`（完整 `repo@sha256:…`）> `image_id`（仅本地 `sha256:…`）> `image_ref` tag。
/// 实现已迁至 `modules::gamebox::library`。
impl ResolvedGameBoxRuntimeSpec {
    pub fn effective_image_ref(&self) -> AwdResult<String> {
        effective_image_ref_from_gamebox(&self.gamebox).map_err(AwdError::from)
    }

    pub fn judge_script_content(&self) -> Option<&str> {
        self.gamebox.judge_script_content.as_deref()
    }

    pub fn judge_args_json(&self) -> Option<&serde_json::Value> {
        self.gamebox.judge_args_json.as_ref()
    }
}

/// 从 EventGameBox 解析 effective runtime spec（GameBox 当前版本 + 赛事覆盖）。
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

    if gamebox.build_status.as_deref() != Some(BUILD_STATUS_READY) {
        return Err(AwdError::Validation(format!(
            "GameBox {} is not ready (status={:?})",
            gamebox.id, gamebox.build_status
        )));
    }

    // Ensure at least one image pin exists.
    let _ = effective_image_ref_from_gamebox(&gamebox)?;

    let effective_healthchecks_json = eg.healthcheck_override_json.clone().unwrap_or_else(|| {
        gamebox
            .healthchecks_json
            .clone()
            .unwrap_or(serde_json::json!([]))
    });

    Ok(ResolvedGameBoxRuntimeSpec {
        username: gamebox.username.clone().unwrap_or_default(),
        effective_cpu_millis: eg.cpu_millis,
        effective_memory_bytes: eg.memory_bytes,
        effective_pids_limit: eg.pids_limit,
        effective_healthchecks_json,
        effective_judge_timeout_secs: eg.judge_timeout_secs.or(gamebox.judge_timeout_secs),
        effective_judge_retry_interval_secs: eg
            .judge_retry_interval_secs
            .or(gamebox.judge_retry_interval_secs),
        event_gamebox: eg,
        gamebox,
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
    crypto: &crate::modules::event::awd::crypto::AwdCrypto,
    event_id: Uuid,
    team_net: &awd_team_networks::Model,
) -> AwdResult<String> {
    use crate::modules::event::awd::crypto::EncryptedBlob;
    let blob = EncryptedBlob {
        ciphertext: team_net.ssh_password_ciphertext.clone(),
        nonce: team_net.ssh_password_nonce.clone(),
        key_version: team_net.key_version,
    };
    let aad = crate::modules::event::awd::crypto::AwdCrypto::build_aad(event_id, "ssh_password");
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

    fn dummy_gamebox(
        repo_digest: Option<&str>,
        image_id: Option<&str>,
        image_ref: Option<&str>,
        status: &str,
    ) -> gameboxes::Model {
        gameboxes::Model {
            id: Uuid::nil(),
            name: "ttt1".into(),
            safe_name: "ttt1".into(),
            category: "other".into(),
            description: String::new(),
            hidden: false,
            created_at: DateTimeWithTimeZone::from(chrono::Utc::now()),
            updated_at: DateTimeWithTimeZone::from(chrono::Utc::now()),
            version: Some("1.0.0".into()),
            source_toml: None,
            spec_json: None,
            spec_digest: None,
            package_digest: Some("b".into()),
            image_ref: image_ref.map(str::to_string),
            image_id: image_id.map(str::to_string),
            image_repo_digest: repo_digest.map(str::to_string),
            username: Some("ctf".into()),
            recommended_cpu_millis: 1000,
            recommended_memory_bytes: 512 * 1024 * 1024,
            recommended_pids_limit: 100,
            healthchecks_json: None,
            judge_script_name: None,
            judge_script_content: None,
            judge_args_json: None,
            judge_timeout_secs: None,
            judge_retry_interval_secs: None,
            build_status: Some(status.into()),
            build_error: None,
            awdp_source_code_dir: None,
            awdp_exploit_script_name: None,
            awdp_exploit_script_content: None,
            awdp_source_artifact_key: None,
            awdp_source_artifact_digest: None,
        }
    }

    #[test]
    fn pinned_image_prefers_repo_digest() {
        let g = dummy_gamebox(
            Some("floatctf/gameboxes/ttt1@sha256:abc"),
            Some("sha256:local"),
            Some("floatctf/gameboxes/ttt1:1.0.0"),
            BUILD_STATUS_READY,
        );
        assert_eq!(
            effective_image_ref_from_gamebox(&g).unwrap(),
            "floatctf/gameboxes/ttt1@sha256:abc"
        );
    }

    #[test]
    fn pinned_image_falls_back_to_image_id_local_only() {
        let g = dummy_gamebox(
            None,
            Some("sha256:localid"),
            Some("floatctf/gameboxes/ttt1:1.0.0"),
            BUILD_STATUS_READY,
        );
        assert_eq!(
            effective_image_ref_from_gamebox(&g).unwrap(),
            "sha256:localid"
        );
    }

    #[test]
    fn ready_without_pin_errors() {
        let g = dummy_gamebox(
            None,
            None,
            Some("floatctf/gameboxes/ttt1:1.0.0"),
            BUILD_STATUS_READY,
        );
        assert!(effective_image_ref_from_gamebox(&g).is_err());
    }
}
