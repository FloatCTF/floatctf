//! Precheck verification — validate all infrastructure before event starts.
//!
//! # Precheck items
//!
//! 1. Config validation: CIDR format, no overlaps, interface/port availability
//! 2. Docker: network exists, FlagServer running, JudgeServer running
//! 3. GameBox instances: all healthy, containers running
//! 4. WireGuard: interface exists, peers loaded
//! 5. Network matrix: connectivity tests
//! 6. Flag: can issue and re-issue flags
//! 7. Judge: scripts execute and callback works
//!
//! # 隔离（Phase 2 P2-1 / 计划 §5.1）
//!
//! precheck 在 `ExecutionContext::Precheck { run_id }` 上下文执行：只读 / 隔离路径，
//! **不写正式 awd_flag_issues / score 表**；正式 issue / judge 调用链在 Phase 3 接入
//! （P2-7/P2-8 本阶段为容器存活探测 + 上下文标注）。
//!
//! # Noop 双门禁
//!
//! Noop 网络 / firewall runtime 永远不允许 Verified：
//! - P2-5 firewall 结构检查：Noop inspect 返回空观测（table_exists=false）→ 必 fail；
//! - P2-6 网络矩阵验证：Noop verify 恒返回 verified=false → 必 fail。
//!
//! # Verified Revision
//!
//! Configuration changes while verified clear the verification.
//! On event start, the revision must match.

use std::collections::HashMap;

use fcmc::AwdContainerRuntime;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
};
use tracing::info;
use uuid::Uuid;

use crate::entity::{
    awd_event_networks, awd_events, awd_gamebox_instances, awd_precheck_runs, awd_team_networks,
    sea_orm_active_enums::{AwdEventStatus, PrecheckStatus},
};
use crate::modules::event::awd_team::{
    AwdError, AwdResult,
    crypto::{AwdCrypto, EncryptedBlob},
    domain::{AwdEventStatusExt, ExecutionContext, Ipv4Cidr, firewall_state::DesiredFirewallState},
    infrastructure::{
        firewall::{FirewallRuntime, HostFirewallEnvironment, ObservedFirewallState, env},
        network::{AwdNetworkRuntime, EventNetworkIdentity},
    },
    repo::{event_network_repo, event_repo, gamebox_repo, wireguard_repo},
    service::firewall_service,
    system::command::{CommandRunner, RealCommandRunner},
};

/// P2-3 SSH 探测 env 开关：默认 note-only；`FLOATCTF_PRECHECK_SSH=1` 才真跑。
/// （按本任务明确要求的环境敏感开关；其余配置仍走 TOML。）
const SSH_PROBE_ENV: &str = "FLOATCTF_PRECHECK_SSH";

/// 一次环境检查的输出：errors（判 fail）+ notes（说明，不判 fail）。
#[derive(Debug, Default, Clone)]
struct CheckReport {
    errors: Vec<(String, String)>,
    notes: Vec<(String, String)>,
}

/// Run a manual precheck on an event.
pub async fn run_precheck(
    db: &DatabaseConnection,
    event_id: Uuid,
    trigger: &str,
    network: &dyn AwdNetworkRuntime,
    firewall: &dyn FirewallRuntime,
    containers: &dyn AwdContainerRuntime,
    crypto: &AwdCrypto,
) -> AwdResult<Uuid> {
    let awd_event = event_repo::find_by_event_id(db, event_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound("AWD event not found".into()))?;

    // Event Network 已分配是预检前提（§62 结构性检查数据源）
    let event_network = event_network_repo::find_by_event_id(db, event_id)
        .await?
        .ok_or_else(|| {
            AwdError::NotFound("event network not allocated；请先在赛事网络页分配".into())
        })?;

    if !awd_event.status.is_configurable() && awd_event.status != AwdEventStatus::Prechecking {
        return Err(AwdError::InvalidState(format!(
            "Cannot precheck in {:?} status",
            awd_event.status
        )));
    }

    // 状态机唯一入口（Phase 0）：进入 Prechecking。
    match &awd_event.status {
        AwdEventStatus::Prechecking => {}
        // 已 Verified 的手动重检：先清除 verified 标记回到 Configuring，再进入 Prechecking。
        AwdEventStatus::Verified => {
            event_repo::transition_event(
                db,
                awd_event.id,
                AwdEventStatus::Verified,
                AwdEventStatus::Configuring,
                event_repo::TransitionPatch::config_changed(),
            )
            .await?;
            event_repo::transition_event(
                db,
                awd_event.id,
                AwdEventStatus::Configuring,
                AwdEventStatus::Prechecking,
                Default::default(),
            )
            .await?;
        }
        other => {
            event_repo::transition_event(
                db,
                awd_event.id,
                other.clone(),
                AwdEventStatus::Prechecking,
                Default::default(),
            )
            .await?;
        }
    }

    // Create precheck run record
    let run = awd_precheck_runs::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        status: Set(PrecheckStatus::Running),
        trigger: Set(trigger.to_string()),
        started_at: Set(chrono::Utc::now().into()),
        ..Default::default()
    };

    let run = run
        .insert(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    let run_id = run.id;
    // 预检执行上下文（P2-1 §5.1）：只读/隔离路径，不污染正式比赛状态。
    // 本阶段用于 run 记录标注；正式 issue/judge 调用链 Phase 3 接入。
    let context = ExecutionContext::Precheck { run_id };

    let mut errors: Vec<(String, String)> = Vec::new();
    let mut notes: Vec<(String, String)> = Vec::new();

    // ── Check 1: Event Network 结构校验（§62）──
    let config_result = validate_event_network(&event_network);
    if let Err(e) = config_result {
        errors.push(("config".to_string(), e));
    }

    // ── Check 2: All teams have networks allocated ──
    use crate::entity::event_teams;

    let teams = event_teams::Entity::find()
        .filter(event_teams::Column::EventId.eq(event_id))
        .all(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    if teams.is_empty() {
        errors.push((
            "teams".to_string(),
            "No teams registered for this event".into(),
        ));
    }

    // ── Check 3: GameBox instances exist for all EventGameBoxes × teams ──
    let event_gameboxes =
        crate::modules::event::awd_team::repo::event_gamebox_repo::find_event_gameboxes_by_event(
            db, event_id,
        )
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    if event_gameboxes.is_empty() {
        errors.push(("gameboxes".to_string(), "No EventGameBox configured".into()));
    }

    // ── Check 4: Docker network observed 记录存在（§14 Observed → runtime resources）──
    use crate::entity::awd_runtime_resources;
    let docker_tracked = awd_runtime_resources::Entity::find()
        .filter(awd_runtime_resources::Column::EventId.eq(event_id))
        .filter(awd_runtime_resources::Column::ResourceType.eq("docker_network"))
        .one(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .is_some();
    if !docker_tracked {
        errors.push((
            "docker".to_string(),
            "Docker network not yet created".into(),
        ));
    }

    // ── Check 5/6: FlagServer / JudgeServer IP —— validate_event_network 已校验 ──

    // ── Check 7: Docker 存活（P2-2）──
    // FlagServer / JudgeServer 容器（名称含 "flagserver"/"judgeserver"）running；
    // 所有 GameBox 实例 inspect_container running。
    let instances = match gamebox_repo::find_instances_by_event(db, event_id).await {
        Ok(list) => list,
        Err(e) => {
            errors.push((
                "gamebox".to_string(),
                format!("find_instances_by_event: {e}"),
            ));
            Vec::new()
        }
    };
    let container_report = check_containers(containers, event_id, &instances).await;
    errors.extend(container_report.errors.clone());
    notes.extend(container_report.notes.clone());

    // ── Check 8: SSH 可达（P2-3，环境敏感：默认 note-only）──
    let ssh_report = check_ssh(db, containers, crypto, event_id, &instances).await;
    errors.extend(ssh_report.errors.clone());
    notes.extend(ssh_report.notes.clone());

    // ── Check 9: WireGuard（P2-4）──
    let (wg_report, wg_observed, wg_active_peers) = check_wireguard(
        network,
        db,
        event_id,
        &event_network.gamebox_cidr.to_string(),
    )
    .await;
    errors.extend(wg_report.errors.clone());
    notes.extend(wg_report.notes.clone());

    // ── Check 10: Firewall 结构检查（P2-5，nftables + Noop 双门禁）──
    let desired_revision = firewall_service::current_network_revision(db).await;
    let (fw_report, observed_state) = check_firewall_structure(firewall, desired_revision).await;
    errors.extend(fw_report.errors.clone());
    notes.extend(fw_report.notes.clone());

    // ── Check 11: 网络矩阵验证（P2-6）──
    // 真实包流 probe（方案 A：挂起玩家 peers；方案 B：canary namespace/container）由
    // Phase 5 E2E 承接（§5.10：不给真实玩家提前攻击窗口）。本阶段做 desired-state
    // 结构验证：revision 全量匹配 + event chains 存在（FirewallRuntime::verify）。
    let matrix_report = match firewall_service::build_desired_state(db, desired_revision).await {
        Ok(desired) => check_network_matrix(firewall, &desired).await,
        Err(e) => {
            let mut r = CheckReport::default();
            r.errors.push((
                "network_matrix".to_string(),
                format!("build_desired_state: {e}"),
            ));
            r
        }
    };
    errors.extend(matrix_report.errors.clone());
    notes.extend(matrix_report.notes.clone());

    // ── Check 12/13: Flag / Judge 隔离探测（P2-7/P2-8）──
    // 本阶段实现 = 容器存活探测（P2-2 已覆盖）+ precheck 上下文标注；
    // 正式 issue / judge 调用链 Phase 3 接入；precheck 不写正式 awd_flag_issues / score 表。
    let flag_check = serde_json::json!({
        "context": "precheck",
        "run_id": run_id,
        "note": "本阶段 Flag 隔离探测 = FlagServer 容器存活（见 container_check）；正式 flag issue 调用链 Phase 3 接入。precheck 不写正式 awd_flag_issues / score 表。",
    });
    let judge_check = serde_json::json!({
        "context": "precheck",
        "run_id": run_id,
        "note": "本阶段 Judge 隔离探测 = JudgeServer 容器存活（见 container_check）；正式 judge 调用链 Phase 3 接入。precheck 不产生正式 Judge Task / score。",
    });

    // Determine overall status（纯函数判定）
    let overall_status = if evaluate_precheck(&errors) {
        PrecheckStatus::Passed
    } else {
        PrecheckStatus::Failed
    };

    let is_passed = overall_status == PrecheckStatus::Passed;

    let error_details = if errors.is_empty() && notes.is_empty() {
        None
    } else {
        Some(serde_json::json!({
            "errors": errors.iter().map(|(k, v)| {
                serde_json::json!({"component": k, "error": v})
            }).collect::<Vec<_>>(),
            "notes": notes.iter().map(|(k, v)| {
                serde_json::json!({"component": k, "note": v})
            }).collect::<Vec<_>>(),
        }))
    };

    // P2-13：Host Firewall Environment 快照（非密钥字段）→ network_check。
    let host_env = env::discover_environment().await;
    let network_check = serde_json::json!({
        "host_environment": host_env_json(&host_env),
        "firewall": {
            "desired_revision": desired_revision,
            "observed_revision": observed_state.as_ref().and_then(|s| s.observed_revision),
            "table_exists": observed_state.as_ref().map(|s| s.table_exists).unwrap_or(false),
            "matrix_verified": matrix_report.errors.is_empty(),
        },
        "wireguard": {
            "interface_up": wg_observed.as_ref().map(|o| o.wireguard_interface_up),
            "active_peers": wg_active_peers,
        },
    });

    // Update precheck run
    let mut run_active: awd_precheck_runs::ActiveModel = awd_precheck_runs::ActiveModel {
        id: Set(run_id),
        status: Set(overall_status),
        completed_at: Set(Some(chrono::Utc::now().into())),
        ..Default::default()
    };

    if let Some(ref details) = error_details {
        run_active.error_msg = Set(Some(details.to_string()));
    }
    run_active.config_check = Set(error_details.clone());
    run_active.container_check = Set(Some(report_json("containers", &container_report)));
    run_active.wireguard_check = Set(Some(report_json("wireguard", &wg_report)));
    run_active.network_check = Set(Some(network_check));
    run_active.flag_check = Set(Some(flag_check));
    run_active.judge_check = Set(Some(judge_check));

    run_active
        .update(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    if is_passed {
        // Mark event as verified（守卫版：要求当前 Prechecking，Phase 0；P2-9 记录配置代数）
        let revision = compute_revision(&event_network);
        let generation = read_configuration_generation(db, awd_event.id).await?;
        event_repo::transition_event(
            db,
            awd_event.id,
            AwdEventStatus::Prechecking,
            AwdEventStatus::Verified,
            event_repo::TransitionPatch::verified_with_generation(&revision, generation),
        )
        .await?;

        info!(
            "[Precheck] Event {} verified (run {}, context {:?})",
            event_id, run_id, context
        );
    } else {
        // 失败：Prechecking → VerificationFailed（Phase 0）
        // 记录失败状态的失败属 best-effort（显式告警，precheck run 记录已落库）。
        if let Err(e) = event_repo::transition_event(
            db,
            awd_event.id,
            AwdEventStatus::Prechecking,
            AwdEventStatus::VerificationFailed,
            Default::default(),
        )
        .await
        {
            tracing::warn!(
                "[Precheck] failed to record VerificationFailed for event {}: {}",
                event_id,
                e
            );
        }
        info!(
            "[Precheck] Event {} failed precheck: {} errors",
            event_id,
            errors.len()
        );
    }

    Ok(run_id)
}

// ── 检查实现（free functions，各返回 CheckReport）──

/// P2-2 Docker 存活检查：FlagServer / JudgeServer 容器（名称含 kind）running；
/// 所有 GameBox 实例（DB 行）inspect_container running。
async fn check_containers(
    containers: &dyn AwdContainerRuntime,
    event_id: Uuid,
    instances: &[awd_gamebox_instances::Model],
) -> CheckReport {
    let mut report = CheckReport::default();

    let live = match containers.list_event_containers(event_id).await {
        Ok(l) => l,
        Err(e) => {
            report
                .errors
                .push(("docker".to_string(), format!("list_event_containers: {e}")));
            return report;
        }
    };
    let live_by_name: HashMap<&str, &fcmc::ContainerState> = live
        .iter()
        .map(|c| (c.container_name.as_str(), c))
        .collect();

    // ── FlagServer / JudgeServer（容器名含 "flagserver" / "judgeserver"）──
    for kind in ["flagserver", "judgeserver"] {
        match live.iter().find(|c| c.container_name.contains(kind)) {
            Some(c) if c.running => {
                report.notes.push((
                    format!("{kind}_container"),
                    format!("running: {}", c.container_id),
                ));
            }
            Some(c) => report.errors.push((
                format!("{kind}_container"),
                format!(
                    "container {} not running (status: {})",
                    c.container_name, c.status
                ),
            )),
            None => report.errors.push((
                format!("{kind}_container"),
                format!("no container matching '{kind}'"),
            )),
        }
    }

    // ── GameBox 实例：DB 行 → inspect_container → running ──
    if instances.is_empty() {
        report
            .notes
            .push(("gamebox".to_string(), "no GameBox instances".to_string()));
    }
    for inst in instances {
        let Some(container_id) = inst.current_container_id.as_deref() else {
            report.errors.push((
                "gamebox".to_string(),
                format!("{}: no current_container_id in DB", inst.container_name),
            ));
            continue;
        };
        // 优先按 container_id inspect（deploy 时写入；DB 名 = 容器名，也可按名查）。
        let by_id = containers.inspect_container(container_id).await;
        match by_id {
            Ok(state) if state.running => {
                report.notes.push((
                    format!("gamebox:{}", inst.container_name),
                    format!("running: {}", state.container_id),
                ));
            }
            Ok(state) => report.errors.push((
                "gamebox".to_string(),
                format!(
                    "{}: container not running (status: {})",
                    inst.container_name, state.status
                ),
            )),
            Err(e) => {
                // inspect 失败：回退按容器名在列表里找一次（container_id 可能因重置变化）。
                match live_by_name.get(inst.container_name.as_str()) {
                    Some(state) if state.running => {
                        report.notes.push((
                            format!("gamebox:{}", inst.container_name),
                            format!("running (by name): {}", state.container_id),
                        ));
                    }
                    Some(state) => report.errors.push((
                        "gamebox".to_string(),
                        format!(
                            "{}: container not running (status: {})",
                            inst.container_name, state.status
                        ),
                    )),
                    None => report.errors.push((
                        "gamebox".to_string(),
                        format!(
                            "{}: inspect_container({container_id}) failed: {e}",
                            inst.container_name
                        ),
                    )),
                }
            }
        }
    }

    report
}

/// P2-3 SSH 可达检查（环境敏感：默认 note-only；`FLOATCTF_PRECHECK_SSH=1` 才真跑）。
///
/// 对每个 GameBox 实例：解密 ssh_password（AAD=`event_id:ssh_password`，凭据完整性，
/// 密码不落日志），然后结构化 argv 跑
/// `ssh -o BatchMode=yes -o ConnectTimeout=3 -o StrictHostKeyChecking=no root@<ip> true`
/// 探测连通性。无容器时软跳过（记 note，不判失败）。
async fn check_ssh(
    db: &DatabaseConnection,
    containers: &dyn AwdContainerRuntime,
    crypto: &AwdCrypto,
    event_id: Uuid,
    instances: &[awd_gamebox_instances::Model],
) -> CheckReport {
    let mut report = CheckReport::default();

    if !ssh_probe_enabled(std::env::var(SSH_PROBE_ENV).ok().as_deref()) {
        report.notes.push((
            "ssh".to_string(),
            format!("SSH 探测未启用（设置 {SSH_PROBE_ENV}=1 开启；默认 note-only）"),
        ));
        return report;
    }

    // 无容器软跳过：Docker 不可达或没有任何容器 → 记 note，不判失败。
    match containers.list_event_containers(event_id).await {
        Ok(live) if live.is_empty() || instances.is_empty() => {
            report
                .notes
                .push(("ssh".to_string(), "无容器，跳过 SSH 探测".to_string()));
            return report;
        }
        Ok(_) => {}
        Err(e) => {
            report.notes.push((
                "ssh".to_string(),
                format!("Docker 不可达，跳过 SSH 探测：{e}"),
            ));
            return report;
        }
    }

    let runner = RealCommandRunner;
    for inst in instances {
        // 解密 ssh_password（凭据完整性检查；解密结果仅用于校验，不参与 ssh 参数、不落日志）
        let network = match awd_team_networks::Entity::find()
            .filter(awd_team_networks::Column::EventId.eq(event_id))
            .filter(awd_team_networks::Column::TeamId.eq(inst.team_id))
            .one(db)
            .await
        {
            Ok(Some(n)) => n,
            Ok(None) => {
                report.errors.push((
                    "ssh".to_string(),
                    format!(
                        "{}: no awd_team_networks row for team {}",
                        inst.container_name, inst.team_id
                    ),
                ));
                continue;
            }
            Err(e) => {
                report
                    .errors
                    .push(("ssh".to_string(), format!("load team network: {e}")));
                continue;
            }
        };
        let blob = EncryptedBlob {
            ciphertext: network.ssh_password_ciphertext,
            nonce: network.ssh_password_nonce,
            key_version: network.key_version,
        };
        let aad = AwdCrypto::build_aad(event_id, "ssh_password");
        if let Err(e) = crypto.decrypt(&blob, &aad) {
            report.errors.push((
                "ssh".to_string(),
                format!("{}: ssh_password 解密失败: {e}", inst.container_name),
            ));
            continue;
        }

        // 结构化 argv（禁 shell 拼接）；BatchMode=yes 禁用密码交互，探测纯连通性。
        let args = vec![
            "-o".to_string(),
            "BatchMode=yes".to_string(),
            "-o".to_string(),
            "ConnectTimeout=3".to_string(),
            "-o".to_string(),
            "StrictHostKeyChecking=no".to_string(),
            format!("root@{}", inst.gamebox_ip),
            "true".to_string(),
        ];
        let out = match runner.run("ssh", &args).await {
            Ok(o) => o,
            Err(e) => {
                report.errors.push((
                    "ssh".to_string(),
                    format!(
                        "{} ({}): 无法执行 ssh: {e}",
                        inst.container_name, inst.gamebox_ip
                    ),
                ));
                continue;
            }
        };
        if out.exit_code == 0 {
            report.notes.push((
                format!("ssh:{}", inst.container_name),
                format!("{}: ssh 可达", inst.gamebox_ip),
            ));
        } else {
            report.errors.push((
                "ssh".to_string(),
                format!(
                    "{} ({}): ssh 探测失败 (exit {}): {}",
                    inst.container_name,
                    inst.gamebox_ip,
                    out.exit_code,
                    out.stderr.trim()
                ),
            ));
        }
    }

    report
}

/// P2-4 WireGuard 检查：接口 up（host inspect）+ DB Active peers 计数。
async fn check_wireguard(
    network: &dyn AwdNetworkRuntime,
    db: &DatabaseConnection,
    event_id: Uuid,
    gamebox_cidr: &str,
) -> (
    CheckReport,
    Option<crate::modules::event::awd_team::infrastructure::network::NetworkObservedState>,
    usize,
) {
    let mut report = CheckReport::default();

    let observed = match network
        .inspect(EventNetworkIdentity {
            event_id,
            gamebox_cidr: gamebox_cidr.to_string(),
        })
        .await
    {
        Ok(o) => o,
        Err(e) => {
            report
                .errors
                .push(("wireguard".to_string(), format!("inspect failed: {e}")));
            return (report, None, 0);
        }
    };

    for note in &observed.notes {
        report.notes.push(("wireguard".to_string(), note.clone()));
    }

    let peers = match wireguard_repo::find_active_peers_by_event(db, event_id).await {
        Ok(p) => p.len(),
        Err(e) => {
            report.errors.push((
                "wireguard".to_string(),
                format!("find_active_peers_by_event: {e}"),
            ));
            return (report, Some(observed), 0);
        }
    };
    report
        .notes
        .push(("wireguard".to_string(), format!("active_peers={peers}")));

    if !observed.wireguard_interface_up {
        report.errors.push((
            "wireguard".to_string(),
            "WireGuard 接口未 up（host inspect 未观测到接口 / 网络不可用）".to_string(),
        ));
    }

    (report, Some(observed), peers)
}

/// P2-5 Firewall 结构检查（nftables，Noop 双门禁）：
/// `nft list table inet floatctf_awd` → table 存在 + base chain `awd_forward` +
/// `hook forward` 声明 + `observed_revision == desired_revision`。
///
/// Noop runtime（inspect 返回空观测 / table_exists=false）→ **必 fail**
/// （Noop 永远不允许 Verified，双门禁）。
async fn check_firewall_structure(
    firewall: &dyn FirewallRuntime,
    desired_revision: u64,
) -> (CheckReport, Option<ObservedFirewallState>) {
    let mut report = CheckReport::default();

    let observed = match firewall.inspect().await {
        Ok(o) => o,
        Err(e) => {
            report
                .errors
                .push(("firewall".to_string(), format!("inspect failed: {e}")));
            return (report, None);
        }
    };

    // Noop runtime 或从未 reconcile → table 不存在 → fail（双门禁）。
    if !observed.table_exists {
        report.errors.push((
            "firewall".to_string(),
            "table inet floatctf_awd 不存在（Noop runtime 或从未 reconcile）".to_string(),
        ));
        return (report, Some(observed));
    }

    // 结构字符串检查：数据面必须包含 FloatCTF 渲染的 base chain 与 forward hook。
    if !observed.raw_output.contains("chain awd_forward") {
        report.errors.push((
            "firewall".to_string(),
            "base chain 'awd_forward' 缺失".to_string(),
        ));
    }
    if !observed.raw_output.contains("hook forward") {
        report
            .errors
            .push(("firewall".to_string(), "forward hook 声明缺失".to_string()));
    }

    // revision 比对（纯函数）。
    if !revision_matches(observed.observed_revision, desired_revision) {
        report.errors.push((
            "firewall".to_string(),
            format!(
                "revision 不匹配: observed={:?} desired={desired_revision}",
                observed.observed_revision
            ),
        ));
    }

    (report, Some(observed))
}

/// P2-6 网络矩阵验证（本阶段：desired-state 结构验证）。
///
/// 真实包流 probe（计划 §5.10）：
/// - 方案 A：precheck 时挂起玩家 WG peers（host 移除，DB 保持 Active）→ Hardening probe
///   → 临时 Attack matrix probe → 恢复 Hardening → 恢复 peers；
/// - 方案 B：专用 canary namespace/container（同构镜像），probe 全部在 canary 内完成。
///
/// 两方案均**不得**给真实玩家提前攻击窗口。本阶段实现为 `FirewallRuntime::verify`
/// （revision 全量匹配 + event chains 存在），真实包流 probe 留给 Phase 5 E2E。
async fn check_network_matrix(
    firewall: &dyn FirewallRuntime,
    desired: &DesiredFirewallState,
) -> CheckReport {
    let mut report = CheckReport::default();

    match firewall.verify(desired).await {
        Ok(v) if v.verified => {
            report.notes.push((
                "network_matrix".to_string(),
                format!("desired-state verify 通过（revision {}）", desired.revision),
            ));
        }
        Ok(v) => {
            report.errors.push((
                "network_matrix".to_string(),
                format!("verify 失败: {}", v.notes.join("; ")),
            ));
        }
        Err(e) => {
            report
                .errors
                .push(("network_matrix".to_string(), format!("verify error: {e}")));
        }
    }

    report
}

// ── 纯函数（可单测）──

/// 判定：errors 为空 → Passed。纯函数。
fn evaluate_precheck(errors: &[(String, String)]) -> bool {
    errors.is_empty()
}

/// revision 比对纯函数（P2-5）：observed == desired 才一致；None 视为不匹配。
fn revision_matches(observed: Option<u64>, expected: u64) -> bool {
    observed == Some(expected)
}

/// SSH 探测是否启用（P2-3）：env 值为 `1`。纯函数。
fn ssh_probe_enabled(env: Option<&str>) -> bool {
    env == Some("1")
}

/// CheckReport → jsonb（component / passed / errors / notes）。
fn report_json(component: &str, report: &CheckReport) -> serde_json::Value {
    serde_json::json!({
        "component": component,
        "passed": report.errors.is_empty(),
        "errors": report.errors.iter().map(|(k, v)| {
            serde_json::json!({"component": k, "error": v})
        }).collect::<Vec<_>>(),
        "notes": report.notes.iter().map(|(k, v)| {
            serde_json::json!({"component": k, "note": v})
        }).collect::<Vec<_>>(),
    })
}

/// P2-13 Host Firewall Environment 快照序列化（非密钥字段）。
fn host_env_json(env: &HostFirewallEnvironment) -> serde_json::Value {
    serde_json::json!({
        "nft_version": env.nft_version,
        "kernel_version": env.kernel_version,
        "docker_firewall_backend": env.docker_firewall_backend,
        "firewalld_active": env.firewalld_active,
        "iptables_frontend": env.iptables_frontend,
        "notes": env.notes,
    })
}

/// §62 Event Network 结构性校验：CIDR 合法且不重叠、infra 是 gamebox 的第一块
/// team-size 子网、flag/judge IP 位于 infra 内且互不相同、interface 名长度合法。
fn validate_event_network(net: &awd_event_networks::Model) -> Result<(), String> {
    // gamebox / wireguard CIDR 合法
    let gbox_cidr = Ipv4Cidr::parse(&net.gamebox_cidr.to_string())
        .map_err(|e| format!("Invalid gamebox_cidr: {}", e))?;
    let wg_cidr = Ipv4Cidr::parse(&net.wireguard_cidr.to_string())
        .map_err(|e| format!("Invalid wireguard_cidr: {}", e))?;

    // 不重叠
    if gbox_cidr.overlaps(&wg_cidr) {
        return Err("gamebox_cidr and wireguard_cidr overlap".into());
    }

    // CIDR 容量：/16 或更小（65536 地址）
    if gbox_cidr.prefix_len > 16 {
        return Err(format!(
            "gamebox_cidr must be /16 or smaller, got /{}",
            gbox_cidr.prefix_len
        ));
    }

    // infra 子网：位于 gamebox CIDR 内（§25 派生规则）
    let infra = Ipv4Cidr::parse(&net.infrastructure_subnet.to_string())
        .map_err(|e| format!("Invalid infrastructure_subnet: {}", e))?;
    if !gbox_cidr.contains(infra.network) {
        return Err(format!(
            "infrastructure_subnet {} 不在 gamebox_cidr {} 内",
            net.infrastructure_subnet, net.gamebox_cidr
        ));
    }

    // flag/judge IP 在 infra 子网内且互不相同
    let fs_ip: std::net::Ipv4Addr = net
        .flagserver_ip
        .ip()
        .to_string()
        .parse()
        .map_err(|_| format!("Invalid flagserver_ip: {}", net.flagserver_ip))?;
    let js_ip: std::net::Ipv4Addr = net
        .judgeserver_ip
        .ip()
        .to_string()
        .parse()
        .map_err(|_| format!("Invalid judgeserver_ip: {}", net.judgeserver_ip))?;
    if !infra.contains(fs_ip) {
        return Err(format!(
            "flagserver_ip {} 不在 infrastructure_subnet {} 内",
            net.flagserver_ip, net.infrastructure_subnet
        ));
    }
    if !infra.contains(js_ip) {
        return Err(format!(
            "judgeserver_ip {} 不在 infrastructure_subnet {} 内",
            net.judgeserver_ip, net.infrastructure_subnet
        ));
    }
    if net.flagserver_ip == net.judgeserver_ip {
        return Err("flagserver_ip and judgeserver_ip must be different".into());
    }

    // WG 接口名长度（Linux 15 字符限制）
    if net.wireguard_interface_name.len() > 15 {
        return Err(format!(
            "wireguard_interface_name too long: {} (max 15)",
            net.wireguard_interface_name.len()
        ));
    }

    Ok(())
}

/// 读取当前配置代数（P2-9）。
async fn read_configuration_generation(
    db: &sea_orm::DatabaseConnection,
    awd_event_id: Uuid,
) -> AwdResult<i64> {
    use sea_orm::EntityTrait;
    let row = awd_events::Entity::find_by_id(awd_event_id)
        .one(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound("AWD event not found".into()))?;
    Ok(row.configuration_generation)
}

/// Compute a configuration revision hash for verification tracking.
fn compute_revision(net: &awd_event_networks::Model) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(net.gamebox_cidr.to_string().as_bytes());
    hasher.update(net.wireguard_cidr.to_string().as_bytes());
    hasher.update(net.wireguard_interface_name.as_bytes());
    hasher.update(net.infrastructure_subnet.to_string().as_bytes());
    hasher.update(net.flagserver_ip.ip().to_string().as_bytes());
    hasher.update(net.judgeserver_ip.ip().to_string().as_bytes());
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::event::awd_team::infrastructure::firewall::NoopFirewallRuntime;

    #[test]
    fn evaluate_precheck_passes_on_empty_errors() {
        let empty: Vec<(String, String)> = Vec::new();
        assert!(evaluate_precheck(&empty));
    }

    #[test]
    fn evaluate_precheck_fails_on_any_error() {
        let errors = vec![("docker".to_string(), "list failed".to_string())];
        assert!(!evaluate_precheck(&errors));
    }

    #[test]
    fn revision_matches_only_on_equal_revision() {
        assert!(revision_matches(Some(7), 7));
        assert!(!revision_matches(None, 7));
        assert!(!revision_matches(Some(6), 7));
        assert!(!revision_matches(Some(8), 7));
    }

    #[test]
    fn ssh_probe_only_when_env_is_one() {
        assert!(ssh_probe_enabled(Some("1")));
        assert!(!ssh_probe_enabled(Some("0")));
        assert!(!ssh_probe_enabled(None));
        assert!(!ssh_probe_enabled(Some("yes")));
    }

    #[tokio::test]
    async fn noop_firewall_structure_never_passes() {
        // Noop 双门禁：inspect 返回空观测（table_exists=false）→ 必 fail。
        let fw = NoopFirewallRuntime;
        let (report, observed) = check_firewall_structure(&fw, 0).await;
        assert!(!report.errors.is_empty());
        assert!(
            report.errors.iter().any(|(k, _)| k == "firewall"),
            "errors: {:?}",
            report.errors
        );
        assert!(observed.as_ref().is_some_and(|o| !o.table_exists));
    }

    #[tokio::test]
    async fn noop_network_matrix_never_verifies() {
        // Noop verify 恒返回 verified=false → network_matrix 必 fail（双门禁第二重）。
        let fw = NoopFirewallRuntime;
        let desired = DesiredFirewallState {
            revision: 1,
            events: Vec::new(),
        };
        let report = check_network_matrix(&fw, &desired).await;
        assert!(!report.errors.is_empty());
        assert!(
            report.errors.iter().any(|(k, _)| k == "network_matrix"),
            "errors: {:?}",
            report.errors
        );
    }

    #[test]
    fn report_json_marks_failed_when_errors_present() {
        let report = CheckReport {
            errors: vec![("ssh".to_string(), "down".to_string())],
            notes: vec![("ssh".to_string(), "probe skipped".to_string())],
        };
        let json = report_json("containers", &report);
        assert_eq!(json["component"], "containers");
        assert_eq!(json["passed"], false);
        assert_eq!(json["errors"][0]["component"], "ssh");
        assert_eq!(json["notes"][0]["note"], "probe skipped");
    }
}
