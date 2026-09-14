use anyhow::{Context, Result, bail};
use tokio::process::Command;

#[derive(Debug)]
pub(crate) struct CmdOutput {
    pub(crate) code: i32,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
}

pub(crate) async fn run(program: &str, args: &[&str]) -> Result<CmdOutput> {
    let output = Command::new(program)
        .args(args)
        .output()
        .await
        .with_context(|| format!("run {program}"))?;
    Ok(CmdOutput {
        code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

pub(crate) async fn command_ok(program: &str, args: &[&str]) -> Result<CmdOutput> {
    let output = run(program, args).await?;
    if output.code != 0 {
        bail!(
            "{} {:?} failed (exit {}): {}",
            program,
            args,
            output.code,
            output.stderr.trim()
        );
    }
    Ok(output)
}
