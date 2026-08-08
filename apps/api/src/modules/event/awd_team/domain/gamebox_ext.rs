//! Extension methods for GameBox status transitions + GameBox 领域纯函数。

use crate::entity::sea_orm_active_enums::GameboxStatus;
use crate::modules::event::awd_team::domain::Ipv4Cidr;

/// safe_name 校验规则（§54）：`^[a-z0-9][a-z0-9_-]*$`，与 Challenge safe_name 对齐。
pub fn validate_safe_name(s: &str) -> Result<(), String> {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return Err("safe_name 不能为空".into());
    }
    let first_ok = bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit();
    if !first_ok {
        return Err(format!("safe_name 必须以小写字母或数字开头: {s}"));
    }
    for &b in bytes {
        let ok = b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_';
        if !ok {
            return Err(format!(
                "safe_name 只允许 [a-z0-9_-]: {s}（字符 0x{:02x}）",
                b
            ));
        }
    }
    Ok(())
}

/// 由展示名生成 safe_name 候选（小写 + 非字母数字转 -）。
pub fn slugify(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut last_dash = false;
    for ch in name.chars() {
        let c = ch.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() || c == '_' {
            out.push(c);
            last_dash = false;
        } else if !last_dash && !out.is_empty() {
            out.push('-');
            last_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

/// 确定性 IP 分配（§13/§66）：`instance_ip = team.gamebox_subnet + event_gamebox.host_offset`。
/// 例：子网 10.42.1.0/24 + offset 10 → 10.42.1.10。offset 必须在 2..=254（避开 .1 网关/广播/保留）。
pub fn instance_ip_for_offset(subnet: &str, host_offset: i16) -> Option<String> {
    if !(2..=254).contains(&host_offset) {
        return None;
    }
    let cidr = Ipv4Cidr::parse(subnet).ok()?;
    // nth_host(n) = network + (n+1)，故 offset 直接映射 host 字节：nth_host(offset-1) = network+offset
    cidr.nth_host(host_offset as u32 - 1)
        .map(|ip| ip.to_string())
}

pub trait GameboxStatusExt {
    fn is_healthy(&self) -> bool;
    fn needs_attention(&self) -> bool;
    fn is_transitional(&self) -> bool;
    fn valid_transitions(&self) -> &'static [GameboxStatus];
    fn can_transition_to(&self, target: GameboxStatus) -> Result<(), String>;
}

impl GameboxStatusExt for GameboxStatus {
    fn is_healthy(&self) -> bool {
        matches!(self, Self::Ready | Self::Running)
    }

    fn needs_attention(&self) -> bool {
        matches!(
            self,
            Self::Missing | Self::Orphan | Self::Conflict | Self::StartFailed | Self::ResetFailed
        )
    }

    fn is_transitional(&self) -> bool {
        matches!(self, Self::Pending | Self::Creating | Self::Resetting)
    }

    fn valid_transitions(&self) -> &'static [GameboxStatus] {
        match self {
            Self::Pending => &[Self::Creating],
            Self::Creating => &[Self::Running, Self::StartFailed],
            Self::Running => &[Self::Ready, Self::Missing, Self::Stopped],
            Self::Ready => &[Self::Resetting, Self::Missing, Self::Stopped, Self::Running],
            Self::Resetting => &[Self::Ready, Self::ResetFailed],
            Self::Missing => &[Self::Creating],
            Self::Orphan => &[Self::Ready, Self::Stopped],
            Self::Conflict => &[Self::Ready],
            Self::StartFailed => &[Self::Creating],
            Self::ResetFailed => &[Self::Resetting],
            Self::Stopped => &[Self::Creating],
        }
    }

    fn can_transition_to(&self, target: GameboxStatus) -> Result<(), String> {
        if self.valid_transitions().contains(&target) {
            Ok(())
        } else {
            Err(format!(
                "Invalid GameBox transition: {:?} -> {:?}",
                self, target
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_name_validation_rules() {
        assert!(validate_safe_name("easy-web").is_ok());
        assert!(validate_safe_name("easy_web").is_ok());
        assert!(validate_safe_name("easyweb1").is_ok());
        assert!(validate_safe_name("EasyWeb").is_err(), "大写非法");
        assert!(validate_safe_name("1easy").is_ok());
        assert!(validate_safe_name("-easy").is_err(), "不能以 - 开头");
        assert!(validate_safe_name("easy web").is_err(), "空格非法");
        assert!(validate_safe_name("").is_err());
    }

    #[test]
    fn slugify_basics() {
        assert_eq!(slugify("Easy Web"), "easy-web");
        assert_eq!(slugify("Pwn 01"), "pwn-01");
        assert_eq!(slugify("  Misc "), "misc");
        assert_eq!(slugify("Already_snake"), "already_snake");
        assert_eq!(slugify("a--b"), "a-b");
    }

    #[test]
    fn instance_ip_is_deterministic_per_offset() {
        // §66：EasyWeb offset=10, Pwn offset=11；Team A 10.1.1.0/24, Team B 10.1.2.0/24
        assert_eq!(
            instance_ip_for_offset("10.1.1.0/24", 10).as_deref(),
            Some("10.1.1.10")
        );
        assert_eq!(
            instance_ip_for_offset("10.1.2.0/24", 10).as_deref(),
            Some("10.1.2.10")
        );
        assert_eq!(
            instance_ip_for_offset("10.1.1.0/24", 11).as_deref(),
            Some("10.1.1.11")
        );
        assert_eq!(
            instance_ip_for_offset("10.1.2.0/24", 11).as_deref(),
            Some("10.1.2.11")
        );
        // offset 边界
        assert_eq!(
            instance_ip_for_offset("10.1.1.0/24", 2).as_deref(),
            Some("10.1.1.2")
        );
        assert_eq!(instance_ip_for_offset("10.1.1.0/24", 254).is_some(), true);
        assert_eq!(
            instance_ip_for_offset("10.1.1.0/24", 1),
            None,
            ".1 保留给网关"
        );
        assert_eq!(instance_ip_for_offset("10.1.1.0/24", 255), None, "广播地址");
        assert_eq!(instance_ip_for_offset("10.1.1.0/24", 0), None);
    }

    #[test]
    fn new_gamebox_does_not_change_existing_ips() {
        // 新增队伍 / 新增 offset 不影响已分配 IP（纯函数：IP 只由 subnet+offset 决定）
        let a1 = instance_ip_for_offset("10.1.1.0/24", 10);
        let _team_c = instance_ip_for_offset("10.1.3.0/24", 10); // 新队伍
        let _new_gb = instance_ip_for_offset("10.1.1.0/24", 12); // 新 GameBox
        assert_eq!(a1.as_deref(), Some("10.1.1.10"));
    }
}
