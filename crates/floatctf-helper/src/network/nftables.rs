use std::io::Write as _;

use anyhow::{Context, Result, bail};

use crate::{command, validation};

pub(crate) async fn list_table(table: &str) -> Result<String> {
    validation::nft_table(table)?;
    let out = command::run("nft", &["list", "table", "inet", table]).await?;
    if out.code == 0 {
        return Ok(out.stdout);
    }
    if out.stderr.contains("No such file") || out.stderr.contains("does not exist") {
        return Ok(String::new());
    }
    bail!(
        "nft list table failed (exit {}): {}",
        out.code,
        out.stderr.trim()
    )
}

async fn ensure_bridge_nf() -> Result<()> {
    for path in [
        "/proc/sys/net/bridge/bridge-nf-call-iptables",
        "/proc/sys/net/bridge/bridge-nf-call-ip6tables",
    ] {
        let value = tokio::fs::read_to_string(path).await.with_context(|| {
            format!("read {path}; run `mise run setup` / production installer first")
        })?;
        if value.trim() != "1" {
            bail!("{path} must be 1; run host setup before starting FloatCTF");
        }
    }
    Ok(())
}

pub(crate) async fn apply(table: &str, ruleset: &str, ensure_bridge_netfilter: bool) -> Result<()> {
    validation::nft_table(table)?;
    validation::nft_ruleset(table, ruleset)?;
    if ensure_bridge_netfilter {
        ensure_bridge_nf().await?;
    }

    let exists = !list_table(table).await?.trim().is_empty();
    let transaction = if exists {
        format!("delete table inet {table}\n{ruleset}")
    } else {
        ruleset.to_string()
    };
    let mut file = tempfile::NamedTempFile::new().context("create nft temp file")?;
    file.write_all(transaction.as_bytes())
        .context("write nft ruleset")?;
    file.flush().context("flush nft ruleset")?;
    let path = file.path().to_str().context("nft temp path is not UTF-8")?;
    command::command_ok("nft", &["-c", "-f", path]).await?;
    command::command_ok("nft", &["-f", path]).await?;
    Ok(())
}

pub(crate) async fn delete_table(table: &str) -> Result<()> {
    validation::nft_table(table)?;
    if list_table(table).await?.trim().is_empty() {
        return Ok(());
    }
    command::command_ok("nft", &["delete", "table", "inet", table]).await?;
    Ok(())
}
