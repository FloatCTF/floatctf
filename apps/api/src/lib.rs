//! FloatCTF — CTF competition platform library.
//!
//! This crate exposes all modules for integration testing.
//! The binary entry point (`main.rs`) calls `bootstrap::run()`.

pub mod api;
pub mod bootstrap;
pub mod core;
pub mod entity;
pub mod infrastructure;
pub mod modules;
pub mod scheduler;
