//! The `bb` command's modules, as a library so the fuzz targets in `fuzz/` can call the parsers
//! directly. Not a stable API: `src/main.rs` is its only intended user.
pub mod api;
pub mod auth;
pub mod browse;
pub mod client;
pub mod config;
pub mod context;
pub mod exit;
pub mod output;
pub mod pipeline;
pub mod pr;
pub mod repo;
pub mod workspace;
