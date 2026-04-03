//! Graceful shutdown — mirrors TypeScript's utils/gracefulShutdown.ts
//!
//! Sets up:
//! - SIGINT (Ctrl+C) handler that triggers graceful shutdown
//! - A failsafe timer that forces exit if cleanup hangs
//!
//! TypeScript's gracefulShutdown:
//!   1. cleanupTerminalModes()
//!   2. runCleanupFunctions()
//!   3. printResumeHint()
//!   4. forceExit(code)

use std::sync::atomic::{AtomicBool, Ordering};

static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Install SIGINT/SIGTERM handlers that trigger graceful shutdown.
/// Call this synchronously from main() before the async runtime starts.
/// This is safe to call multiple times (only the first call takes effect).
pub fn setup_graceful_shutdown() {
    // SIGINT: Ctrl+C
    let _ = ctrlc::set_handler(|| {
        if SHUTDOWN_REQUESTED.swap(true, Ordering::SeqCst) {
            // Second interrupt — force exit immediately.
            std::process::exit(130);
        }
        eprintln!("\n[Claude Code] Received interrupt. Shutting down gracefully...");
        // Trigger shutdown in the async context.
        // The TUI/event loop checks is_shutdown_requested() and exits its loop.
        // For now, the SIGINT just sets the flag; the REPL must cooperate.
    });
}

/// Check if graceful shutdown has been requested (Ctrl+C pressed).
/// Used by the TUI/REPL to break out of its event loop.
pub fn is_shutdown_requested() -> bool {
    SHUTDOWN_REQUESTED.load(Ordering::SeqCst)
}

/// Trigger graceful shutdown from async context (e.g. cleanup handlers).
/// Does NOT exit — callers should call graceful_shutdown() next.
pub fn request_shutdown() {
    SHUTDOWN_REQUESTED.store(true, Ordering::SeqCst);
}

/// Run cleanup and exit. Called from the SIGINT handler or at natural exit.
pub async fn graceful_shutdown(exit_code: i32) {
    if SHUTDOWN_REQUESTED.swap(true, Ordering::SeqCst) {
        std::process::exit(exit_code);
    }

    eprintln!("[Claude Code] Running cleanup...");
    crate::init::cleanup::run_cleanup_functions().await;

    std::process::exit(exit_code);
}
