use std::process::Stdio;

use actix_web::{HttpRequest, HttpResponse, web};
use actix_ws::Message;
use futures_util::StreamExt;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::api::prelude::*;
use crate::core::security::jwt::validate_jwt;
use crate::entity::super_admin;

/// GET /api/admin/terminal/ws
#[get("/ws")]
pub async fn terminal_ws(
    req: HttpRequest,
    stream: web::Payload,
    query: web::Query<std::collections::HashMap<String, String>>,
) -> Result<HttpResponse, actix_web::Error> {
    // Validate JWT from query param
    let token = query.get("token").cloned().unwrap_or_default();
    let db = req.app_data::<WebDb>().cloned().expect("WebDb not found");

    let claims = validate_jwt(token)
        .map_err(|_| actix_web::error::ErrorUnauthorized("Invalid or missing token"))?;

    let _admin = super_admin::Entity::find_by_id(claims.sub)
        .one(db.get_ref())
        .await
        .map_err(|_| actix_web::error::ErrorInternalServerError("DB error"))?
        .ok_or_else(|| actix_web::error::ErrorUnauthorized("Not a super admin"))?;

    // Upgrade to WebSocket
    let (res, mut session, mut msg_stream) = actix_ws::handle(&req, stream)?;

    // Check if web terminal is enabled
    let enable_web_terminal = std::env::var("ENABLE_WEB_TERMINAL")
        .unwrap_or_else(|_| "0".to_string())
        .parse::<i32>()
        .unwrap_or(0);

    if enable_web_terminal != 1 {
        let _ = session
            .text("\r\n\x1b[31m[ERROR] Web terminal is disabled. Please set ENABLE_WEB_TERMINAL=1 in .env and restart.\x1b[0m\r\n")
            .await;
        let _ = session.close(None).await;
        return Ok(res);
    }

    // Find best available shell: fish > zsh > bash > sh
    let (shell, login_arg) = find_shell();

    // Spawn shell inside script for PTY support
    let mut cmd = if cfg!(target_os = "macos") {
        let mut c = Command::new("script");
        c.arg("-q").arg("/dev/null").arg(shell);
        if let Some(flag) = login_arg {
            c.arg(flag);
        }
        c
    } else {
        let mut c = Command::new("script");
        let shell_cmd = if let Some(flag) = login_arg {
            format!("{shell} {flag}")
        } else {
            shell.to_string()
        };
        c.arg("-q").arg("-c").arg(shell_cmd).arg("/dev/null");
        c
    };

    // Set TERM so programs know how to render
    cmd.env("TERM", "xterm-256color");
    // Disable pagination
    cmd.env("PAGER", "cat");
    cmd.env("MANPAGER", "cat");
    // Merge stderr into stdout for simplicity
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    cmd.stdin(Stdio::piped());
    cmd.kill_on_drop(true);

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            let _ = session
                .text(format!("Failed to spawn shell: {e}\r\n"))
                .await;
            let _ = session.close(None).await;
            return Ok(res);
        }
    };

    let mut child_stdin = child.stdin.take().unwrap();
    let child_stdout = child.stdout.take().unwrap();
    let child_stderr = child.stderr.take().unwrap();

    // Read stdout and forward to WebSocket
    let mut session_out = session.clone();
    let stdout_handle = actix_web::rt::spawn(async move {
        use tokio::io::AsyncReadExt;
        let mut reader = tokio::io::BufReader::new(child_stdout);
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf).await {
                Ok(0) => break, // EOF
                Ok(n) => {
                    let data = Vec::from(&buf[..n]);
                    if session_out.binary(data).await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    // Read stderr and forward to WebSocket
    let mut session_err = session.clone();
    let stderr_handle = actix_web::rt::spawn(async move {
        use tokio::io::AsyncReadExt;
        let mut reader = tokio::io::BufReader::new(child_stderr);
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    let data = Vec::from(&buf[..n]);
                    if session_err.binary(data).await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    // Clone session for the msg processing task (needed for pong)
    let mut session_msg = session.clone();

    // Process incoming WebSocket messages
    actix_web::rt::spawn(async move {
        while let Some(Ok(msg)) = msg_stream.next().await {
            match msg {
                Message::Text(text) => {
                    // Check if this is a resize message
                    if let Ok(ctrl) = serde_json::from_str::<TerminalControl>(&text) {
                        if ctrl.r#type == "resize" {
                            let rows = ctrl.rows.unwrap_or(24);
                            let cols = ctrl.cols.unwrap_or(80);
                            // Clear screen before stty so the echoed command is wiped
                            let cmd = format!(
                                "printf '\\033[2J\\033[H'; stty rows {} cols {} 2>/dev/null\r",
                                rows, cols
                            );
                            let _ = child_stdin.write_all(cmd.as_bytes()).await;
                            continue;
                        }
                    }
                    // Regular text input - forward to bash stdin
                    let _ = child_stdin.write_all(text.as_bytes()).await;
                }
                Message::Binary(bin) => {
                    let _ = child_stdin.write_all(&bin).await;
                }
                Message::Ping(bytes) => {
                    let _ = session_msg.pong(&bytes).await;
                }
                Message::Close(_) => {
                    break;
                }
                _ => {}
            }
        }

        // WebSocket closed, terminate the child process
        child.kill().await.ok();
    });

    // Cleanup: when stdout or stderr readers end, close the session
    actix_web::rt::spawn(async move {
        let _ = tokio::join!(stdout_handle, stderr_handle);
        let _ = session.close(None).await;
    });

    Ok(res)
}

/// Try shells in order: fish > zsh > bash > sh.
/// Returns (shell_name, optional_login_flag).
fn find_shell() -> (&'static str, Option<&'static str>) {
    let candidates: &[(&str, Option<&str>)] = &[
        ("fish", Some("--login")),
        ("zsh", Some("--login")),
        ("bash", Some("--login")),
        ("sh", None),
    ];
    for &(shell, login) in candidates {
        if std::process::Command::new(shell)
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            return (shell, login);
        }
    }
    ("sh", None)
}

#[derive(Debug, Deserialize)]
struct TerminalControl {
    #[serde(rename = "type")]
    r#type: String,
    #[serde(default)]
    cols: Option<u16>,
    #[serde(default)]
    rows: Option<u16>,
}
