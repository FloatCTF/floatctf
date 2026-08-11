//! Network address management for AWD events.
//!
//! Handles CIDR parsing, subnet allocation, and IP assignment
//! without depending on the `ipnetwork` crate.

use crate::modules::event::awd::AwdError;
use std::net::Ipv4Addr;

/// Represents a parsed IPv4 CIDR block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ipv4Cidr {
    pub network: Ipv4Addr,
    pub prefix_len: u8,
}

/// Represents an allocated subnet within a larger CIDR.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllocatedSubnet {
    pub cidr: Ipv4Cidr,
    pub team_id: Option<uuid::Uuid>,
}

impl Ipv4Cidr {
    /// Parse a CIDR string like "10.0.0.0/16".
    pub fn parse(cidr_str: &str) -> Result<Self, AwdError> {
        let parts: Vec<&str> = cidr_str.split('/').collect();
        if parts.len() != 2 {
            return Err(AwdError::Validation(format!(
                "Invalid CIDR format: {}",
                cidr_str
            )));
        }

        let ip: Ipv4Addr = parts[0]
            .parse()
            .map_err(|e| AwdError::Validation(format!("Invalid IP in CIDR: {}", e)))?;

        let prefix_len: u8 = parts[1]
            .parse()
            .map_err(|e| AwdError::Validation(format!("Invalid prefix length: {}", e)))?;

        if prefix_len > 32 {
            return Err(AwdError::Validation(format!(
                "Prefix length must be <= 32, got {}",
                prefix_len
            )));
        }

        Ok(Self {
            network: ip,
            prefix_len,
        })
    }

    /// Convert back to a CIDR string.
    pub fn to_string(&self) -> String {
        format!("{}/{}", self.network, self.prefix_len)
    }

    /// 转 ipnetwork::IpNetwork（DB CIDR 列读写用，§20）。
    pub fn to_ipnetwork(&self) -> ipnetwork::IpNetwork {
        ipnetwork::IpNetwork::V4(
            ipnetwork::Ipv4Network::new(self.network, self.prefix_len)
                .expect("Ipv4Cidr already normalized"),
        )
    }

    /// 从 ipnetwork::IpNetwork 解析（DB 列读取后转换）。
    pub fn from_ipnetwork(net: &ipnetwork::IpNetwork) -> Option<Self> {
        match net {
            ipnetwork::IpNetwork::V4(v4) => Some(Self {
                network: v4.network(),
                prefix_len: v4.prefix(),
            }),
            ipnetwork::IpNetwork::V6(_) => None,
        }
    }

    /// Total number of addresses in this CIDR block.
    pub fn total_addresses(&self) -> u64 {
        1u64 << (32 - self.prefix_len as u64)
    }

    /// The subnet mask as an Ipv4Addr.
    pub fn mask(&self) -> Ipv4Addr {
        let bits = u32::MAX << (32 - self.prefix_len);
        Ipv4Addr::from(bits)
    }

    /// The network address (first address in the block).
    pub fn network_address(&self) -> Ipv4Addr {
        self.network
    }

    /// The broadcast address (last address in the block).
    pub fn broadcast_address(&self) -> Ipv4Addr {
        let bits = u32::from(self.network) | !u32::from(self.mask());
        Ipv4Addr::from(bits)
    }

    /// Check if this CIDR contains the given IP.
    pub fn contains(&self, ip: Ipv4Addr) -> bool {
        let ip_bits = u32::from(ip);
        let net_bits = u32::from(self.network);
        let mask_bits = u32::from(self.mask());
        (ip_bits & mask_bits) == (net_bits & mask_bits)
    }

    /// Check if two CIDR blocks overlap.
    pub fn overlaps(&self, other: &Ipv4Cidr) -> bool {
        self.contains(other.network)
            || self.contains(other.broadcast_address())
            || other.contains(self.network)
            || other.contains(self.broadcast_address())
    }

    /// Get the nth usable host address (skipping network and broadcast addresses).
    /// Returns None if n exceeds the number of usable addresses.
    pub fn nth_host(&self, n: u32) -> Option<Ipv4Addr> {
        let total = self.total_addresses();
        if total < 3 || n as u64 >= total - 2 {
            return None;
        }
        let base = u32::from(self.network);
        Some(Ipv4Addr::from(base + (n + 1) as u32))
    }

    /// Split this CIDR into /24 subnets (or specified new_prefix).
    /// Returns None if new_prefix < current prefix or new_prefix > 32.
    pub fn subnets(&self, new_prefix: u8) -> Option<Vec<Ipv4Cidr>> {
        if new_prefix < self.prefix_len || new_prefix > 32 {
            return None;
        }

        let count = 1u64 << (new_prefix as u64 - self.prefix_len as u64);
        let subnet_size = 1u64 << (32 - new_prefix as u64);
        let base = u32::from(self.network);

        let mut result = Vec::with_capacity(count as usize);
        for i in 0..count {
            let subnet_base = Ipv4Addr::from(base + (i * subnet_size) as u32);
            result.push(Ipv4Cidr {
                network: subnet_base,
                prefix_len: new_prefix,
            });
        }

        Some(result)
    }

    /// 惰性取第 index 个 new_prefix 子网（不生成全量 Vec，§79 防百万级枚举）。
    /// index 从 0 开始；超出容量返回 None。
    pub fn nth_subnet(&self, new_prefix: u8, index: u64) -> Option<Ipv4Cidr> {
        if new_prefix < self.prefix_len || new_prefix > 32 {
            return None;
        }
        let count = 1u64 << (new_prefix as u64 - self.prefix_len as u64);
        if index >= count {
            return None;
        }
        let subnet_size = 1u64 << (32 - new_prefix as u64);
        let base = u32::from(self.network);
        Some(Ipv4Cidr {
            network: Ipv4Addr::from(base + (index * subnet_size) as u32),
            prefix_len: new_prefix,
        })
    }

    /// 本子网在父网中的 slot 序号（index）。不在父网内或未对齐返回 None。
    pub fn subnet_index_in(&self, parent: &Ipv4Cidr) -> Option<u64> {
        if self.prefix_len < parent.prefix_len || !parent.contains(self.network) {
            return None;
        }
        let size = 1u64 << (32 - self.prefix_len as u64);
        let offset = u32::from(self.network).wrapping_sub(u32::from(parent.network));
        if offset as u64 % size != 0 {
            return None;
        }
        Some(offset as u64 / size)
    }
}

/// 平台地址池：pool 前缀 ≤ event 前缀 ≤ team 前缀（§5/§10）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkPool {
    pub pool: Ipv4Cidr,
    pub event_prefix: u8,
    pub team_prefix: u8,
}

impl NetworkPool {
    /// 构造并校验前缀顺序：pool ≤ event ≤ team ≤ 32。
    pub fn new(pool: Ipv4Cidr, event_prefix: u8, team_prefix: u8) -> Result<Self, AwdError> {
        if event_prefix < pool.prefix_len {
            return Err(AwdError::Validation(format!(
                "event prefix /{} must not be shorter than pool prefix /{}",
                event_prefix, pool.prefix_len
            )));
        }
        if team_prefix < event_prefix {
            return Err(AwdError::Validation(format!(
                "team prefix /{} must not be shorter than event prefix /{}",
                team_prefix, event_prefix
            )));
        }
        if team_prefix > 32 {
            return Err(AwdError::Validation(format!(
                "team prefix /{} out of range",
                team_prefix
            )));
        }
        Ok(Self {
            pool,
            event_prefix,
            team_prefix,
        })
    }

    /// 理论可容纳的 Event 数量。
    pub fn event_capacity(&self) -> u64 {
        1u64 << (self.event_prefix as u64 - self.pool.prefix_len as u64)
    }

    /// 单个 Event 内理论 Team 数量（第一块保留给基础设施，§25）。
    pub fn team_capacity_per_event(&self) -> u64 {
        (1u64 << (self.team_prefix as u64 - self.event_prefix as u64)).saturating_sub(1)
    }

    /// 单个 Team 子网可容纳的主机数（含网络/广播地址）。
    pub fn hosts_per_team(&self) -> u64 {
        1u64 << (32 - self.team_prefix as u64)
    }

    /// host_offset 合法范围校验（§41）：
    /// 0 = network 地址；1 = gateway 保留（§26）；末位 = broadcast。
    pub fn validate_host_offset(&self, offset: u16) -> bool {
        offset >= 2 && (offset as u64) < self.hosts_per_team().saturating_sub(1)
    }

    /// 惰性取第 index 个 event 级子网。
    pub fn nth_event_subnet(&self, index: u64) -> Option<Ipv4Cidr> {
        self.pool.nth_subnet(self.event_prefix, index)
    }
}

/// WireGuard 端口池（§29）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WireGuardPortRange {
    pub min: u16,
    pub max: u16,
}

impl WireGuardPortRange {
    pub fn new(min: u16, max: u16) -> Result<Self, AwdError> {
        // u16 天然上限 65535，无需再校验 max；min<1 排除端口 0。
        if min < 1 || min > max {
            return Err(AwdError::Validation(format!(
                "invalid WG port range {}-{}",
                min, max
            )));
        }
        Ok(Self { min, max })
    }

    /// 理论可容纳的 Event 数（端口数）。
    pub fn capacity(&self) -> u64 {
        (self.max - self.min) as u64 + 1
    }

    pub fn contains(&self, port: u16) -> bool {
        port >= self.min && port <= self.max
    }
}

/// 基础设施主机保留策略（§26，typed constants，禁止散落 magic number）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InfraHostPolicy;

impl InfraHostPolicy {
    /// .1 网关 / runtime 保留
    pub const GATEWAY_RESERVED: u16 = 1;
    /// .2 FlagServer
    pub const FLAGSERVER_OFFSET: u16 = 2;
    /// .3 JudgeServer
    pub const JUDGESERVER_OFFSET: u16 = 3;

    /// 基础设施子网 = 包含 flagserver_ip 的 team-prefix 块（§25 派生）。
    pub fn infrastructure_subnet(
        gamebox_cidr: &Ipv4Cidr,
        flagserver_ip: Ipv4Addr,
        team_prefix: u8,
    ) -> Option<Ipv4Cidr> {
        let bits = u32::from(flagserver_ip);
        let mask = if team_prefix >= 32 {
            u32::MAX
        } else {
            u32::MAX << (32 - team_prefix)
        };
        let network = Ipv4Addr::from(bits & mask);
        let cidr = Ipv4Cidr {
            network,
            prefix_len: team_prefix,
        };
        if !gamebox_cidr.contains(network) {
            return None;
        }
        Some(cidr)
    }

    /// 基础设施子网内的固定服务 IP：offset 2/3 → .2/.3。
    pub fn service_ip(infra: &Ipv4Cidr, offset: u16) -> Option<Ipv4Addr> {
        infra.nth_host(offset.saturating_sub(1) as u32)
    }
}

/// WG 接口名：deterministic + collision-resistant + ≤ Linux 15 字符（§27）。
/// 形如 `fawg_<8hex>`（13 字符）。
pub fn wireguard_interface_name(event_id: &uuid::Uuid) -> String {
    format!("fawg_{}", &event_id.simple().to_string()[..8])
}

/// Docker 网络逻辑名：deterministic（§28），desired identity。
/// 形如 `fctf-awd-<8hex>`。
pub fn docker_network_name(event_id: &uuid::Uuid) -> String {
    format!("fctf-awd-{}", &event_id.simple().to_string()[..8])
}

/// Team 子网分配器（§36/§38/§39，纯逻辑）：
/// - 已有分配永远优先复用（持久化 subnet 为事实）；
/// - 新 Team 取第一个空闲 slot（index 0 = infra 保留）；
/// - used_indexes 应包含「所有已分配过」的 slot（含 released 行），
///   保证同一 Event 生命周期内不复用已释放的 Team slot。
pub struct TeamSubnetAllocator<'a> {
    pub event_cidr: &'a Ipv4Cidr,
    pub team_prefix: u8,
    pub used_indexes: &'a [u64],
}

impl<'a> TeamSubnetAllocator<'a> {
    /// 第一个空闲 team slot；用尽返回 None。
    pub fn next_free_index(&self) -> Option<u64> {
        let capacity = 1u64 << (self.team_prefix as u64 - self.event_cidr.prefix_len as u64);
        // index 0 是基础设施块（§25），从 1 开始
        for i in 1..capacity {
            if !self.used_indexes.contains(&i) {
                return Some(i);
            }
        }
        None
    }

    pub fn subnet_for_index(&self, index: u64) -> Option<Ipv4Cidr> {
        self.event_cidr.nth_subnet(self.team_prefix, index)
    }
}

#[cfg(test)]
mod net_pool_tests {
    use super::*;

    fn c(s: &str) -> Ipv4Cidr {
        Ipv4Cidr::parse(s).unwrap()
    }

    #[test]
    fn prefix_order_validated() {
        assert!(NetworkPool::new(c("10.0.0.0/8"), 16, 24).is_ok());
        assert!(NetworkPool::new(c("10.0.0.0/8"), 8, 16).is_ok());
        // event 前缀短于 pool 前缀 → 非法（§10 反例）
        assert!(NetworkPool::new(c("10.0.0.0/16"), 12, 24).is_err());
        // event 前缀长于 team 前缀 → 非法
        assert!(NetworkPool::new(c("10.0.0.0/8"), 24, 16).is_err());
        assert!(NetworkPool::new(c("10.0.0.0/8"), 16, 40).is_err());
    }

    #[test]
    fn capacity_formulas() {
        let pool = NetworkPool::new(c("10.0.0.0/8"), 16, 24).unwrap();
        assert_eq!(pool.event_capacity(), 256);
        assert_eq!(pool.team_capacity_per_event(), 255); // 256 - 1 infra
        assert_eq!(pool.hosts_per_team(), 256);
    }

    #[test]
    fn host_offset_bounds() {
        let pool = NetworkPool::new(c("10.0.0.0/8"), 16, 24).unwrap();
        assert!(!pool.validate_host_offset(0)); // network
        assert!(!pool.validate_host_offset(1)); // gateway reserved
        assert!(pool.validate_host_offset(2));
        assert!(pool.validate_host_offset(254));
        assert!(!pool.validate_host_offset(255)); // broadcast
    }

    #[test]
    fn nth_subnet_lazy() {
        let cidr = c("10.0.0.0/16");
        assert_eq!(cidr.nth_subnet(24, 0).unwrap().to_string(), "10.0.0.0/24");
        assert_eq!(cidr.nth_subnet(24, 1).unwrap().to_string(), "10.0.1.0/24");
        assert_eq!(
            cidr.nth_subnet(24, 255).unwrap().to_string(),
            "10.0.255.0/24"
        );
        assert!(cidr.nth_subnet(24, 256).is_none());
    }

    #[test]
    fn subnet_index_roundtrip() {
        let parent = c("10.42.0.0/16");
        for i in [0u64, 1, 5, 255] {
            let sub = parent.nth_subnet(24, i).unwrap();
            assert_eq!(sub.subnet_index_in(&parent), Some(i));
        }
        assert_eq!(c("10.43.0.0/24").subnet_index_in(&parent), None);
    }

    #[test]
    fn infra_derivation() {
        let gb = c("10.42.0.0/16");
        let infra =
            InfraHostPolicy::infrastructure_subnet(&gb, "10.42.0.10".parse().unwrap(), 24).unwrap();
        assert_eq!(infra.to_string(), "10.42.0.0/24");
        assert_eq!(
            InfraHostPolicy::service_ip(&infra, InfraHostPolicy::FLAGSERVER_OFFSET).unwrap(),
            "10.42.0.2".parse::<Ipv4Addr>().unwrap()
        );
        assert_eq!(
            InfraHostPolicy::service_ip(&infra, InfraHostPolicy::JUDGESERVER_OFFSET).unwrap(),
            "10.42.0.3".parse::<Ipv4Addr>().unwrap()
        );
    }

    #[test]
    fn interface_and_docker_names_deterministic() {
        let id = uuid::Uuid::parse_str("12345678-abcd-4def-9abc-1234567890ab").unwrap();
        let wg = wireguard_interface_name(&id);
        let dn = docker_network_name(&id);
        assert_eq!(wg, "fawg_12345678");
        assert_eq!(dn, "fctf-awd-12345678");
        assert!(wg.len() <= 15);
        assert_eq!(wireguard_interface_name(&id), wg);
        assert_eq!(docker_network_name(&id), dn);
    }

    #[test]
    fn team_allocator_skips_infra_and_used() {
        let event = c("10.42.0.0/16");
        let a = TeamSubnetAllocator {
            event_cidr: &event,
            team_prefix: 24,
            used_indexes: &[1, 3],
        };
        assert_eq!(a.next_free_index(), Some(2));
        assert_eq!(a.subnet_for_index(2).unwrap().to_string(), "10.42.2.0/24");
        let full = TeamSubnetAllocator {
            event_cidr: &event,
            team_prefix: 24,
            used_indexes: &[],
        };
        assert_eq!(full.next_free_index(), Some(1)); // 0 = infra 保留
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_cidr() {
        let cidr = Ipv4Cidr::parse("10.0.0.0/16").unwrap();
        assert_eq!(cidr.prefix_len, 16);
        assert_eq!(cidr.network, Ipv4Addr::new(10, 0, 0, 0));
    }

    #[test]
    fn test_parse_invalid_cidr_rejected() {
        assert!(Ipv4Cidr::parse("not-a-cidr").is_err());
        assert!(Ipv4Cidr::parse("10.0.0.0/33").is_err());
    }

    #[test]
    fn test_contains() {
        let cidr = Ipv4Cidr::parse("10.0.0.0/24").unwrap();
        assert!(cidr.contains(Ipv4Addr::new(10, 0, 0, 1)));
        assert!(cidr.contains(Ipv4Addr::new(10, 0, 0, 254)));
        assert!(!cidr.contains(Ipv4Addr::new(10, 0, 1, 1)));
    }

    #[test]
    fn test_overlap_detection() {
        let a = Ipv4Cidr::parse("10.0.0.0/16").unwrap();
        let b = Ipv4Cidr::parse("10.0.1.0/24").unwrap();
        let c = Ipv4Cidr::parse("10.1.0.0/16").unwrap();

        assert!(a.overlaps(&b));
        assert!(b.overlaps(&a));
        assert!(!a.overlaps(&c));
    }

    #[test]
    fn test_subnets() {
        let cidr = Ipv4Cidr::parse("10.0.0.0/16").unwrap();
        let subnets = cidr.subnets(24).unwrap();
        assert_eq!(subnets.len(), 256);
        assert_eq!(subnets[0].to_string(), "10.0.0.0/24");
        assert_eq!(subnets[255].to_string(), "10.0.255.0/24");
    }

    #[test]
    fn test_nth_host() {
        let cidr = Ipv4Cidr::parse("10.0.0.0/24").unwrap();
        assert_eq!(cidr.nth_host(0), Some(Ipv4Addr::new(10, 0, 0, 1)));
        assert_eq!(cidr.nth_host(1), Some(Ipv4Addr::new(10, 0, 0, 2)));
        assert_eq!(cidr.nth_host(252), Some(Ipv4Addr::new(10, 0, 0, 253)));
        assert_eq!(cidr.nth_host(253), Some(Ipv4Addr::new(10, 0, 0, 254))); // last usable
        assert_eq!(cidr.nth_host(254), None); // would be broadcast
    }

    #[test]
    fn test_total_addresses() {
        assert_eq!(
            Ipv4Cidr::parse("10.0.0.0/24").unwrap().total_addresses(),
            256
        );
        assert_eq!(
            Ipv4Cidr::parse("10.0.0.0/16").unwrap().total_addresses(),
            65536
        );
    }
}
