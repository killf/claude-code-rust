//! Slash command parsing and registry.
//!
//! Mirrors TypeScript's src/utils/slashCommandParsing.ts and src/commands.ts.
//!
//! Slash commands are triggered by user input starting with `/`. Commands are
//! either built-in (e.g., /help, /compact, /clear) or skill-based.

/// Result of parsing a slash command input.
#[derive(Debug, Clone)]
pub struct ParsedSlashCommand {
    /// The command name (without leading `/`).
    pub command_name: String,
    /// Arguments after the command name.
    pub args: String,
    /// Whether this is an MCP tool command.
    pub is_mcp: bool,
}

/// Built-in command names.
/// Matches TypeScript's `builtInCommandNames` in commands.ts.
pub const BUILTIN_COMMAND_NAMES: &[&str] = &[
    "help",
    "compact",
    "clear",
    "status",
    "model",
    "commit",
    "review-pr",
    "review",
    "plan",
    "compact",
    "clear",
    "skills",
    "task",
    "tasks",
    "memory",
    "diff",
    "session",
    "init",
    "config",
    "theme",
    "cost",
    "resume",
    "attach",
    "ide",
    "doctor",
    "feedback",
    "logout",
    "login",
    "share",
    "exit",
    "quit",
];

/// Result of executing a slash command.
pub enum SlashCommandResult {
    /// Command was handled, return a message to show the user.
    Handled(String),
    /// Command triggered a session compaction.
    Compact,
    /// Command triggered a session clear.
    Clear,
    /// Command should be forwarded to the agent as a normal message.
    ForwardToAgent,
    /// Command is unknown.
    Unknown,
    /// Command requires async execution (e.g., skill).
    Async(Box<dyn std::future::Future<Output = SlashCommandResult> + Send>),
}

impl std::fmt::Debug for SlashCommandResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Handled(s) => f.debug_tuple("Handled").field(s).finish(),
            Self::Compact => write!(f, "Compact"),
            Self::Clear => write!(f, "Clear"),
            Self::ForwardToAgent => write!(f, "ForwardToAgent"),
            Self::Unknown => write!(f, "Unknown"),
            Self::Async(_) => write!(f, "Async(...)"),
        }
    }
}

/// Parse a slash command input string into its component parts.
/// Returns `None` if the input doesn't start with `/`.
///
/// Mirrors TypeScript's `parseSlashCommand()`.
pub fn parse_slash_command(input: &str) -> Option<ParsedSlashCommand> {
    let trimmed = input.trim();
    if !trimmed.starts_with('/') {
        return None;
    }

    let without_slash = &trimmed[1..];
    let words: Vec<&str> = without_slash.split_whitespace().collect();

    if words.is_empty() {
        return None;
    }

    let mut command_name = words[0].to_string();
    let mut is_mcp = false;
    let mut args_start = 1;

    // Check for MCP commands: `/tool (MCP) arg1 arg2`
    if words.len() > 1 && words[1] == "(MCP)" {
        command_name = format!("{command_name} (MCP)");
        is_mcp = true;
        args_start = 2;
    }

    let args = if args_start < words.len() {
        words[args_start..].join(" ")
    } else {
        String::new()
    };

    Some(ParsedSlashCommand {
        command_name,
        args,
        is_mcp,
    })
}

/// Check if a command name is a built-in command.
pub fn is_builtin_command(name: &str) -> bool {
    BUILTIN_COMMAND_NAMES.contains(&name)
}

/// Execute a slash command synchronously.
/// Returns the result of executing the command.
pub fn execute_slash_command(
    cmd: &ParsedSlashCommand,
    context: &SlashCommandContext,
) -> SlashCommandResult {
    match cmd.command_name.as_str() {
        "help" => execute_help(&cmd.args),
        "compact" => SlashCommandResult::Compact,
        "clear" => SlashCommandResult::Clear,
        "status" => execute_status(context),
        "model" => execute_model(&cmd.args, context),
        "commit" => SlashCommandResult::ForwardToAgent,
        "review-pr" | "review" => SlashCommandResult::ForwardToAgent,
        "plan" => SlashCommandResult::ForwardToAgent,
        "skills" => execute_skills(),
        "task" | "tasks" => SlashCommandResult::ForwardToAgent,
        "memory" => SlashCommandResult::ForwardToAgent,
        "diff" => SlashCommandResult::ForwardToAgent,
        "session" => SlashCommandResult::ForwardToAgent,
        "init" => SlashCommandResult::ForwardToAgent,
        "config" => SlashCommandResult::ForwardToAgent,
        "theme" => SlashCommandResult::ForwardToAgent,
        "cost" => execute_cost(context),
        "resume" => SlashCommandResult::ForwardToAgent,
        "attach" => SlashCommandResult::ForwardToAgent,
        "ide" => SlashCommandResult::ForwardToAgent,
        "doctor" => SlashCommandResult::ForwardToAgent,
        "feedback" => SlashCommandResult::ForwardToAgent,
        "logout" | "login" => SlashCommandResult::ForwardToAgent,
        "share" => SlashCommandResult::ForwardToAgent,
        "exit" | "quit" => SlashCommandResult::Handled("Goodbye!".to_string()),
        _ => SlashCommandResult::Unknown,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Built-in command handlers
// ─────────────────────────────────────────────────────────────────────────────

/// Context available when executing slash commands.
#[derive(Debug, Clone)]
pub struct SlashCommandContext {
    pub session_id: String,
    pub model: String,
    pub message_count: usize,
    pub cost: f64,
}

fn execute_help(_args: &str) -> SlashCommandResult {
    let msg = r#"Available slash commands:

/help              - Show this help message
/compact           - Compact conversation to save tokens
/clear             - Clear conversation history
/status            - Show current session status
/model [name]      - Show or set the model
/commit [msg]      - Commit changes (git)
/review [pr]       - Review a pull request
/plan              - Enter plan mode
/skills            - List available skills
/tasks             - List tasks
/memory            - Manage session memory
/diff              - Show uncommitted changes
/session [cmd]     - Manage sessions
/config [key] [val] - Get or set configuration
/theme [name]      - Change color theme
/cost              - Show session cost
/resume [id]       - Resume a previous session
/doctor            - Run diagnostics
/exit              - Exit Claude Code
"#;
    SlashCommandResult::Handled(msg.to_string())
}

fn execute_status(ctx: &SlashCommandContext) -> SlashCommandResult {
    let msg = format!(
        "Session: {}\nModel: {}\nMessages: {}\nCost: ${:.4}",
        ctx.session_id, ctx.model, ctx.message_count, ctx.cost
    );
    SlashCommandResult::Handled(msg)
}

fn execute_model(args: &str, ctx: &SlashCommandContext) -> SlashCommandResult {
    if args.is_empty() {
        return SlashCommandResult::Handled(format!("Current model: {}", ctx.model));
    }
    SlashCommandResult::Handled(format!(
        "Model set to: {}\nNote: Use --model CLI flag or /config model to change model.",
        args
    ))
}

fn execute_skills() -> SlashCommandResult {
    let msg = r#"Skills directory is not yet configured.
Skills allow you to add custom slash commands.
See https://docs.claude.com for more information."#;
    SlashCommandResult::Handled(msg.to_string())
}

fn execute_cost(ctx: &SlashCommandContext) -> SlashCommandResult {
    SlashCommandResult::Handled(format!("Session cost: ${:.4}", ctx.cost))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_slash_command_basic() {
        let result = parse_slash_command("/help").unwrap();
        assert_eq!(result.command_name, "help");
        assert_eq!(result.args, "");
        assert!(!result.is_mcp);
    }

    #[test]
    fn test_parse_slash_command_with_args() {
        let result = parse_slash_command("/model claude-opus-4-6").unwrap();
        assert_eq!(result.command_name, "model");
        assert_eq!(result.args, "claude-opus-4-6");
        assert!(!result.is_mcp);
    }

    #[test]
    fn test_parse_slash_command_mcp() {
        let result = parse_slash_command("/fetch (MCP) arg1 arg2").unwrap();
        assert_eq!(result.command_name, "fetch (MCP)");
        assert_eq!(result.args, "arg1 arg2");
        assert!(result.is_mcp);
    }

    #[test]
    fn test_parse_slash_command_not_slash() {
        assert!(parse_slash_command("help").is_none());
    }

    #[test]
    fn test_parse_slash_command_whitespace() {
        let result = parse_slash_command("  /compact  ").unwrap();
        assert_eq!(result.command_name, "compact");
    }

    #[test]
    fn test_is_builtin_command() {
        assert!(is_builtin_command("help"));
        assert!(is_builtin_command("compact"));
        assert!(is_builtin_command("clear"));
        assert!(!is_builtin_command("foobar"));
    }

    #[test]
    fn test_execute_help() {
        let result = execute_help("");
        match result {
            SlashCommandResult::Handled(msg) => {
                assert!(msg.contains("Available slash commands"));
            }
            _ => panic!("expected Handled"),
        }
    }
}
