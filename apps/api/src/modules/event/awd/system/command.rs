//! 安全构造系统命令（结构化参数，禁止 shell 拼接）。

use std::process::Output;
use tokio::process::Command;

#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// 系统命令执行抽象（可测试 / 可 mock）。
#[async_trait::async_trait]
pub trait CommandRunner: Send + Sync {
    async fn run(&self, program: &str, args: &[String]) -> anyhow::Result<CommandOutput>;
}

/// 在宿主上真实执行命令的运行器。
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

/// 测试用记录型命令运行器——只记录不执行。
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

/// 构建`wg` command arguments safely。
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
        // CommandRunner 不提供 stdin：私钥必须经临时文件传入（/dev/stdin 读到 EOF，
        // 接口永远拿不到私钥 → 玩家无法握手，真实主机实测）。文件在 run() 期间
        // 由本作用域持有的 NamedTempFile 保证存在。
        use std::io::Write;
        let mut key_file = tempfile::NamedTempFile::new()?;
        key_file.write_all(private_key.as_bytes())?;
        key_file.write_all(b"\n")?;
        key_file.flush()?;
        let key_path = key_file
            .path()
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("WG key path not utf8"))?
            .to_string();
        let args = vec![
            "set".to_string(),
            iface.to_string(),
            "private-key".to_string(),
            key_path,
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

/// 构建`conntrack` command arguments safely。
pub mod conntrack_cmd {
    use super::*;

    pub async fn flush_event(runner: &dyn CommandRunner, cidr: &str) -> anyhow::Result<()> {
        let args = vec!["-D".to_string(), "-s".to_string(), cidr.to_string()];
        runner.run("conntrack", &args).await?;
        Ok(())
    }
}
