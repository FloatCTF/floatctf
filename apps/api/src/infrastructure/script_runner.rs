//! 宿主侧受控脚本执行原语（共享基础设施）。
//!
//! 从 awd-judgeserver 的「落盘 → chmod → 子进程 → 清理」模式提取，
//! 供 AWD Judge / AWDP Judge / AWDP Exploit 复用：
//!   - temp 脚本文件（受控权限）；
//!   - env_clear + 白名单；
//!   - 超时 + stdout/stderr 上限；
//!   - 无论成败删除临时脚本。

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::time::timeout;

/// 受控脚本执行结果。
#[derive(Debug, Clone)]
pub struct ScriptOutcome {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
    pub timed_out: bool,
}

/// 执行脚本：`interpreter script_path script_args...`（如 `python3 <temp>/run_script ip1 ip2`）。
///
/// 环境白名单：PATH / HOME / LANG / PYTHONIOENCODING / 传入的 `env`（如 FLOATCTF_*）。
pub async fn run_script(
    script_content: &str,
    interpreter: &str,
    script_args: &[String],
    env: &[String],
    timeout_secs: Duration,
    stdout_limit: usize,
    stderr_limit: usize,
) -> Result<ScriptOutcome, String> {
    let dir = tempfile::tempdir().map_err(|e| format!("tempdir for script: {e}"))?;
    let script_path: PathBuf = dir.path().join("run_script");
    std::fs::write(&script_path, script_content).map_err(|e| format!("write script: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| format!("chmod script: {e}"))?;
    }

    let mut cmd = Command::new(interpreter);
    cmd.arg(&script_path)
        .args(script_args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("HOME", std::env::var("HOME").unwrap_or("/root".into()))
        .env("LANG", "C.UTF-8")
        .env("PYTHONIOENCODING", "utf-8");
    for kv in env {
        if let Some((k, v)) = kv.split_once('=') {
            cmd.env(k, v);
        }
    }

    let started = std::time::Instant::now();
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("spawn script process: {e}"))?;

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut timed_out = false;

    if let Some(mut so) = child.stdout.take() {
        let mut buf = [0u8; 4096];
        loop {
            let n = match timeout(timeout_secs, so.read(&mut buf)).await {
                Ok(Ok(0)) => break,
                Ok(Ok(n)) => n,
                Ok(Err(e)) => {
                    stderr.extend_from_slice(format!("[read stdout: {e}]").as_bytes());
                    break;
                }
                Err(_) => {
                    timed_out = true;
                    break;
                }
            };
            if stdout.len() < stdout_limit {
                let take = n.min(stdout_limit - stdout.len());
                stdout.extend_from_slice(&buf[..take]);
            }
        }
    }
    if let Some(mut se) = child.stderr.take() {
        let mut buf = [0u8; 4096];
        loop {
            let n = match timeout(timeout_secs, se.read(&mut buf)).await {
                Ok(Ok(0)) => break,
                Ok(Ok(n)) => n,
                Ok(Err(e)) => {
                    stderr.extend_from_slice(format!("[read stderr: {e}]").as_bytes());
                    break;
                }
                Err(_) => {
                    timed_out = true;
                    break;
                }
            };
            if stderr.len() < stderr_limit {
                let take = n.min(stderr_limit - stderr.len());
                stderr.extend_from_slice(&buf[..take]);
            }
        }
    }

    let exit_code = if timed_out {
        let _ = child.kill().await;
        child.wait().await.map(|s| s.code()).unwrap_or(None)
    } else {
        match timeout(timeout_secs, child.wait()).await {
            Ok(Ok(status)) => status.code(),
            Ok(Err(e)) => {
                stderr.extend_from_slice(format!("[wait: {e}]").as_bytes());
                None
            }
            Err(_) => {
                let _ = child.kill().await;
                timed_out = true;
                child.wait().await.map(|s| s.code()).unwrap_or(None)
            }
        }
    };

    Ok(ScriptOutcome {
        exit_code,
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
        duration_ms: started.elapsed().as_millis() as u64,
        timed_out,
    })
}

/// 解析批量脚本 stdout JSON：`[{"success": bool, "gamebox_ip": "..."}]`。
/// 按 target_ip 返回逐盒结果（plan §29：不允许只看 exit code）。
pub fn parse_batch_results(stdout: &str) -> Result<Vec<BatchTargetResult>, String> {
    let value: serde_json::Value =
        serde_json::from_str(stdout).map_err(|e| format!("script stdout not JSON: {e}"))?;
    let arr = value
        .as_array()
        .ok_or_else(|| "script stdout must be a JSON array".to_string())?;
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        let ip = item
            .get("gamebox_ip")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let success = item
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let error = item
            .get("error")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        out.push(BatchTargetResult { ip, success, error });
    }
    Ok(out)
}

#[derive(Debug, Clone)]
pub struct BatchTargetResult {
    pub ip: String,
    pub success: bool,
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_batch_json() {
        let out = r#"[{"success": true, "gamebox_ip": "10.0.0.1"}, {"success": false, "gamebox_ip": "10.0.0.2", "error": "boom"}]"#;
        let rows = parse_batch_results(out).unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows[0].success);
        assert!(!rows[1].success);
        assert_eq!(rows[1].error.as_deref(), Some("boom"));
    }

    #[test]
    fn parse_batch_rejects_non_array() {
        assert!(parse_batch_results("{\"success\": true}").is_err());
        assert!(parse_batch_results("not json").is_err());
    }

    #[tokio::test]
    async fn runs_script_with_args_and_env() {
        let out = run_script(
            "import os, sys\nprint(os.environ.get('FLOATCTF_TEST', 'missing'))\nsys.exit(0)",
            "python3",
            &[],
            &["FLOATCTF_TEST=hello".into()],
            Duration::from_secs(10),
            4096,
            4096,
        )
        .await
        .unwrap();
        assert_eq!(out.exit_code, Some(0));
        assert!(out.stdout.contains("hello"), "stdout={}", out.stdout);
    }

    #[tokio::test]
    async fn nonzero_exit_propagates() {
        let out = run_script(
            "import sys\nsys.exit(3)",
            "python3",
            &[],
            &[],
            Duration::from_secs(10),
            4096,
            4096,
        )
        .await
        .unwrap();
        assert_eq!(out.exit_code, Some(3));
    }
}
