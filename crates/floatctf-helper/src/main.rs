mod command;
mod control;
mod docker_proxy;
mod network;
mod validation;

use anyhow::Result;
use helper_protocol::{DEFAULT_CONTROL_SOCKET_PATH, DEFAULT_DOCKER_SOCKET_PATH};

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let socket_path = arg_value(&args, "--control-socket")
        .or_else(|| arg_value(&args, "--socket"))
        .unwrap_or_else(|| DEFAULT_CONTROL_SOCKET_PATH.to_string());
    let docker_socket_path = arg_value(&args, "--docker-socket")
        .unwrap_or_else(|| DEFAULT_DOCKER_SOCKET_PATH.to_string());
    let docker_backend =
        arg_value(&args, "--docker-backend").unwrap_or_else(|| "/var/run/docker.sock".to_string());

    let control = control::serve(socket_path.clone());
    let docker = docker_proxy::serve(&docker_socket_path, &docker_backend);
    tokio::pin!(control);
    tokio::pin!(docker);

    tokio::select! {
        result = &mut control => result?,
        result = &mut docker => result?,
        _ = tokio::signal::ctrl_c() => {}
    }

    let _ = tokio::fs::remove_file(&socket_path).await;
    let _ = tokio::fs::remove_file(&docker_socket_path).await;
    Ok(())
}

fn arg_value(args: &[String], key: &str) -> Option<String> {
    args.iter()
        .position(|arg| arg == key)
        .and_then(|index| args.get(index + 1))
        .cloned()
}
