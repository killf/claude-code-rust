//! OpenCC - unified Claude Code Rust implementation

pub mod agent;
pub mod analytics;
pub mod api;
pub mod cli;
pub mod commands;
pub mod config;
pub mod coordinator;
pub mod error;
pub mod init;
pub mod lsp;
pub mod mcp;
pub mod plugins;
pub mod prompts;
pub mod session;
pub mod tools;
pub mod tui;
pub mod types;

pub use error::CliError;
