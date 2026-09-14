use anyhow::{Context, Result, bail};
use ipnet::Ipv4Net;

fn validate_simple_name(value: &str, what: &str, max_len: usize) -> Result<()> {
    if value.is_empty()
        || value.len() > max_len
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
    {
        bail!("invalid {what}: {value:?}");
    }
    Ok(())
}

pub(crate) fn interface(interface: &str) -> Result<()> {
    validate_simple_name(interface, "WireGuard interface", 15)?;
    let suffix = interface
        .strip_prefix("fawg_")
        .ok_or_else(|| anyhow::anyhow!("refusing non-FloatCTF WireGuard interface: {interface}"))?;
    if suffix.len() != 8
        || !suffix
            .bytes()
            .all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f'))
    {
        bail!("invalid FloatCTF WireGuard interface: {interface}");
    }
    Ok(())
}

pub(crate) fn bridge(bridge: &str) -> Result<()> {
    validate_simple_name(bridge, "bridge interface", 15)?;
    let suffix = bridge
        .strip_prefix("fctfawd")
        .ok_or_else(|| anyhow::anyhow!("refusing non-FloatCTF bridge interface: {bridge}"))?;
    if suffix.len() != 8
        || !suffix
            .bytes()
            .all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f'))
    {
        bail!("invalid FloatCTF bridge interface: {bridge}");
    }
    Ok(())
}

pub(crate) fn key(key: &str, what: &str) -> Result<()> {
    if key.is_empty() || key.len() > 128 || key.chars().any(char::is_whitespace) {
        bail!("invalid {what}");
    }
    Ok(())
}

pub(crate) fn cidr(cidr: &str) -> Result<()> {
    cidr.parse::<Ipv4Net>()
        .with_context(|| format!("invalid IPv4 CIDR: {cidr}"))?;
    Ok(())
}

pub(crate) fn nft_table(table: &str) -> Result<()> {
    validate_simple_name(table, "nftables table", 64)?;
    if table != "floatctf_awd" && !table.starts_with("floatctf_awdp_") {
        bail!("refusing non-FloatCTF nftables table: {table}");
    }
    Ok(())
}

pub(crate) fn nft_ruleset(table: &str, ruleset: &str) -> Result<()> {
    nft_table(table)?;
    if ruleset.len() > 1024 * 1024 {
        bail!("nftables ruleset too large");
    }

    // ApplyNftTable 只接受 API renderer 生成的 declarative table body。拒绝 nft 的
    // command/directive 语法，避免通过同一个 `nft -f` batch 越权修改其他宿主对象。
    const FORBIDDEN_PREFIXES: &[&str] = &[
        "add ",
        "delete ",
        "destroy ",
        "flush ",
        "insert ",
        "replace ",
        "rename ",
        "reset ",
        "include ",
        "define ",
        "redefine ",
        "undefine ",
    ];
    for line in ruleset.lines() {
        // Whole-line comments are inert nft syntax. Skip them before splitting on
        // semicolons so punctuation in human-readable comments cannot be mistaken
        // for a second nft statement. Inline non-comment statements are still
        // inspected segment-by-segment below.
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        for (segment_index, segment) in line.split(';').enumerate() {
            let segment = segment.trim();
            if segment.is_empty() {
                continue;
            }
            if FORBIDDEN_PREFIXES
                .iter()
                .any(|prefix| segment.starts_with(prefix))
            {
                bail!("nftables command/directive is forbidden: {segment}");
            }
            // 首段允许 renderer 的常规声明/规则；分号后的合法 renderer 片段目前只有
            // `policy ...`。其余内容 fail-closed，防止 `...; add/delete/...` 的内联逃逸。
            if segment_index > 0 && !segment.starts_with("policy ") {
                bail!("unexpected nftables statement after semicolon: {segment}");
            }
        }
    }

    let declarations: Vec<_> = ruleset
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("table "))
        .collect();
    if declarations.len() != 1 {
        bail!("ruleset must declare exactly one nftables table");
    }
    let mut parts = declarations[0].split_whitespace();
    if parts.next() != Some("table")
        || parts.next() != Some("inet")
        || parts.next() != Some(table)
        || parts.next() != Some("{")
        || parts.next().is_some()
    {
        bail!("ruleset must declare exactly table inet {table}");
    }
    Ok(())
}

pub(crate) fn docker_forward(
    wg_interface: &str,
    bridge_name: &str,
    gamebox_cidr: &str,
) -> Result<()> {
    interface(wg_interface)?;
    bridge(bridge_name)?;
    cidr(gamebox_cidr)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_floatctf_tables_are_allowed() {
        assert!(nft_table("floatctf_awd").is_ok());
        assert!(nft_table("floatctf_awdp_practice").is_ok());
        assert!(nft_table("docker").is_err());
    }

    #[test]
    fn only_floatctf_wireguard_interfaces_are_allowed() {
        assert!(interface("fawg_1234abcd").is_ok());
        assert!(interface("wg0").is_err());
    }

    #[test]
    fn only_floatctf_event_bridges_are_allowed() {
        assert!(bridge("fctfawd1234abcd").is_ok());
        assert!(bridge("docker0").is_err());
        assert!(bridge("br-123456789012").is_err());
    }

    #[test]
    fn ruleset_ownership_is_enforced() {
        assert!(nft_ruleset("floatctf_awd", "table inet floatctf_awd {\n}\n").is_ok());
        assert!(
            nft_ruleset(
                "floatctf_awd",
                "table inet floatctf_awd {\n    # restrictive DROP only; no Docker rules touched\n}\n"
            )
            .is_ok()
        );
        assert!(nft_ruleset("floatctf_awd", "flush ruleset\n").is_err());
        assert!(nft_ruleset("floatctf_awd", "table inet other {\n}\n").is_err());
        assert!(
            nft_ruleset(
                "floatctf_awd",
                "table inet floatctf_awd {\n}\nadd table inet other {\n}\n"
            )
            .is_err()
        );
        assert!(
            nft_ruleset(
                "floatctf_awd",
                "table inet floatctf_awd {\n    chain x { type filter hook forward priority 1; delete table inet other; }\n}\n"
            )
            .is_err()
        );
    }
}
