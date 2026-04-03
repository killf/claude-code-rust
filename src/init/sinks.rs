//! Analytics sinks initialization.
//!
//! Mirrors TypeScript's utils/sinks.ts — specifically initSinks():
//!   initializeErrorLogSink() + initializeAnalyticsSink()
//!
//! In TypeScript, initSinks() is called from init() AFTER configs are loaded
//! but BEFORE the REPL starts, so that events emitted during startup are captured.
//!
//! In Rust, analytics::log_event() already writes to local JSONL synchronously.
//! This module adds the init call and future sink extensibility.

/// Initialize all analytics and error sinks.
/// Idempotent — safe to call multiple times.
/// Mirrors TypeScript's initSinks().
pub fn init_sinks() {
    // Phase 1: local JSONL is synchronous — no "connection" to open.
    // Phase 2: would open HTTP sink to GrowthBook/Datadog here.
    tracing::debug!("analytics sinks initialized");
}
