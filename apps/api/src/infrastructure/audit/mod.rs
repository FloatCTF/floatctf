//! Structured audit logging for sensitive admin / AWD operations.

pub mod service;

pub use service::{AuditAction, AuditService};
