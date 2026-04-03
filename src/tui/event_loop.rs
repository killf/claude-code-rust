//! TUI event loop and interactive mode

use crate::commands::SessionState;
use crate::coordinator::Coordinator;
use crate::api::client::ApiClient;
use crate::types::{Session, Tool};
use futures::future::BoxFuture;
use std::collections::HashMap;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Shared state for REPL commands
#[derive(Clone)]
pub struct ReplState {
    pub session: Arc<Mutex<Session>>,
    pub provider: String,
    pub command_registry: Arc<crate::commands::CommandRegistry>,
    /// Coordinator for multi-agent parallel execution
    pub coordinator: Arc<Coordinator>,
    /// API client for spawning sub-agents
    pub api_client: Arc<ApiClient>,
    /// Available tools for sub-agents
    pub tools: Arc<Vec<Arc<dyn Tool>>>,
    /// Working directory for sub-agents
    pub working_directory: PathBuf,
    /// Environment variables for sub-agents
    pub env: HashMap<String, String>,
}

impl SessionState for ReplState {
    fn session_id(&self) -> String {
        // Use blocking lock for sync context
        futures::executor::block_on(self.session.lock()).id.clone()
    }

    fn model(&self) -> String {
        futures::executor::block_on(self.session.lock()).model.clone()
    }

    fn cost(&self) -> f64 {
        futures::executor::block_on(self.session.lock()).cost
    }

    fn input_tokens(&self) -> u64 {
        futures::executor::block_on(self.session.lock()).token_usage.input_tokens
    }

    fn output_tokens(&self) -> u64 {
        futures::executor::block_on(self.session.lock()).token_usage.output_tokens
    }

    fn total_tokens(&self) -> u64 {
        futures::executor::block_on(self.session.lock()).token_usage.total()
    }

    fn message_count(&self) -> usize {
        futures::executor::block_on(self.session.lock()).messages.len()
    }

    fn provider(&self) -> String {
        self.provider.clone()
    }

    fn print_help(&self) {
        ReplHistory::print_help();
    }

    fn print_exit(&self) {
        println!("Goodbye!");
    }

    fn print_clear(&self) {
        print!("\x1b[2J\x1b[H");
        let _ = std::io::stdout().flush();
    }
}

impl ReplState {
    /// Set the session state on the command registry so builtins can access it
    pub async fn wire_registry(&self) {
        self.command_registry
            .set_session_state(Box::new(self.clone()))
            .await;
    }
}

/// REPL help text (static)
#[allow(dead_code)]
pub struct ReplHistory;

#[allow(dead_code)]
impl ReplHistory {
    #[allow(dead_code)]
    pub fn print_help() {
        println!("\nClaude Code — Available commands:");
        println!("  {:<18} Show this help", "/help, /h");
        println!("  {:<18} Exit Claude Code", "/exit, /q");
        println!("  {:<18} Clear conversation", "/clear");
        println!("  {:<18} Show session info", "/session");
        println!("  {:<18} Show current model", "/model, /m");
        println!("  {:<18} Show token usage", "/tokens, /tc");
        println!("  {:<18} Show estimated cost", "/cost");
        println!("  {:<18} Show configuration", "/config, /cfg");
        println!("  {:<18} Run prompts in parallel agents", "/multi <p1>; <p2>; ...");
        println!();
    }

    #[allow(dead_code)]
    pub fn print_clear() {
        print!("\x1b[2J\x1b[H");
        let _ = std::io::stdout().flush();
    }
}

/// Read a line from stdin, supporting basic editing (backspace) and Ctrl+C.
/// Uses crossterm terminal mode to intercept individual keypresses.
fn read_line_sync() -> Result<String, io::Error> {
    use crossterm::event::{read, Event, KeyCode, KeyModifiers};
    use crossterm::terminal::{disable_raw_mode, enable_raw_mode};

    enable_raw_mode()?;

    let mut input = String::new();

    loop {
        match read() {
            Ok(Event::Key(key)) => {
                if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                    disable_raw_mode().ok();
                    return Ok(String::new());
                }
                match key.code {
                    KeyCode::Enter => {
                        break;
                    }
                    KeyCode::Char(c) => {
                        input.push(c);
                        print!("{c}");
                        io::stdout().flush()?;
                    }
                    KeyCode::Backspace => {
                        if !input.is_empty() {
                            input.pop();
                            print!("\x08 \x08");
                            io::stdout().flush()?;
                        }
                    }
                    KeyCode::Esc => {
                        disable_raw_mode().ok();
                        return Ok(String::new());
                    }
                    _ => {}
                }
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }

    disable_raw_mode().ok();
    Ok(input)
}

/// Async wrapper: run the sync reader in a blocking task.
async fn read_line() -> Result<String, io::Error> {
    tokio::task::spawn_blocking(read_line_sync)
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::Other, "read_line task cancelled"))?
}

pub async fn run_repl(
    mut on_message: impl FnMut(String) -> BoxFuture<'static, bool>,
    state: ReplState,
) -> Result<(), io::Error> {
    println!("Claude Code Interactive Mode");
    println!(
        "Type your message and press Enter. Ctrl+C or Esc to exit.\nType /help for commands.\n---\n"
    );

    loop {
        print!("\n> ");
        io::stdout().flush()?;

        let input = read_line().await?;
        let text = input.trim().to_string();
        if text.is_empty() {
            continue;
        }

        // /multi: run multiple prompts in parallel agents
        if text.starts_with("/multi ") || text == "/multi" {
            let prompt_text = text.strip_prefix("/multi ").unwrap_or("").trim().to_string();
            if prompt_text.is_empty() {
                println!("Usage: /multi <prompt1> [; <prompt2> [; ...]]");
                println!("Runs multiple agents in parallel. Separate prompts with ';'");
                continue;
            }
            let prompts: Vec<String> = prompt_text
                .split(';')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();

            if prompts.is_empty() {
                println!("No valid prompts provided.");
                continue;
            }

            println!("\n[Coordinator] Spawning {} agents in parallel...", prompts.len());
            let model = futures::executor::block_on(state.session.lock()).model.clone();
            let results = state.coordinator
                .run_multi(
                    (*state.api_client).clone(),
                    model,
                    (*state.tools).clone(),
                    state.working_directory.clone(),
                    state.env.clone(),
                    prompts.clone(),
                )
                .await;

            for (i, result) in results.into_iter().enumerate() {
                print!("\n--- Agent {} ---\n", i + 1);
                match result {
                    Ok(output) => {
                        if output.is_empty() {
                            println!("(no output)");
                        } else {
                            println!("{}", output);
                        }
                    }
                    Err(e) => {
                        eprintln!("[Agent {} error: {}]", i + 1, e);
                    }
                }
            }
            println!("\n--- All agents finished ---");
            continue;
        }

        // Route all commands (built-in + custom) through CommandRegistry
        if let Some(cmd) = state.command_registry.resolve(&text) {
            let output = state.command_registry.execute(cmd).await;
            match output {
                Ok(out) => {
                    // success=false signals /exit — exit the REPL
                    if !out.success && out.stdout.is_empty() && out.stderr.is_empty() {
                        return Ok(());
                    }
                    if !out.stdout.is_empty() { println!("{}", out.stdout); }
                    if !out.stderr.is_empty() { eprintln!("{}", out.stderr); }
                }
                Err(e) => {
                    eprintln!("[Command error: {e}]");
                }
            }
            continue;
        }

        // Not a command — send to agent
        println!("\n[Claude]");

        if !on_message(text).await {
            break;
        }
    }

    println!("\nGoodbye!");
    Ok(())
}
