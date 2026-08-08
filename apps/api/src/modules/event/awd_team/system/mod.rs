//! AWD system layer — encapsulates all privileged command execution.
//!
//! # Safety
//!
//! This module is the ONLY place where system commands (nft, wg, ip, conntrack)
//! may be executed. All external command parameters use structured argument
//! passing — never shell string concatenation.

pub mod command;
pub mod conntrack;
pub mod wireguard;
