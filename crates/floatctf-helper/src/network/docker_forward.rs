use anyhow::{Result, bail};
use helper_protocol::DockerForwardCheck;

use crate::{command::CmdOutput, command::run, validation};

fn raw_rule(action: &str, wg: &str, cidr: &str) -> Vec<String> {
    let mut args = vec![
        "-t".to_string(),
        "raw".to_string(),
        action.to_string(),
        "PREROUTING".to_string(),
    ];
    if action == "-I" {
        args.push("1".to_string());
    }
    args.extend([
        "-i".to_string(),
        wg.to_string(),
        "-d".to_string(),
        cidr.to_string(),
        "-j".to_string(),
        "ACCEPT".to_string(),
    ]);
    args
}

fn user_rule(action: &str, wg: &str, bridge: &str) -> Vec<String> {
    let mut args = vec![action.to_string(), "DOCKER-USER".to_string()];
    if action == "-I" {
        args.push("1".to_string());
    }
    args.extend([
        "-i".to_string(),
        wg.to_string(),
        "-o".to_string(),
        bridge.to_string(),
        "-j".to_string(),
        "ACCEPT".to_string(),
    ]);
    args
}

async fn run_owned_iptables(args: &[String]) -> Result<CmdOutput> {
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    run("iptables", &refs).await
}

pub(crate) async fn ensure(wg: &str, bridge: &str, cidr: &str) -> Result<()> {
    validation::docker_forward(wg, bridge, cidr)?;
    if run_owned_iptables(&raw_rule("-C", wg, cidr)).await?.code != 0 {
        let out = run_owned_iptables(&raw_rule("-I", wg, cidr)).await?;
        if out.code != 0 {
            bail!(
                "iptables raw insert failed (exit {}): {}",
                out.code,
                out.stderr.trim()
            );
        }
    }
    if run_owned_iptables(&user_rule("-C", wg, bridge)).await?.code != 0 {
        let out = run_owned_iptables(&user_rule("-I", wg, bridge)).await?;
        if out.code != 0 {
            bail!(
                "iptables DOCKER-USER insert failed (exit {}): {}",
                out.code,
                out.stderr.trim()
            );
        }
    }
    Ok(())
}

pub(crate) async fn check(wg: &str, bridge: &str, cidr: &str) -> Result<DockerForwardCheck> {
    validation::docker_forward(wg, bridge, cidr)?;
    let mut missing = Vec::new();
    if run_owned_iptables(&raw_rule("-C", wg, cidr)).await?.code != 0 {
        missing.push("raw PREROUTING ACCEPT".to_string());
    }
    if run_owned_iptables(&user_rule("-C", wg, bridge)).await?.code != 0 {
        missing.push("DOCKER-USER ACCEPT".to_string());
    }
    Ok(DockerForwardCheck { missing })
}

pub(crate) async fn remove(wg: &str, bridge: &str, cidr: &str) -> Result<()> {
    validation::docker_forward(wg, bridge, cidr)?;
    for args in [raw_rule("-D", wg, cidr), user_rule("-D", wg, bridge)] {
        let out = run_owned_iptables(&args).await?;
        if out.code != 0
            && !out.stderr.contains("does a matching rule exist")
            && !out.stderr.contains("Bad rule")
        {
            bail!(
                "iptables delete failed (exit {}): {}",
                out.code,
                out.stderr.trim()
            );
        }
    }
    Ok(())
}
