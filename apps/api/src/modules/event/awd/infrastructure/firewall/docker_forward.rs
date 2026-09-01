//! Docker 29 nftables 反欺骗兼容层（Phase 9.1）。
//!
//! 真实部署集成发现（chore/awd-phase9-real-e2e-report.md §2.3）：
//! Docker 29.7.2（iptables-nft 后端）为每个桥接网络上的容器注入两条 terminal drop：
//!
//! - `table ip raw PREROUTING`：`ip daddr <容器IP> iifname != <桥> drop`
//!   （反 IP 欺骗：非桥接口进入、目的地为容器 IP 的包直接丢弃）；
//! - `table ip filter DOCKER`：`iifname != <桥> oifname <桥> drop`
//!   （非桥接口进入、从桥出去的包直接丢弃）。
//!
//! 两条规则都在 FloatCTF 链（`inet floatctf_awd` forward priority 1）之前执行且为
//! terminal verdict —— 玩家经 WireGuard（`fawg_<8hex>`，非桥接口）访问 GameBox 的
//! 流量在到达 FloatCTF 防火墙之前就被 Docker 吞掉（真实主机实测：SYN 丢失）。
//! `--internal=false` 只解决 DOCKER-INTERNAL 的 drop，不解决这两条。
//!
//! 生产修复（本模块）：
//! - `table ip raw PREROUTING`：在链首插入
//!   `-i <wg_iface> -d <event gamebox_cidr> -j ACCEPT`
//!   （仅跳过本赛事 CIDR 的反欺骗检查；FloatCTF forward 链仍执行全部隔离矩阵）；
//! - `table ip filter DOCKER-USER`：插入
//!   `-i <wg_iface> -o <event bridge> -j ACCEPT`
//!   （DOCKER-USER 是 Docker 官方文档指定的用户自定义链，先于 DOCKER-FORWARD 执行）。
//!
//! 为什么这是"受支持"的集成机制（而非盲目硬改 Docker 链）：
//! 1. 两个表都由 iptables-nft 管理（`managed by iptables-nft` 注释），用 `iptables`
//!    兼容二进制（宿主的 v1.8.13 nf_tables）插入/删除是与管理方一致的唯一受支持方式；
//! 2. DOCKER-USER 是 Docker 文档明确提供的用户规则挂载点（先于 Docker 自身规则）；
//! 3. 规则严格按赛事作用域限定（wg 接口名 + 赛事桥名 + 赛事 CIDR），幂等（-C 检查
//!    后插入 / -D 删除），不触碰任何其他 Docker/libvirt/Incus 网络；
//! 4. 插入位置固定为链首，Docker 之后追加的反欺骗规则永远落在其后；
//! 5. 删除即完全还原（真实主机验证：删除后恢复阻断）。
//!
//! 生命周期：deploy 时 ensure；precheck 时校验（缺失报错，可重部署修复）；
//! archive 时 remove。

use std::sync::Arc;

use async_trait::async_trait;
use tracing::warn;

use crate::modules::event::awd::{
    AwdError, AwdResult,
    system::command::{CommandRunner, RealCommandRunner},
};

/// iptables（iptables-nft）可执行文件名。
const IPTABLES_BIN: &str = "iptables";

/// 单个赛事作用域的 Docker 反欺骗放行规则（raw PREROUTING + DOCKER-USER）。
#[derive(Debug, Clone)]
pub struct DockerForwardAccessSpec {
    /// WireGuard 接口名（如 `fawg_ab12cd34`）——规则按此接口放行。
    pub wg_interface: String,
    /// 事件 Docker 桥接口名（如 `fctfawdab12cd34`，≤15 字符）。
    pub bridge_name: String,
    /// 事件 GameBox CIDR（如 `10.4.0.0/16`）——raw 规则按此限定目的地。
    pub gamebox_cidr: String,
}

/// 生产实现：经 `iptables`（iptables-nft）维护 Docker 反欺骗放行规则。
pub struct DockerForwardRuntime {
    runner: Arc<dyn CommandRunner>,
}

impl DockerForwardRuntime {
    pub fn new() -> Self {
        Self {
            runner: Arc::new(RealCommandRunner),
        }
    }

    #[cfg(test)]
    pub fn with_runner(runner: Arc<dyn CommandRunner>) -> Self {
        Self { runner }
    }

    /// 幂等放行：规则不存在则插入；已存在则跳过。
    pub async fn ensure_access(&self, spec: &DockerForwardAccessSpec) -> AwdResult<()> {
        // raw PREROUTING：-C 检查（exit 0 = 存在；1 = 缺失；其他 = 错误）
        match self.runner.run(IPTABLES_BIN, &raw_check_args(spec)).await {
            Ok(out) if out.exit_code == 0 => {}
            Ok(_) => {
                let out = self
                    .runner
                    .run(IPTABLES_BIN, &raw_insert_args(spec))
                    .await
                    .map_err(|e| AwdError::Network(format!("iptables raw insert: {e}")))?;
                if out.exit_code != 0 {
                    return Err(AwdError::Network(format!(
                        "iptables raw insert failed (exit {}): {}",
                        out.exit_code, out.stderr
                    )));
                }
            }
            Err(e) => {
                return Err(AwdError::Network(format!("iptables raw check: {e}")));
            }
        }

        // filter DOCKER-USER：同上
        match self.runner.run(IPTABLES_BIN, &user_check_args(spec)).await {
            Ok(out) if out.exit_code == 0 => {}
            Ok(_) => {
                let out = self
                    .runner
                    .run(IPTABLES_BIN, &user_insert_args(spec))
                    .await
                    .map_err(|e| AwdError::Network(format!("iptables DOCKER-USER insert: {e}")))?;
                if out.exit_code != 0 {
                    return Err(AwdError::Network(format!(
                        "iptables DOCKER-USER insert failed (exit {}): {}",
                        out.exit_code, out.stderr
                    )));
                }
            }
            Err(e) => {
                return Err(AwdError::Network(format!(
                    "iptables DOCKER-USER check: {e}"
                )));
            }
        }

        Ok(())
    }

    /// 幂等校验（precheck 用）：两条规则都必须存在。
    pub async fn check_access(&self, spec: &DockerForwardAccessSpec) -> AwdResult<Vec<String>> {
        let mut missing = Vec::new();
        match self.runner.run(IPTABLES_BIN, &raw_check_args(spec)).await {
            Ok(out) if out.exit_code == 0 => {}
            Ok(_) => missing.push("raw PREROUTING ACCEPT".to_string()),
            Err(e) => return Err(AwdError::Network(format!("iptables raw check: {e}"))),
        }
        match self.runner.run(IPTABLES_BIN, &user_check_args(spec)).await {
            Ok(out) if out.exit_code == 0 => {}
            Ok(_) => missing.push("DOCKER-USER ACCEPT".to_string()),
            Err(e) => {
                return Err(AwdError::Network(format!(
                    "iptables DOCKER-USER check: {e}"
                )));
            }
        }
        Ok(missing)
    }

    /// 删除放行规则（archive 清理）。best-effort：缺失视为已清理。
    pub async fn remove_access(&self, spec: &DockerForwardAccessSpec) {
        for (name, args) in [
            ("raw PREROUTING", raw_delete_args(spec)),
            ("DOCKER-USER", user_delete_args(spec)),
        ] {
            match self.runner.run(IPTABLES_BIN, &args).await {
                Ok(out) if out.exit_code == 0 => {}
                Ok(out) => warn!(
                    "[DockerForward] remove {name} skipped (exit {}): {}",
                    out.exit_code, out.stderr
                ),
                Err(e) => warn!("[DockerForward] remove {name} error: {e}"),
            }
        }
    }
}

impl Default for DockerForwardRuntime {
    fn default() -> Self {
        Self::new()
    }
}

// ── 结构化 argv 构造（模块级自由函数）──

fn raw_check_args(spec: &DockerForwardAccessSpec) -> Vec<String> {
    vec![
        "-t".into(),
        "raw".into(),
        "-C".into(),
        "PREROUTING".into(),
        "-i".into(),
        spec.wg_interface.clone(),
        "-d".into(),
        spec.gamebox_cidr.clone(),
        "-j".into(),
        "ACCEPT".into(),
    ]
}

fn raw_insert_args(spec: &DockerForwardAccessSpec) -> Vec<String> {
    let mut v = raw_check_args(spec);
    // -C → -I PREROUTING 1（链首：先于 Docker 之后追加的反欺骗 drop）
    // ["-t","raw","-C","PREROUTING", ...] → ["-t","raw","-I","PREROUTING","1", ...]
    v[2] = "-I".into();
    v.insert(4, "1".into());
    v
}

fn raw_delete_args(spec: &DockerForwardAccessSpec) -> Vec<String> {
    let mut v = raw_check_args(spec);
    v[2] = "-D".into();
    v
}

fn user_check_args(spec: &DockerForwardAccessSpec) -> Vec<String> {
    vec![
        "-C".into(),
        "DOCKER-USER".into(),
        "-i".into(),
        spec.wg_interface.clone(),
        "-o".into(),
        spec.bridge_name.clone(),
        "-j".into(),
        "ACCEPT".into(),
    ]
}

fn user_insert_args(spec: &DockerForwardAccessSpec) -> Vec<String> {
    let mut v = user_check_args(spec);
    // ["-C","DOCKER-USER", ...] → ["-I","DOCKER-USER","1", ...]
    v[0] = "-I".into();
    v.insert(2, "1".into());
    v
}

fn user_delete_args(spec: &DockerForwardAccessSpec) -> Vec<String> {
    let mut v = user_check_args(spec);
    v[0] = "-D".into();
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::event::awd::system::command::{CommandOutput, RecordingCommandRunner};

    fn spec() -> DockerForwardAccessSpec {
        DockerForwardAccessSpec {
            wg_interface: "fawg_ab12cd34".into(),
            bridge_name: "fctfawdab12cd34".into(),
            gamebox_cidr: "10.4.0.0/16".into(),
        }
    }

    /// 模拟 iptables：-C 按 presets 判定存在性；-I/-D 记录。
    #[derive(Clone)]
    struct FakeIptables {
        /// -C 时判定为"已存在"的规则集（raw / user 各一）。
        raw_present: bool,
        user_present: bool,
        ops: std::sync::Arc<std::sync::Mutex<Vec<(String, Vec<String>)>>>,
    }

    #[async_trait]
    impl CommandRunner for FakeIptables {
        async fn run(&self, program: &str, args: &[String]) -> anyhow::Result<CommandOutput> {
            assert_eq!(program, "iptables");
            self.ops
                .lock()
                .unwrap()
                .push((program.into(), args.to_vec()));
            let is_raw = args.first().map(|s| s.as_str()) == Some("-t");
            let action = args
                .iter()
                .position(|a| a == "-C" || a == "-I" || a == "-D")
                .and_then(|i| args.get(i).cloned());
            let present = if is_raw {
                self.raw_present
            } else {
                self.user_present
            };
            match action.as_deref() {
                Some("-C") if present => Ok(CommandOutput {
                    exit_code: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                }),
                Some("-C") => Ok(CommandOutput {
                    exit_code: 1,
                    stdout: String::new(),
                    stderr: "Bad rule (does not exist)".into(),
                }),
                Some("-I" | "-D") => Ok(CommandOutput {
                    exit_code: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                }),
                _ => Ok(CommandOutput {
                    exit_code: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                }),
            }
        }
    }

    #[tokio::test]
    async fn ensure_inserts_both_rules_when_missing() {
        let fake = FakeIptables {
            raw_present: false,
            user_present: false,
            ops: Default::default(),
        };
        let rt = DockerForwardRuntime::with_runner(Arc::new(fake.clone()));
        rt.ensure_access(&spec()).await.expect("ensure ok");

        let ops = fake.ops.lock().unwrap().clone();
        // 4 条命令：raw -C → raw -I；user -C → user -I
        assert_eq!(ops.len(), 4);
        // 精确 argv 断言（防重复链名/错位：Phase 9.1 真实部署发现 "-I PREROUTING 1" 构造错误）
        let s = &spec();
        assert_eq!(
            ops[0].1,
            vec![
                "-t".to_string(),
                "raw".into(),
                "-C".into(),
                "PREROUTING".into(),
                "-i".into(),
                s.wg_interface.clone(),
                "-d".into(),
                s.gamebox_cidr.clone(),
                "-j".into(),
                "ACCEPT".into(),
            ]
        );
        assert_eq!(
            ops[1].1,
            vec![
                "-t".to_string(),
                "raw".into(),
                "-I".into(),
                "PREROUTING".into(),
                "1".into(),
                "-i".into(),
                s.wg_interface.clone(),
                "-d".into(),
                s.gamebox_cidr.clone(),
                "-j".into(),
                "ACCEPT".into(),
            ]
        );
        assert_eq!(
            ops[2].1,
            vec![
                "-C".into(),
                "DOCKER-USER".into(),
                "-i".into(),
                s.wg_interface.clone(),
                "-o".into(),
                s.bridge_name.clone(),
                "-j".into(),
                "ACCEPT".into(),
            ]
        );
        assert_eq!(
            ops[3].1,
            vec![
                "-I".into(),
                "DOCKER-USER".into(),
                "1".into(),
                "-i".into(),
                s.wg_interface.clone(),
                "-o".into(),
                s.bridge_name.clone(),
                "-j".into(),
                "ACCEPT".into(),
            ]
        );
    }

    #[tokio::test]
    async fn ensure_skips_when_already_present() {
        let fake = FakeIptables {
            raw_present: true,
            user_present: true,
            ops: Default::default(),
        };
        let rt = DockerForwardRuntime::with_runner(Arc::new(fake.clone()));
        rt.ensure_access(&spec()).await.expect("ensure ok");

        let ops = fake.ops.lock().unwrap().clone();
        // 只有两条 -C 检查，无插入
        assert_eq!(ops.len(), 2);
        assert!(ops.iter().all(|(_, a)| a.contains(&"-C".to_string())));
    }

    #[tokio::test]
    async fn check_reports_missing_rules() {
        let fake = FakeIptables {
            raw_present: false,
            user_present: true,
            ops: Default::default(),
        };
        let rt = DockerForwardRuntime::with_runner(Arc::new(fake.clone()));
        let missing = rt.check_access(&spec()).await.expect("check ok");
        assert_eq!(missing, vec!["raw PREROUTING ACCEPT"]);
    }

    #[tokio::test]
    async fn remove_issues_delete_for_both() {
        let fake = FakeIptables {
            raw_present: true,
            user_present: true,
            ops: Default::default(),
        };
        let rt = DockerForwardRuntime::with_runner(Arc::new(fake.clone()));
        rt.remove_access(&spec()).await;
        let ops = fake.ops.lock().unwrap().clone();
        assert_eq!(ops.len(), 2);
        assert!(ops.iter().all(|(_, a)| a.contains(&"-D".to_string())));
    }

    #[tokio::test]
    async fn args_never_contain_shell_metacharacters() {
        // 结构化 argv 铁律：规则参数不得引入 shell 拼接风险
        let fake = FakeIptables {
            raw_present: false,
            user_present: false,
            ops: Default::default(),
        };
        let rt = DockerForwardRuntime::with_runner(Arc::new(fake.clone()));
        rt.ensure_access(&spec()).await.expect("ok");
        let ops = fake.ops.lock().unwrap().clone();
        for (_, args) in ops {
            for a in args {
                assert!(!a.contains(';') && !a.contains('$') && !a.contains('`'));
            }
        }
    }

    #[tokio::test]
    async fn recording_runner_compat() {
        // RecordingCommandRunner 用于其他模块测试；此处验证命令序列可记录
        let recorder = Arc::new(RecordingCommandRunner::new());
        let rt = DockerForwardRuntime::with_runner(recorder.clone());
        let spec = spec();
        rt.ensure_access(&spec).await.expect("ensure ok");
        let cmds = recorder.recorded();
        assert!(!cmds.is_empty());
        assert!(cmds.iter().all(|(p, _)| p == "iptables"));
    }
}
