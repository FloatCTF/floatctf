//! Centralized system command execution layer.
//!
//! # Safety
//!
//! This module is the ONLY place where external system commands may be
//! executed. All commands use structured argument lists — shell string
//! concatenation is forbidden.

use std::process::Output;
use tokio::process::Command;

#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// Abstraction over system command execution (testable / mockable).
#[async_trait::async_trait]
pub trait CommandRunner: Send + Sync {
    async fn run(&self, program: &str, args: &[String]) -> anyhow::Result<CommandOutput>;
}

/// Real command runner that executes commands on the host.
pub struct RealCommandRunner;

#[async_trait::async_trait]
impl CommandRunner for RealCommandRunner {
    async fn run(&self, program: &str, args: &[String]) -> anyhow::Result<CommandOutput> {
        let output: Output = Command::new(program).args(args).output().await?;
        Ok(CommandOutput {
            exit_code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    }
}

/// Recording command runner for testing — records commands without executing them.
#[derive(Default)]
pub struct RecordingCommandRunner {
    pub commands: std::sync::Mutex<Vec<(String, Vec<String>)>>,
}

impl RecordingCommandRunner {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn recorded(&self) -> Vec<(String, Vec<String>)> {
        self.commands.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl CommandRunner for RecordingCommandRunner {
    async fn run(&self, program: &str, args: &[String]) -> anyhow::Result<CommandOutput> {
        self.commands
            .lock()
            .unwrap()
            .push((program.to_string(), args.to_vec()));
        Ok(CommandOutput {
            exit_code: 0,
            stdout: String::new(),
            stderr: String::new(),
        })
    }
}

// ── Safe wrappers for specific system commands ──

/// Build `wg` command arguments safely.
pub mod wireguard_cmd {
    use super::*;

    pub async fn genkey(runner: &dyn CommandRunner) -> anyhow::Result<String> {
        let out = runner.run("wg", &["genkey".to_string()]).await?;
        Ok(out.stdout.trim().to_string())
    }

    pub async fn pubkey(runner: &dyn CommandRunner, private_key: &str) -> anyhow::Result<String> {
        let out = runner.run("wg", &["pubkey".to_string()]).await?;
        // pubkey reads private key from stdin — handled by piping
        // For MVP, we generate via crypto module instead
        Ok(out.stdout.trim().to_string())
    }

    pub async fn set_interface(
        runner: &dyn CommandRunner,
        iface: &str,
        private_key: &str,
        listen_port: u16,
    ) -> anyhow::Result<()> {
        let args = vec![
            "set".to_string(),
            iface.to_string(),
            "private-key".to_string(),
            "/dev/stdin".to_string(),
            "listen-port".to_string(),
            listen_port.to_string(),
        ];
        runner.run("wg", &args).await?;
        Ok(())
    }

    pub async fn add_peer(
        runner: &dyn CommandRunner,
        iface: &str,
        public_key: &str,
        allowed_ips: &str,
    ) -> anyhow::Result<()> {
        let args = vec![
            "set".to_string(),
            iface.to_string(),
            "peer".to_string(),
            public_key.to_string(),
            "allowed-ips".to_string(),
            allowed_ips.to_string(),
        ];
        runner.run("wg", &args).await?;
        Ok(())
    }

    pub async fn remove_peer(
        runner: &dyn CommandRunner,
        iface: &str,
        public_key: &str,
    ) -> anyhow::Result<()> {
        let args = vec![
            "set".to_string(),
            iface.to_string(),
            "peer".to_string(),
            public_key.to_string(),
            "remove".to_string(),
        ];
        runner.run("wg", &args).await?;
        Ok(())
    }
}

/// Build `iptables` / `iptables-restore` command arguments safely.
pub mod firewall_cmd {
    use super::*;

    pub async fn apply_rules(runner: &dyn CommandRunner, rules: &str) -> anyhow::Result<()> {
        // Write rules to temp file, then iptables-restore
        let tmp = tempfile::NamedTempFile::new()?;
        std::fs::write(tmp.path(), rules)?;
        let args = vec![
            "iptables-restore".to_string(),
            "--noflush".to_string(),
            tmp.path().to_string_lossy().to_string(),
        ];
        // Actually use iptables-restore as the command
        runner.run("iptables-restore", &args[1..]).await?;
        Ok(())
    }

    pub async fn save_snapshot(
        runner: &dyn CommandRunner,
        chain_prefix: &str,
    ) -> anyhow::Result<String> {
        let out = runner
            .run("iptables-save", &["-t".to_string(), "filter".to_string()])
            .await?;
        // Filter lines matching chain_prefix
        let filtered: Vec<&str> = out
            .stdout
            .lines()
            .filter(|l| l.contains(chain_prefix))
            .collect();
        Ok(filtered.join("\n"))
    }
}

/// Build `conntrack` command arguments safely.
pub mod conntrack_cmd {
    use super::*;

    pub async fn flush_event(runner: &dyn CommandRunner, cidr: &str) -> anyhow::Result<()> {
        let args = vec!["-D".to_string(), "-s".to_string(), cidr.to_string()];
        runner.run("conntrack", &args).await?;
        Ok(())
    }
}
