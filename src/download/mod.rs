//! Download subsystem.
//!
//! Modules:
//! - [`probe`]   – HTTP HEAD probe (Content-Length, Accept-Ranges)
//! - [`segment`] – Single range-request worker
//! - [`engine`]  – Orchestrator: splits file, spawns workers, tracks progress
//! - [`types`]   – Shared data types and state

pub mod connections;
pub mod engine;
pub mod probe;
pub mod segment;
pub mod types;
