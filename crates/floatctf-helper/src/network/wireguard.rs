use std::io::Write as _;

use anyhow::{Context, Result};
use helper_protocol::EventNetworkObservation;

use crate::{command, validation};

pub(crate) async fn ensure(
    interface: &str,
    private_key: &str,
    listen_port: u16,
    address: &str,
) -> Result<()> {
    validation::interface(interface)?;
    validation::key(private_key, "private key")?;
    validation::cidr(address)?;

    let exists = command::run("ip", &["link", "show", interface]).await?.code == 0;
    if !exists {
        command::command_ok("ip", &["link", "add", interface, "type", "wireguard"]).await?;
    }

    let mut key_file = tempfile::NamedTempFile::new().context("create WireGuard key file")?;
    key_file
        .write_all(private_key.as_bytes())
        .context("write WireGuard key")?;
    key_file
        .write_all(b"\n")
        .context("write WireGuard newline")?;
    key_file.flush().context("flush WireGuard key")?;
    let key_path = key_file
        .path()
        .to_str()
        .context("WireGuard key path is not UTF-8")?;

    command::command_ok(
        "wg",
        &[
            "set",
            interface,
            "private-key",
            key_path,
            "listen-port",
            &listen_port.to_string(),
        ],
    )
    .await?;
    command::command_ok("ip", &["address", "replace", address, "dev", interface]).await?;
    command::command_ok("ip", &["link", "set", interface, "up"]).await?;
    Ok(())
}

pub(crate) async fn remove(interface: &str) -> Result<()> {
    validation::interface(interface)?;
    let exists = command::run("ip", &["link", "show", interface]).await?.code == 0;
    if exists {
        command::command_ok("ip", &["link", "delete", interface]).await?;
    }
    Ok(())
}

pub(crate) async fn add_peer(interface: &str, public_key: &str, allowed_ips: &str) -> Result<()> {
    validation::interface(interface)?;
    validation::key(public_key, "public key")?;
    validation::cidr(allowed_ips)?;
    command::command_ok(
        "wg",
        &[
            "set",
            interface,
            "peer",
            public_key,
            "allowed-ips",
            allowed_ips,
        ],
    )
    .await?;
    Ok(())
}

pub(crate) async fn remove_peer(interface: &str, public_key: &str) -> Result<()> {
    validation::interface(interface)?;
    validation::key(public_key, "public key")?;
    command::command_ok("wg", &["set", interface, "peer", public_key, "remove"]).await?;
    Ok(())
}

pub(crate) async fn inspect(interface: &str) -> Result<EventNetworkObservation> {
    validation::interface(interface)?;
    let mut observation = EventNetworkObservation::default();
    match command::run("ip", &["link", "show", interface]).await {
        Ok(out) if out.code == 0 => {
            let first = out.stdout.lines().next().unwrap_or("").trim().to_string();
            observation
                .notes
                .push(format!("ip link show {interface}: {first}"));
            observation.wireguard_interface_up =
                out.stdout.contains("LOWER_UP") || out.stdout.contains("state UP");
        }
        Ok(out) => observation.notes.push(format!(
            "ip link show {interface} failed (exit {}): {}",
            out.code,
            out.stderr.trim()
        )),
        Err(err) => observation
            .notes
            .push(format!("ip link show {interface} failed: {err:#}")),
    }

    match tokio::fs::read_to_string("/proc/sys/net/bridge/bridge-nf-call-iptables").await {
        Ok(value) => observation.notes.push(format!(
            "bridge-nf-call-iptables={} (same-bridge isolation {})",
            value.trim(),
            if value.trim() == "1" {
                "effective"
            } else {
                "NOT effective"
            }
        )),
        Err(_) => observation.notes.push(
            "bridge-nf-call-iptables unavailable (same-bridge isolation NOT effective)".to_string(),
        ),
    }
    Ok(observation)
}
