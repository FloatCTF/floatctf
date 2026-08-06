//! Network address management for AWD events.
//!
//! Handles CIDR parsing, subnet allocation, and IP assignment
//! without depending on the `ipnetwork` crate.

use crate::modules::event::awd_team::AwdError;
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
