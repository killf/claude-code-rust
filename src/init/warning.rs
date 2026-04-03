//! Panic handler — mirrors TypeScript's utils/warningHandler.ts
//!
//! TypeScript's initializeWarningHandler():
//! - Suppresses default Node.js process.on('warning') handler noise
//! - Installs custom handler that counts unique warnings and logs to analytics
//!
//! In Rust, the equivalent is replacing std::panic::set_hook to:
//! - Count unique panic messages (bounded map) for analytics
//! - Log each panic to the analytics/error sink before aborting

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

static PANIC_COUNTS: OnceLock<Mutex<HashMap<String, usize>>> = OnceLock::new();
static PREV_HOOK: OnceLock<Mutex<Option<Box<dyn Fn(&std::panic::PanicHookInfo) + Send + Sync>>>> =
    OnceLock::new();

const MAX_PANIC_KEYS: usize = 50;

/// Initialize the panic handler.
/// Call this synchronously after config loading but before the REPL starts.
/// Idempotent — safe to call multiple times.
pub fn initialize_warning_handler() {
    let prev = std::panic::take_hook();
    let _ = PREV_HOOK.set(Mutex::new(Some(prev)));

    std::panic::set_hook(Box::new(move |info| {
        let key = panic_key(info);

        // Count occurrences (bounded map).
        let count = {
            let counts = PANIC_COUNTS.get_or_init(|| Mutex::new(HashMap::new()));
            let mut guard = counts.lock().unwrap();
            if guard.len() < MAX_PANIC_KEYS || guard.contains_key(&key) {
                *guard.entry(key.clone()).or_insert(0) += 1;
            }
            *guard.get(&key).unwrap_or(&1)
        };

        // Log to analytics (fire-and-forget — don't block the panic).
        crate::analytics::log_event(
            "process_panic",
            Some(serde_json::json!({
                "panic_key": key,
                "count": count,
            })),
        );

        // Call the previous hook (usually the default that prints to stderr).
        if let Some(hook) = PREV_HOOK.get().and_then(|m| m.lock().ok()?.take()) {
            hook(info);
        }
    }));
}

/// Extract a short stable key from panic info for counting.
fn panic_key(info: &std::panic::PanicHookInfo) -> String {
    let msg = info
        .payload()
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| info.payload().downcast_ref::<String>().map(|s| s.as_str()))
        .unwrap_or("<unknown>");
    let loc = info
        .location()
        .map(|l| format!("{}:{}", l.line(), l.column()))
        .unwrap_or_else(|| "<unknown>".to_string());
    // Truncate message to avoid unbounded key sizes.
    let msg = if msg.len() > 80 { &msg[..80] } else { msg };
    format!("{}: {}", loc, msg)
}
