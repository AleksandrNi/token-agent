//! # Token Service Library
//!
//! Provides functionality for fetching tokens from various sources,
//! parsing them according to a structured contract, caching them,
//! and exposing them to sinks and endpoints.
//!
//! Modules:
//! - `config` — service configuration and contract types
//! - `cache` — token cache implementation
//! - `sources` — HTTP, Metadata, OAuth2 token sources
//! - `parser` — parsing responses to extract tokens and expirations

pub mod config;
pub mod cache;
pub mod sources;
pub mod resilience;
pub mod parser;
pub mod tests;
pub mod observability;
pub mod server;
pub mod sinks;
pub mod helpers;
pub mod utils;


pub use crate::config::sources::*;
pub use crate::parser::parser::parse_tokens;
