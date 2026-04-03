//! Prompt construction system
//!
//! Mirrors TypeScript's src/constants/systemPromptSections.ts, src/utils/systemPrompt.ts,
//! and src/constants/prompts.ts:
//! - Section-based composition with memoization
//! - Priority chain: override > coordinator > agent > custom > default
//! - Cache cleared on /clear and /compact

pub mod builtin;
pub mod cache;
pub mod coordinator;
pub mod section;

/// Priority sources for system prompt construction (matches TypeScript buildEffectiveSystemPrompt).
#[derive(Debug, Clone)]
pub enum PromptSource {
    /// Highest priority: override everything else (e.g., loop mode).
    Override(String),
    /// Coordinator mode: orchestrator with worker agents.
    Coordinator,
    /// Agent definition prompt (REPLACES default unless proactive).
    Agent(String),
    /// --system-prompt CLI flag / config value.
    Custom(String),
    /// Standard Claude Code prompt built from builtin sections.
    Default,
}

/// Source of truth for prompt composition.
/// Corresponds to TypeScript's buildEffectiveSystemPrompt inputs.
#[derive(Debug, Clone)]
pub struct PromptContext<'a> {
    /// Override prompt (highest priority).
    pub override_prompt: Option<&'a str>,
    /// Coordinator mode active?
    pub coordinator_mode: bool,
    /// Agent definition system prompt (from AgentDefinition.getSystemPrompt).
    pub agent_definition: Option<&'a str>,
    /// Proactive mode (KAIROS) active?
    pub proactive_mode: bool,
    /// Custom system prompt (--system-prompt flag or config).
    pub custom_prompt: Option<&'a str>,
    /// Appended to end of prompt (except when override is set).
    pub append_prompt: Option<&'a str>,
}

/// Result of building the effective system prompt.
#[derive(Debug, Clone)]
pub struct SystemPrompt {
    /// The composed sections as strings.
    pub sections: Vec<String>,
}

impl SystemPrompt {
    /// Returns the full prompt as a single string.
    pub fn as_string(&self) -> String {
        self.sections.join("\n")
    }

    /// Returns the sections as an array (matches TypeScript SystemPrompt = string[]).
    pub fn sections(&self) -> &[String] {
        &self.sections
    }
}

/// Builds the effective system prompt following TypeScript's priority chain:
/// 1. override → replace all
/// 2. coordinator → coordinator prompt
/// 3. agent (proactive: append; else: replace)
/// 4. custom → replace default
/// 5. default
/// Plus append_prompt appended at end (except when override).
pub fn build_effective_system_prompt(ctx: PromptContext<'_>) -> SystemPrompt {

    // 1. Override wins everything
    if let Some(ref override_prompt) = ctx.override_prompt {
        let mut sections = vec![override_prompt.to_string()];
        if let Some(ref append) = ctx.append_prompt {
            sections.push(append.to_string());
        }
        return SystemPrompt { sections };
    }

    // 2. Coordinator mode
    if ctx.coordinator_mode {
        let coordinator_section = coordinator::get_coordinator_system_prompt();
        let mut sections = vec![coordinator_section];
        if let Some(ref append) = ctx.append_prompt {
            sections.push(append.to_string());
        }
        return SystemPrompt { sections };
    }

    // 3. Agent definition
    if let Some(ref agent_prompt) = ctx.agent_definition {
        if ctx.proactive_mode {
            // In proactive mode, agent instructions APPEND to the default prompt.
            let default_sections = builtin::build_default_prompt();
            let mut sections = default_sections;
            sections.push(format!("\n# Custom Agent Instructions\n{agent_prompt}"));
            if let Some(ref append) = ctx.append_prompt {
                sections.push(append.to_string());
            }
            return SystemPrompt { sections };
        } else {
            // Non-proactive: agent REPLACES default
            let mut sections = vec![agent_prompt.to_string()];
            if let Some(ref append) = ctx.append_prompt {
                sections.push(append.to_string());
            }
            return SystemPrompt { sections };
        }
    }

    // 4. Custom vs Default
    let base = if let Some(ref custom) = ctx.custom_prompt {
        vec![custom.to_string()]
    } else {
        builtin::build_default_prompt()
    };

    let mut sections = base;
    if let Some(ref append) = ctx.append_prompt {
        sections.push(append.to_string());
    }
    SystemPrompt { sections }
}

/// Returns the default Claude Code system prompt (static fallback).
/// Used when no sections are registered or during early boot.
pub fn default_system_prompt() -> &'static str {
    crate::types::session::DEFAULT_SYSTEM_PROMPT
}
