//! 回归：GameBox 健康检查探针必须绕过宿主 HTTP 代理。
//!
//! reqwest 默认继承 `HTTP(S)_PROXY` / `ALL_PROXY`。探针目标是 Docker 内网 IP，
//! 宿主一旦设置代理（国内开发机常见：cargo/docker 构建需要代理），请求就会被
//! 送到代理由代理转发；代理无法回连 Docker 内网 → 探针恒失败 → 实例永远判为
//! 未就绪。本测试用「死代理 + 空 NO_PROXY」证明探针客户端显式关闭了系统代理继承。
//!
//! 测试自带对照：先用一个未关闭代理继承的 reqwest Client 断言请求确实被劫持，
//! 再断言 `probe_one` 直连成功——因此在无代理环境下同样是有效回归。
//! 进程内注入的环境变量由 `EnvGuard` 在退出（含 panic）时还原。

use std::ffi::OsString;
use std::time::Duration;

use floatctf::modules::gamebox::healthcheck::{AppHealthcheck, probe_one};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// 注入受控环境变量，并在 Drop 时还原原值（含 panic 展开路径）。
struct EnvGuard(Vec<(&'static str, Option<OsString>)>);

impl EnvGuard {
    fn apply(pairs: &[(&'static str, &str)]) -> Self {
        let mut saved = Vec::with_capacity(pairs.len());
        for (key, value) in pairs {
            saved.push((*key, std::env::var_os(key)));
            // SAFETY: 本文件只包含这一个测试，进程内无并发读写环境变量。
            unsafe { std::env::set_var(key, value) };
        }
        Self(saved)
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (key, previous) in &self.0 {
            match previous {
                // SAFETY: 同上。
                Some(value) => unsafe { std::env::set_var(key, value) },
                // SAFETY: 同上。
                None => unsafe { std::env::remove_var(key) },
            }
        }
    }
}

/// 起一个最小 HTTP 服务（固定返回 200），返回端口与任务句柄。
async fn spawn_ok_server() -> (u16, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let handle = tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                return;
            };
            tokio::spawn(async move {
                let mut buf = [0u8; 1024];
                let _ = sock.read(&mut buf).await;
                let _ = sock
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
                    )
                    .await;
                let _ = sock.shutdown().await;
            });
        }
    });
    (port, handle)
}

#[tokio::test]
async fn http_probe_bypasses_host_proxy() {
    // 死代理：任何走代理的请求都会立刻连接失败；清空 NO_PROXY 排除列表，
    // 让 reqwest 的默认行为（继承系统代理）暴露出来。
    let _env = EnvGuard::apply(&[
        ("HTTP_PROXY", "http://127.0.0.1:1"),
        ("http_proxy", "http://127.0.0.1:1"),
        ("ALL_PROXY", "http://127.0.0.1:1"),
        ("all_proxy", "http://127.0.0.1:1"),
        ("NO_PROXY", ""),
        ("no_proxy", ""),
    ]);

    let (port, _server) = spawn_ok_server().await;
    let url = format!("http://127.0.0.1:{port}/healthz");

    // 对照：未关闭代理继承的客户端确实会被劫持到死代理。
    let hijacked = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap()
        .get(&url)
        .send()
        .await;
    assert!(
        hijacked.is_err(),
        "对照失败：环境代理未生效，本测试无法证明探针绕过了代理"
    );

    // 探针：必须直连成功。
    let check = AppHealthcheck::Http {
        port,
        path: "/healthz".to_string(),
        expected_status: 200,
    };
    let result = probe_one("127.0.0.1", &check, Duration::from_secs(3)).await;
    assert!(
        result.ok,
        "健康检查被宿主代理劫持（应显式 no_proxy 直连容器内网）：{}",
        result.detail
    );
}
