//! Analytics event logging.
//!
//! Mirrors TypeScript's src/services/analytics/index.ts.
//!
//! For now, logs events locally to ~/.claude/analytics.jsonl.
//! TypeScript has GrowthBook/Datadog integration — Rust starts with local-only.

use serde::Serialize;
use serde_json::json;

/// Analytics event.
#[derive(Debug, Serialize)]
pub struct AnalyticsEvent {
    pub event: String,
    pub timestamp_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<serde_json::Value>,
}

/// Log an analytics event to the local event log.
pub fn log_event<S: Serialize + std::fmt::Debug>(
    name: &str,
    properties: Option<S>,
) {
    let event = AnalyticsEvent {
        event: name.to_string(),
        timestamp_ms: chrono::Utc::now().timestamp_millis(),
        properties: properties.map(|p| serde_json::to_value(&p).unwrap_or_default()),
    };

    if let Err(e) = write_event(&event) {
        eprintln!("[Warning] Failed to log analytics event '{name}': {e}");
    }
}

/// Write an event to the local JSONL file.
fn write_event(event: &AnalyticsEvent) -> Result<(), crate::error::CliError> {
    use std::io::Write;

    let path = analytics_path()?;
    let line = serde_json::to_string(event)? + "\n";

    // Ensure directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| crate::error::CliError::Io(e))?;
    }

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| crate::error::CliError::Io(e))?;

    file.write_all(line.as_bytes())
        .map_err(|e| crate::error::CliError::Io(e))?;

    Ok(())
}

/// Path to the local analytics log file.
fn analytics_path() -> Result<std::path::PathBuf, crate::error::CliError> {
    let base = dirs::config_dir()
        .ok_or_else(|| crate::error::CliError::Other("could not determine config directory".into()))?;
    Ok(base.join("claude").join("analytics.jsonl"))
}

// ─────────────────────────────────────────────────────────────────────────────
// Pre-defined event helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Log a hook execution event.
#[allow(dead_code)]
pub fn log_hook(hook_name: &str, hook_type: &str, duration_ms: u64, success: bool) {
    log_event("hook_executed", Some(json!({
        "hook_name": hook_name,
        "hook_type": hook_type,
        "duration_ms": duration_ms,
        "success": success,
    })));
}

/// Log a session start event.
#[allow(dead_code)]
pub fn log_session_start(session_id: &str, model: &str) {
    log_event("session_start", Some(json!({
        "session_id": session_id,
        "model": model,
    })));
}

/// Log a session end event.
#[allow(dead_code)]
pub fn log_session_end(session_id: &str, message_count: usize, cost: f64) {
    log_event("session_end", Some(json!({
        "session_id": session_id,
        "message_count": message_count,
        "cost": cost,
    })));
}

/// Log a config lock contention event.
#[allow(dead_code)]
pub fn log_config_lock_contention(lock_time_ms: u64) {
    log_event("tengu_config_lock_contention", Some(json!({
        "lock_time_ms": lock_time_ms,
    })));
}

/// Log a slash command execution event.
#[allow(dead_code)]
pub fn log_slash_command(command_name: &str) {
    log_event("slash_command", Some(json!({
        "command_name": command_name,
    })));
}
