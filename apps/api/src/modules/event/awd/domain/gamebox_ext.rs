//! Extension methods for GameBox status transitions + AWD GameBox 领域纯函数。

use crate::entity::sea_orm_active_enums::GameboxStatus;
use crate::modules::event::awd::domain::Ipv4Cidr;

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
