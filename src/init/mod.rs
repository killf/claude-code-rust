//! Application initialization layer
//!
//! Mirrors TypeScript's src/entrypoints/init.ts:
//! - cleanup registry (cleanup.rs)
//! - graceful shutdown (graceful.rs)
//! - panic/warning handler (warning.rs)
//! - analytics sinks (sinks.rs)
//! - config env injection (env_inject.rs)
//!
//! Phase order in cli/mod.rs:
//!   1. Bootstrap::load()        — config only
//!   2. inject_config_env()      — apply config.env → std::env
//!   3. setup_graceful_shutdown()
//!   4. initialize_warning_handler()
//!   5. init_sinks()
//!   6. register_cleanup()
//!   7. Bootstrap::resolve_auth() — ANTHROPIC_AUTH_TOKEN now visible

pub mod cleanup;
pub mod graceful;
pub mod warning;
pub mod sinks;
pub mod env_inject;
