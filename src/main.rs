//! Claude Code CLI binary entry point
//!
//! Mirrors TypeScript's bootstrap-entry.ts + main.tsx layered startup:
//! - Layer 1 (sync, main): fast-path flags (--version, --update) before any deps
//! - Layer 2 (async): full CLI via cli::run()

use open_cc::cli;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // ── Layer 1: Fast-path flags — zero module loading ──────────────────────
    // Only --version / -V: -v is used by clap as --verbose shorthand.
    // Matches TypeScript bootstrap-entry.ts: args.length === 1 guard.
    if args.len() == 2 {
        match args[1].as_str() {
            "--version" | "-V" => {
                // env!() is a compile-time constant — zero runtime cost.
                println!("{} (Claude Code)", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            "--update" | "--upgrade" => {
                eprintln!("Hint: use the 'update' subcommand to update Claude Code.");
                eprintln!("Run 'claude --help' for available commands.");
                std::process::exit(1);
            }
            _ => {}
        }
    }

    // ── Layer 2: Async CLI ─────────────────────────────────────────────────
    // Use manual runtime builder so main() stays sync (needed for ctrlc::set_handler
    // in init::graceful). Mirrors TypeScript's main() async function.
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async_main());
}

async fn async_main() {
    if let Err(e) = cli::run().await {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}
