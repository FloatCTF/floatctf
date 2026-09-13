use std::{os::unix::fs::PermissionsExt, path::Path};

use anyhow::{Context, Result};
use helper_protocol::{PingData, Request, Response};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{UnixListener, UnixStream},
};

use crate::network::{conntrack, docker_forward, nftables, routes, wireguard};

const MAX_REQUEST_BYTES: u64 = 2 * 1024 * 1024;

pub(crate) async fn serve(socket_path: String) -> Result<()> {
    let socket = Path::new(&socket_path);
    if let Some(parent) = socket.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("create runtime directory {}", parent.display()))?;
    }
    if socket.exists() {
        tokio::fs::remove_file(socket)
            .await
            .with_context(|| format!("remove stale socket {}", socket.display()))?;
    }

    let listener = UnixListener::bind(socket)
        .with_context(|| format!("bind unix socket {}", socket.display()))?;
    std::fs::set_permissions(socket, std::fs::Permissions::from_mode(0o660))
        .with_context(|| format!("chmod unix socket {}", socket.display()))?;

    eprintln!("floatctf-helper control socket listening on {socket_path}");
    loop {
        let (stream, _) = listener.accept().await.context("accept unix connection")?;
        tokio::spawn(async move {
            if let Err(err) = handle_connection(stream).await {
                eprintln!("helper control request failed: {err:#}");
            }
        });
    }
}

async fn handle_connection(stream: UnixStream) -> Result<()> {
    let (read_half, mut write_half) = stream.into_split();
    let mut line = String::new();
    let mut reader = BufReader::new(read_half).take(MAX_REQUEST_BYTES);
    let read = reader.read_line(&mut line).await.context("read request")?;
    if read == 0 {
        return Ok(());
    }

    let response = match serde_json::from_str::<Request>(&line) {
        Ok(request) => match dispatch(request).await {
            Ok(response) => response,
            Err(err) => Response::failure(format!("{err:#}")),
        },
        Err(err) => Response::failure(format!("invalid request: {err}")),
    };

    let mut encoded = serde_json::to_vec(&response).context("encode response")?;
    encoded.push(b'\n');
    write_half
        .write_all(&encoded)
        .await
        .context("write response")?;
    Ok(())
}

async fn dispatch(request: Request) -> Result<Response> {
    match request {
        Request::Ping => Ok(Response::success(PingData {
            version: env!("CARGO_PKG_VERSION").to_string(),
        })),
        Request::EnsureWireguard {
            interface,
            private_key,
            listen_port,
            address,
        } => {
            wireguard::ensure(&interface, &private_key, listen_port, &address).await?;
            Ok(Response::empty())
        }
        Request::RemoveWireguard { interface } => {
            wireguard::remove(&interface).await?;
            Ok(Response::empty())
        }
        Request::AddWireguardPeer {
            interface,
            public_key,
            allowed_ips,
        } => {
            wireguard::add_peer(&interface, &public_key, &allowed_ips).await?;
            Ok(Response::empty())
        }
        Request::RemoveWireguardPeer {
            interface,
            public_key,
        } => {
            wireguard::remove_peer(&interface, &public_key).await?;
            Ok(Response::empty())
        }
        Request::FlushConntrack { cidr } => {
            conntrack::flush(&cidr).await?;
            Ok(Response::empty())
        }
        Request::InspectEventNetwork { interface } => {
            Ok(Response::success(wireguard::inspect(&interface).await?))
        }
        Request::ListHostRouteCidrs => {
            Ok(Response::success(routes::list_host_route_cidrs().await?))
        }
        Request::ListNftTable { table } => {
            Ok(Response::success(nftables::list_table(&table).await?))
        }
        Request::ApplyNftTable {
            table,
            ruleset,
            ensure_bridge_netfilter,
        } => {
            nftables::apply(&table, &ruleset, ensure_bridge_netfilter).await?;
            Ok(Response::empty())
        }
        Request::DeleteNftTable { table } => {
            nftables::delete_table(&table).await?;
            Ok(Response::empty())
        }
        Request::EnsureDockerForward {
            wg_interface,
            bridge_name,
            gamebox_cidr,
        } => {
            docker_forward::ensure(&wg_interface, &bridge_name, &gamebox_cidr).await?;
            Ok(Response::empty())
        }
        Request::CheckDockerForward {
            wg_interface,
            bridge_name,
            gamebox_cidr,
        } => Ok(Response::success(
            docker_forward::check(&wg_interface, &bridge_name, &gamebox_cidr).await?,
        )),
        Request::RemoveDockerForward {
            wg_interface,
            bridge_name,
            gamebox_cidr,
        } => {
            docker_forward::remove(&wg_interface, &bridge_name, &gamebox_cidr).await?;
            Ok(Response::empty())
        }
    }
}
