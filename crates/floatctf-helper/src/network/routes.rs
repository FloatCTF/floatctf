use anyhow::Result;
use ipnet::Ipv4Net;

use crate::command;

pub(crate) async fn list_host_route_cidrs() -> Result<Vec<String>> {
    let out = command::command_ok("ip", &["-o", "route", "show"]).await?;
    let mut cidrs = Vec::new();
    for line in out.stdout.lines().map(str::trim) {
        if line.is_empty() || line.starts_with("default") {
            continue;
        }
        let dst = line.split_whitespace().next().unwrap_or("");
        if dst.contains('/') && !dst.contains(':') && dst.parse::<Ipv4Net>().is_ok() {
            cidrs.push(dst.to_string());
        }
    }
    cidrs.sort();
    cidrs.dedup();
    Ok(cidrs)
}
