use std::collections::HashMap;

use fcmc::{ContainerRuntime, ContainerSpec, DockerContainerRuntime, NetworkSpec, ResourceLimits};
use uuid::Uuid;

/// Live regression for Docker embedded-DNS aliases on the primary network.
///
/// Run explicitly with:
/// `FLOATCTF_TEST_DOCKER_ALIAS=1 cargo test -p fcmc --test network_alias_live -- --ignored --nocapture`
#[tokio::test]
#[ignore = "requires a real Docker/helper socket"]
async fn primary_network_alias_survives_create_with_fixed_ip() -> anyhow::Result<()> {
    if std::env::var("FLOATCTF_TEST_DOCKER_ALIAS").ok().as_deref() != Some("1") {
        eprintln!("skip: FLOATCTF_TEST_DOCKER_ALIAS=1 not set");
        return Ok(());
    }

    let suffix = Uuid::new_v4().simple().to_string();
    let suffix = &suffix[..8];
    let network_name = format!("fctf-fcmc-alias-live-{suffix}");
    let container_name = format!("fctf-fcmc-alias-live-server-{suffix}");
    let client_name = format!("fctf-fcmc-alias-live-client-{suffix}");
    let alias = format!("alias-{suffix}");

    let (runtime, _) = DockerContainerRuntime::from_preferred().await?;
    runtime
        .create_network(NetworkSpec {
            name: network_name.clone(),
            subnet_cidr: "10.254.251.0/24".into(),
            bridge_name: Some(format!("fctf{suffix}")),
            internal: false,
            check_duplicate: true,
        })
        .await?;

    let result = async {
        runtime
            .create_and_start(ContainerSpec {
                name: container_name.clone(),
                image: "redis:7-alpine".into(),
                env: vec![],
                labels: HashMap::new(),
                network_name: Some(network_name.clone()),
                fixed_ip: Some("10.254.251.10".into()),
                network_aliases: vec![alias.clone()],
                port_bindings: vec![],
                auto_remove: false,
                resources: ResourceLimits::default(),
                network_mode: None,
                healthcheck: None,
            })
            .await?;

        let inspect = runtime
            .inner()
            .inspect_container(
                &container_name,
                None::<bollard::query_parameters::InspectContainerOptions>,
            )
            .await?;
        let endpoint = inspect
            .network_settings
            .and_then(|s| s.networks)
            .and_then(|mut networks| networks.remove(&network_name))
            .ok_or_else(|| anyhow::anyhow!("container missing primary network {network_name}"))?;
        let aliases = endpoint.aliases.unwrap_or_default();
        anyhow::ensure!(
            aliases.iter().any(|value| value == &alias),
            "expected alias {alias} missing from Docker inspect: {aliases:?}"
        );

        // Match AWDP GameBox creation: network_name is set but fixed_ip is absent.
        runtime
            .create_and_start(ContainerSpec {
                name: client_name.clone(),
                image: "redis:7-alpine".into(),
                env: vec![],
                labels: HashMap::new(),
                network_name: Some(network_name.clone()),
                fixed_ip: None,
                network_aliases: vec![],
                port_bindings: vec![],
                auto_remove: false,
                resources: ResourceLimits::default(),
                network_mode: None,
                healthcheck: None,
            })
            .await?;
        let outcome = runtime
            .exec(
                &client_name,
                fcmc::ExecOptions {
                    cmd: vec!["getent".into(), "hosts".into(), alias.clone()],
                    env: vec![],
                    workdir: None,
                    timeout: std::time::Duration::from_secs(5),
                    stdin: None,
                    stdout_limit: 16 * 1024,
                    stderr_limit: 16 * 1024,
                },
            )
            .await?;
        anyhow::ensure!(
            outcome.exit_code == Some(0),
            "client on network_name-only attachment cannot resolve {alias}: stdout={:?} stderr={:?}",
            outcome.stdout,
            outcome.stderr
        );
        Ok::<_, anyhow::Error>(())
    }
    .await;

    let _ = runtime
        .stop_and_remove(&client_name, fcmc::IMMEDIATE_STOP_TIMEOUT)
        .await;
    let _ = runtime
        .stop_and_remove(&container_name, fcmc::IMMEDIATE_STOP_TIMEOUT)
        .await;
    let _ = runtime.remove_network(&network_name).await;
    result
}
