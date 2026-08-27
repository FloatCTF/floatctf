//! NftablesFirewallRuntime — FloatCTF AWD 唯一生产 firewall runtime（Phase 1 P1-3/5/9）。
//!
//! 只管理 FloatCTF 自己拥有的 `table inet floatctf_awd`：
//! - 禁止 `nft flush ruleset`
//! - 禁止修改 Docker/firewalld/libvirt/其他程序 tables
//! - 完整 reconcile = render 整个 table → `nft -c` 校验 → `nft -f` 原子应用 → verify
//! - 失败 Fail Closed（调用方据此置 NetworkError）
//!
//! 所有命令走统一 privileged command abstraction（structured argv，禁 shell 拼接）。

use std::sync::Arc;

use async_trait::async_trait;

use crate::modules::event::awd::{
    AwdError, AwdResult,
    domain::firewall_state::DesiredFirewallState,
    system::command::{CommandRunner, RealCommandRunner},
};

use super::{
    FirewallApplyResult, FirewallRuntime, FirewallVerification, ObservedFirewallState, TABLE_NAME,
    render,
};

/// nft 可执行文件名。
const NFT_BIN: &str = "nft";

/// 生产实现：native nftables。
pub struct NftablesFirewallRuntime {
    runner: Arc<dyn CommandRunner>,
}

impl NftablesFirewallRuntime {
    pub fn new() -> Self {
        Self {
            runner: Arc::new(RealCommandRunner),
        }
    }

    #[cfg(test)]
    pub fn with_runner(runner: Arc<dyn CommandRunner>) -> Self {
        Self { runner }
    }

    /// `nft -f <file>`：原子应用。
    async fn apply_file(&self, path: &str) -> AwdResult<()> {
        let out = self
            .runner
            .run(NFT_BIN, &["-f".to_string(), path.to_string()])
            .await
            .map_err(|e| AwdError::Network(format!("nft apply failed to run: {e}")))?;
        if out.exit_code != 0 {
            return Err(AwdError::Network(format!(
                "nft -f failed (exit {}): {}",
                out.exit_code, out.stderr
            )));
        }
        Ok(())
    }

    /// `nft -c -f <file>`：语法校验（不改变规则）。
    async fn check_file(&self, path: &str) -> AwdResult<()> {
        let out = self
            .runner
            .run(
                NFT_BIN,
                &["-c".to_string(), "-f".to_string(), path.to_string()],
            )
            .await
            .map_err(|e| AwdError::Network(format!("nft -c failed to run: {e}")))?;
        if out.exit_code != 0 {
            return Err(AwdError::Network(format!(
                "nft -c syntax check failed (exit {}): {}",
                out.exit_code, out.stderr
            )));
        }
        Ok(())
    }

    /// `nft list table inet floatctf_awd`：观测当前状态。
    async fn list_table(&self) -> AwdResult<String> {
        let out = self
            .runner
            .run(
                NFT_BIN,
                &[
                    "list".to_string(),
                    "table".to_string(),
                    "inet".to_string(),
                    TABLE_NAME.to_string(),
                ],
            )
            .await
            .map_err(|e| AwdError::Network(format!("nft list failed to run: {e}")))?;
        if out.exit_code != 0 {
            // table 不存在时 nft 返回非 0 —— 视为空观测
            return Ok(String::new());
        }
        Ok(out.stdout)
    }
}

impl Default for NftablesFirewallRuntime {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl FirewallRuntime for NftablesFirewallRuntime {
    async fn inspect(&self) -> AwdResult<ObservedFirewallState> {
        let out = self.list_table().await?;
        let mut state = render::parse_observed_table(&out);
        if !state.table_exists && !out.trim().is_empty() {
            // 非空输出但无我们的 table —— 观测异常（他方规则存在，我们无权处理）
            state
                .notes
                .push("nft list returned output without floatctf_awd table".into());
        }
        Ok(state)
    }

    async fn reconcile(&self, desired: &DesiredFirewallState) -> AwdResult<FirewallApplyResult> {
        // 0. 空态（无任何赛事需要策略）→ 删除整个 floatctf_awd table（P4-13 方案 B）
        if desired.is_empty() {
            let observed = self.inspect().await?;
            if observed.table_exists {
                let delete = format!("delete table inet {TABLE_NAME}\n");
                let delete_tmp = tempfile::NamedTempFile::new()
                    .map_err(|e| AwdError::Network(format!("nft temp file: {e}")))?;
                std::fs::write(delete_tmp.path(), &delete)
                    .map_err(|e| AwdError::Network(format!("nft ruleset write: {e}")))?;
                let delete_path = delete_tmp
                    .path()
                    .to_str()
                    .ok_or_else(|| AwdError::Network("temp path not utf8".into()))?
                    .to_string();
                self.apply_file(&delete_path).await?;
            }
            return Ok(FirewallApplyResult {
                revision: desired.revision,
                applied: true,
            });
        }

        // 1. render 完整 table（整个 floatctf_awd 内容）
        let ruleset = render::render_table(desired);
        let tmp = tempfile::NamedTempFile::new()
            .map_err(|e| AwdError::Network(format!("nft temp file: {e}")))?;
        std::fs::write(tmp.path(), &ruleset)
            .map_err(|e| AwdError::Network(format!("nft ruleset write: {e}")))?;
        let path = tmp
            .path()
            .to_str()
            .ok_or_else(|| AwdError::Network("temp path not utf8".into()))?
            .to_string();

        // 2. 语法校验（nft -c）——失败不改动数据面
        self.check_file(&path).await?;

        // 3. 原子应用（nft -f）：先删除自有 table 再整表重建（唯一允许的 table 级操作）
        let delete = format!("delete table inet {TABLE_NAME}\n");
        let delete_tmp = tempfile::NamedTempFile::new()
            .map_err(|e| AwdError::Network(format!("nft temp file: {e}")))?;
        std::fs::write(delete_tmp.path(), &delete)
            .map_err(|e| AwdError::Network(format!("nft ruleset write: {e}")))?;
        let delete_path = delete_tmp
            .path()
            .to_str()
            .ok_or_else(|| AwdError::Network("temp path not utf8".into()))?
            .to_string();

        // 3a. 若 table 已存在则删除（仅自有 table，所有权铁律 §4 允许）
        let observed = self.inspect().await?;
        if observed.table_exists {
            self.apply_file(&delete_path).await?;
        }
        // 3b. 整表重建（原子）
        self.apply_file(&path).await?;

        // 4. verify：revision 落定
        let verified = self.verify(desired).await?;
        if !verified.verified {
            return Err(AwdError::Network(format!(
                "nft reconcile verify failed: {}",
                verified.notes.join("; ")
            )));
        }

        Ok(FirewallApplyResult {
            revision: desired.revision,
            applied: true,
        })
    }

    async fn verify(&self, desired: &DesiredFirewallState) -> AwdResult<FirewallVerification> {
        let out = self.list_table().await?;
        let observed = render::parse_observed_table(&out);
        let mut notes = Vec::new();

        if !observed.table_exists {
            notes.push("table inet floatctf_awd missing".into());
        }
        if observed.observed_revision != Some(desired.revision) {
            notes.push(format!(
                "revision mismatch: desired={} observed={:?}",
                desired.revision, observed.observed_revision
            ));
        }
        for event in &desired.events {
            let chain = format!(
                "event_{}",
                render::NftObjectName::event_key(&event.event_id).as_str()
            );
            if !observed.event_chains.contains(&chain) {
                notes.push(format!("missing chain {chain}"));
            }
        }

        Ok(FirewallVerification {
            verified: notes.is_empty(),
            observed,
            notes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::event::awd::{
        domain::firewall_state::{DesiredEventPolicy, DesiredTeamPolicy, IpNet},
        system::command::{CommandOutput, RecordingCommandRunner},
    };
    use uuid::Uuid;

    fn sample_desired(revision: u64) -> DesiredFirewallState {
        DesiredFirewallState {
            revision,
            events: vec![DesiredEventPolicy {
                event_key: "ev_test".into(),
                event_id: Uuid::new_v4(),
                phase: crate::entity::sea_orm_active_enums::AwdPhase::Hardening,
                gamebox_network: IpNet::parse("10.42.0.0/16").unwrap(),
                infrastructure_network: IpNet::parse("10.42.0.0/24").unwrap(),
                flagserver_ip: "10.42.0.10".parse().unwrap(),
                judgeserver_ip: "10.42.0.11".parse().unwrap(),
                teams: vec![DesiredTeamPolicy {
                    team_id: Uuid::new_v4(),
                    wireguard_network: IpNet::parse("172.31.1.0/24").unwrap(),
                    gamebox_network: IpNet::parse("10.42.1.0/24").unwrap(),
                }],
                banned_teams: vec![],
                is_final_settlement: false,
            }],
        }
    }

    /// 模拟 nft 行为：list 返回上次 apply 的 ruleset；-c/-f 成功。
    #[derive(Clone)]
    struct FakeNftRunner {
        store: std::sync::Arc<std::sync::Mutex<Option<String>>>,
    }

    #[async_trait]
    impl CommandRunner for FakeNftRunner {
        async fn run(&self, program: &str, args: &[String]) -> anyhow::Result<CommandOutput> {
            assert_eq!(program, "nft");
            if args.first().map(|s| s.as_str()) == Some("list") {
                let out = self.store.lock().unwrap().clone().unwrap_or_default();
                return Ok(CommandOutput {
                    exit_code: if out.is_empty() { 1 } else { 0 },
                    stdout: out,
                    stderr: String::new(),
                });
            }
            if args.contains(&"-f".to_string()) {
                let idx = args.iter().position(|a| a == "-f").unwrap() + 1;
                let path = &args[idx];
                let content = std::fs::read_to_string(path).unwrap();
                let mut store = self.store.lock().unwrap();
                if args.contains(&"-c".to_string()) {
                    // 校验模式不改 store
                    return Ok(CommandOutput {
                        exit_code: 0,
                        stdout: String::new(),
                        stderr: String::new(),
                    });
                }
                *store = Some(content);
                return Ok(CommandOutput {
                    exit_code: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                });
            }
            Ok(CommandOutput {
                exit_code: 0,
                stdout: String::new(),
                stderr: String::new(),
            })
        }
    }

    #[tokio::test]
    async fn reconcile_renders_applies_and_verifies() {
        let runner = Arc::new(FakeNftRunner {
            store: Default::default(),
        });
        let rt = NftablesFirewallRuntime::with_runner(runner.clone());

        let desired = sample_desired(11);
        let res = rt.reconcile(&desired).await.expect("reconcile ok");
        assert!(res.applied);
        assert_eq!(res.revision, 11);

        // verify 通过：revision 一致 + event chain 存在
        let v = rt.verify(&desired).await.unwrap();
        assert!(v.verified, "notes: {:?}", v.notes);

        // 观测状态解析正确
        let obs = rt.inspect().await.unwrap();
        assert!(obs.table_exists);
        assert_eq!(obs.observed_revision, Some(11));
    }

    #[tokio::test]
    async fn reconcile_verify_fails_on_revision_mismatch() {
        let runner = Arc::new(FakeNftRunner {
            store: Default::default(),
        });
        let rt = NftablesFirewallRuntime::with_runner(runner.clone());

        let desired = sample_desired(11);
        rt.reconcile(&desired).await.unwrap();

        // 期望 revision 变化但 store 未变 → verify 失败
        let desired2 = sample_desired(12);
        let v = rt.verify(&desired2).await.unwrap();
        assert!(!v.verified);
    }

    #[tokio::test]
    async fn reconcile_uses_check_before_apply() {
        // 记录命令序列：必须 -c 在 -f 之前
        let recorder = Arc::new(RecordingCommandRunner::new());
        let rt = NftablesFirewallRuntime::with_runner(recorder.clone());
        // 无 FakeNft 的 store —— RecordingCommandRunner 全部返回成功；
        // verify 会失败（store 为空），但先验证命令序列。
        let desired = sample_desired(1);
        let _ = rt.reconcile(&desired).await;

        let cmds = recorder.recorded();
        let nft_cmds: Vec<(String, Vec<String>)> =
            cmds.into_iter().filter(|(p, _)| p == "nft").collect();
        // list → (-c -f) → delete(-f) → (-f) → list → list
        let check_idx = nft_cmds
            .iter()
            .position(|(_, a)| a.first().map(|s| s.as_str()) == Some("-c"));
        let apply_idx = nft_cmds
            .iter()
            .rposition(|(_, a)| a.first().map(|s| s.as_str()) == Some("-f"));
        assert!(check_idx.is_some(), "must run nft -c first");
        assert!(apply_idx.is_some(), "must run nft -f");
        assert!(check_idx.unwrap() < apply_idx.unwrap());
    }

    #[tokio::test]
    async fn check_failure_does_not_apply() {
        struct FailCheck;
        #[async_trait]
        impl CommandRunner for FailCheck {
            async fn run(&self, _program: &str, args: &[String]) -> anyhow::Result<CommandOutput> {
                if args.contains(&"-c".to_string()) {
                    return Ok(CommandOutput {
                        exit_code: 1,
                        stdout: String::new(),
                        stderr: "syntax error at line 1".into(),
                    });
                }
                Ok(CommandOutput {
                    exit_code: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                })
            }
        }
        let rt = NftablesFirewallRuntime::with_runner(Arc::new(FailCheck));
        let err = rt
            .reconcile(&sample_desired(1))
            .await
            .expect_err("must fail");
        assert!(err.to_string().contains("syntax"));
    }
}
