//! Session types for Claude Code CLI

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::{Message, PermissionMode};

/// Default system prompt for Claude Code sessions
pub const DEFAULT_SYSTEM_PROMPT: &str = r#"You are Claude Code, an AI assistant built by Anthropic.
Your knowledge cutoff is 2024-04. You are helpful, creative, and care about writing good software.

Available tools: bash, read, write, edit, grep, glob, web_fetch, web_search.
- Use bash to run commands.
- Use read to view files.
- Use write to create or overwrite files.
- Use edit to make targeted changes.
- Use grep to search file contents.
- Use glob to find files by pattern.
- Use web_fetch to get content from a URL.
- Use web_search to search the web.

When using tools:
- Be precise about file paths.
- Prefer the most targeted tool for the job.
- Always prefer existing files over creating new ones.
- Check your work by reading files back.

For code tasks:
- Write clean, readable code.
- Follow the project's existing conventions.
- Write tests when adding significant logic.

If you don't know something, say so rather than guessing.
"#;

/// A Claude Code session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub messages: Vec<Message>,
    pub model: String,
    pub system_prompt: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub cost: f64,
    pub token_usage: TokenUsage,
}

impl Session {
    pub fn new(id: String, model: String) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        Self {
            id,
            messages: Vec::new(),
            model,
            system_prompt: None,
            created_at: now,
            updated_at: now,
            cost: 0.0,
            token_usage: TokenUsage::default(),
        }
    }

    pub fn add_message(&mut self, msg: Message) {
        self.updated_at = chrono::Utc::now().timestamp_millis();
        self.messages.push(msg);
    }

    pub fn input_tokens(&self) -> u64 {
        self.token_usage.input_tokens
    }

    pub fn output_tokens(&self) -> u64 {
        self.token_usage.output_tokens
    }

    pub fn total_tokens(&self) -> u64 {
        self.token_usage.total()
    }
}

/// Token usage tracking
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cache_read_tokens: u64,
}

impl TokenUsage {
    pub fn total(&self) -> u64 {
        self.input_tokens
            + self.output_tokens
            + self.cache_creation_tokens
            + self.cache_read_tokens
    }

    pub fn add(&mut self, other: &TokenUsage) {
        self.input_tokens += other.input_tokens;
        self.output_tokens += other.output_tokens;
        self.cache_creation_tokens += other.cache_creation_tokens;
        self.cache_read_tokens += other.cache_read_tokens;
    }
}

/// Session configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    pub model: String,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub system_prompt: Option<String>,
    pub permission_mode: PermissionMode,
    pub tools: Vec<serde_json::Value>,
    pub metadata: HashMap<String, serde_json::Value>,
    /// Highest-priority override prompt (e.g., loop mode)
    pub override_prompt: Option<String>,
    /// Coordinator (multi-agent) mode
    pub coordinator_mode: bool,
    /// Agent definition system prompt
    pub agent_definition: Option<String>,
    /// Proactive (KAIROS) mode
    pub proactive_mode: bool,
    /// Custom system prompt (--system-prompt flag)
    pub custom_prompt: Option<String>,
    /// Prompt appended to end of system prompt
    pub append_prompt: Option<String>,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            model: "claude-opus-4-5".to_string(),
            temperature: None,
            max_tokens: Some(8192),
            system_prompt: None,
            permission_mode: PermissionMode::Default,
            tools: Vec::new(),
            metadata: HashMap::new(),
            override_prompt: None,
            coordinator_mode: false,
            agent_definition: None,
            proactive_mode: false,
            custom_prompt: None,
            append_prompt: None,
        }
    }
}

impl SessionConfig {
    /// Create a SessionConfig with a specific permission mode
    pub fn with_permission_mode(mut self, mode: PermissionMode) -> Self {
        self.permission_mode = mode;
        self
    }
}

/// Session summary for listing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: String,
    pub model: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub cost: f64,
    pub message_count: usize,
    pub active_worktree: Option<String>,
}
