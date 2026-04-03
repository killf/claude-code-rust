//! Agent engine - core loop

use crate::api::ApiClient;
use crate::analytics::{log_hook, log_session_start};
use crate::error::CliError;
use crate::prompts::{build_effective_system_prompt, PromptContext};
use crate::session::{CompactionConfig, SessionCompactor};
use crate::types::{
    ContentBlock, Message, PermissionDecision, ToolContext, ToolResult,
    UserContent,
};
use super::slash::{
    execute_slash_command, parse_slash_command, SlashCommandContext, SlashCommandResult,
};

use super::context::AgentContext;
use super::hooks::{Hook, HookType};
use super::permission::PermissionChecker;

/// Result of agent execution
#[derive(Debug)]
pub enum AgentOutcome {
    Completed,
    Error(String),
    Interrupted,
}

/// Core agent engine
pub struct AgentEngine {
    api_client: ApiClient,
    context: AgentContext,
    permission_checker: PermissionChecker,
    compactor: SessionCompactor,
}

impl AgentEngine {
    pub fn new(
        api_client: ApiClient,
        context: AgentContext,
    ) -> Self {
        let permission_checker = PermissionChecker::new(context.permission_mode());
        let compactor = SessionCompactor::new(CompactionConfig::default());

        // Run SessionStart hooks synchronously (shell commands are fast)
        let start_hooks = context.hooks_of_type(HookType::SessionStart);
        let session_id = context.session.id.clone();
        let cwd = context.working_directory.display().to_string();
        for hook in &start_hooks {
            let start = std::time::Instant::now();
            let payload = serde_json::json!({
                "hook": "session_start",
                "session_id": session_id,
                "cwd": cwd,
            });
            let hook_name = hook.name.clone();
            // Use block_on to run the async hook synchronously
            let rt = tokio::runtime::Handle::current();
            let result = rt.block_on(hook.run(&payload));
            let hook_ok = result.is_ok();
            if let Err(ref e) = result {
                eprintln!("[Hook warning] session_start '{}': {e}", hook_name);
            }
            log_hook(&hook.name, "session_start", start.elapsed().as_millis() as u64, hook_ok);
        }

        // Log session start analytics
        log_session_start(&context.session.id, context.model());

        Self {
            api_client,
            context,
            permission_checker,
            compactor,
        }
    }

    /// Run the agent with an initial prompt
    pub async fn run(&mut self, initial_prompt: String) -> Result<AgentOutcome, CliError> {
        // Check if initial prompt is a slash command
        if let Some(result) = self.handle_slash_command(&initial_prompt)? {
            match result {
                SlashCommandResult::Handled(msg) => {
                    println!("{msg}");
                    return Ok(AgentOutcome::Completed);
                }
                SlashCommandResult::Compact => {
                    self.compactor.compact(&mut self.context.session.messages);
                    eprintln!("[Info] Conversation compacted.");
                }
                SlashCommandResult::Clear => {
                    self.context.session.messages.clear();
                    eprintln!("[Info] Conversation cleared.");
                }
                SlashCommandResult::ForwardToAgent
                | SlashCommandResult::Unknown
                | SlashCommandResult::Async(_) => {
                    // Pass through to agent
                }
            }
        }

        // Add user message
        self.context.session.add_message(Message::User {
            content: UserContent::text(initial_prompt),
        });

        loop {
            // Build effective system prompt from all sources
            let ctx = PromptContext {
                override_prompt: self.context.config.override_prompt.as_deref(),
                coordinator_mode: self.context.config.coordinator_mode,
                agent_definition: self.context.config.agent_definition.as_deref(),
                proactive_mode: self.context.config.proactive_mode,
                custom_prompt: self.context.config.custom_prompt.as_deref(),
                append_prompt: None,
            };
            let system_prompt = build_effective_system_prompt(ctx);
            let system_sections = system_prompt.sections.into();

            // Build messages for API
            let model = self.context.model().to_string();
            let max_tokens = self.context.config.max_tokens.unwrap_or(8192);

            // Send to API
            let tools = if self.context.tools.is_empty() {
                None
            } else {
                Some(self.context.tools.as_slice())
            };
            let response = self.api_client.chat(&self.context.session, &model, max_tokens, tools, system_sections).await?;

            // Track token usage
            self.context.session.token_usage.add(&response.usage);

            // Check if we need to compact context
            if self.compactor.should_compact(&self.context.session.messages, &self.context.session.token_usage) {
                eprintln!("[Info] Compacting conversation to stay within token limit...");
                self.compactor.compact(&mut self.context.session.messages);
            }

            // Track cost
            let cost = self.api_client.estimate_cost(&response.usage, &model);
            self.context.session.cost += cost;

            // Add assistant response
            self.context.session.add_message(Message::Assistant {
                content: Some(response.content.clone()),
            });

            // Process content blocks
            let mut tool_results = Vec::new();

            for block in &response.content.content {
                match block {
                    ContentBlock::Text { text } => {
                        if !text.is_empty() {
                            println!("{}", text);
                        }
                    }
                    ContentBlock::ToolUse { id, name, input } => {
                        // Find the tool
                        let tool = match self.context.find_tool(name) {
                            Some(t) => t,
                            None => {
                                let result = ToolResult::error(format!("Unknown tool: {}", name));
                                tool_results.push((id.clone(), result));
                                continue;
                            }
                        };

                        // Check permissions
                        let decision = self.permission_checker
                            .check_tool(tool.as_ref(), input, "")
                            .await;

                        match decision {
                            PermissionDecision::Allow => {
                                // Execute tool
                                let tool_ctx = ToolContext {
                                    session_id: self.context.session.id.clone(),
                                    agent_id: "main".to_string(),
                                    working_directory: self.context.working_directory.clone(),
                                    can_use_tool: true,
                                    parent_message_id: None,
                                    env: self.context.env.clone(),
                                };

                                // Run pre_tool_use hooks
                                for hook in self.context.hooks_of_type(HookType::PreToolUse) {
                                    let payload = Hook::pre_tool_payload(tool.name(), input, &self.context.session.id);
                                    if let Err(e) = hook.run(&payload).await {
                                        eprintln!("[Hook warning] {}: {e}", hook.name);
                                    }
                                }

                                let result = tool.call(input.clone(), tool_ctx).await
                                    .unwrap_or_else(|e| ToolResult::error(e.to_string()));

                                // Run post_tool_use hooks
                                for hook in self.context.hooks_of_type(HookType::PostToolUse) {
                                    let payload = Hook::post_tool_payload(tool.name(), &result, &self.context.session.id);
                                    if let Err(e) = hook.run(&payload).await {
                                        eprintln!("[Hook warning] {}: {e}", hook.name);
                                    }
                                }

                                tool_results.push((id.clone(), result));
                            }
                            PermissionDecision::Ask { message, .. } => {
                                // In interactive mode, would ask user
                                let result = ToolResult::error(format!(
                                    "Permission required: {}\nPlease run with --permission-mode=acceptEdits to bypass.",
                                    message
                                ));
                                tool_results.push((id.clone(), result));
                            }
                            PermissionDecision::Deny(msg) => {
                                let result = ToolResult::error(format!("Permission denied: {}", msg));
                                tool_results.push((id.clone(), result));
                            }
                            PermissionDecision::Passthrough(msg) => {
                                let result = ToolResult::text(msg);
                                tool_results.push((id.clone(), result));
                            }
                        }
                    }
                    ContentBlock::Image { .. } => {}
                    ContentBlock::ToolResult { .. } => {}
                }
            }

            // Add tool results to session
            for (tool_use_id, result) in tool_results {
                self.context.session.add_message(Message::ToolResult {
                    tool_use_id,
                    content: result.content.iter()
                        .map(|b| b.preview())
                        .collect::<Vec<_>>()
                        .join("\n"),
                    is_error: result.is_error,
                });
            }

            // Check stop reason
            if response.stop_reason.as_deref() == Some("end_turn") {
                return Ok(AgentOutcome::Completed);
            }

            if response.stop_reason.as_deref() == Some("max_tokens") {
                // Continue with another turn
                continue;
            }

            if response.stop_reason.is_none() && response.content.content.is_empty() {
                return Ok(AgentOutcome::Completed);
            }
        }
    }

    /// Run in streaming mode
    pub async fn run_streaming(
        &mut self,
        initial_prompt: String,
    ) -> Result<AgentOutcome, CliError> {
        self.context.session.add_message(Message::User {
            content: UserContent::text(initial_prompt),
        });

        let ctx = PromptContext {
            override_prompt: self.context.config.override_prompt.as_deref(),
            coordinator_mode: self.context.config.coordinator_mode,
            agent_definition: self.context.config.agent_definition.as_deref(),
            proactive_mode: self.context.config.proactive_mode,
            custom_prompt: self.context.config.custom_prompt.as_deref(),
            append_prompt: None,
        };
        let system_prompt = build_effective_system_prompt(ctx);
        let system_sections = system_prompt.sections.into();

        let model = self.context.model().to_string();
        let max_tokens = self.context.config.max_tokens.unwrap_or(8192);

        let tools = if self.context.tools.is_empty() {
            None
        } else {
            Some(self.context.tools.as_slice())
        };

        let mut collected_text = String::new();

        self.api_client
            .chat_streaming(&self.context.session, &model, max_tokens, tools, |text| {
                print!("{}", text);
                collected_text.push_str(&text);
            }, system_sections)
            .await?;

        println!();

        // Parse collected text into content blocks
        let content = ContentBlock::Text { text: collected_text };

        self.context.session.add_message(Message::Assistant {
            content: Some(crate::types::AssistantContent {
                content: vec![content],
                model: model.clone(),
                stop_reason: None,
            }),
        });

        Ok(AgentOutcome::Completed)
    }

    /// Run the agent with an existing session (no new user message added).
    /// Used for resume — the session is already loaded with history.
    pub async fn run_resume(&mut self) -> Result<AgentOutcome, CliError> {
        loop {
            let ctx = PromptContext {
                override_prompt: self.context.config.override_prompt.as_deref(),
                coordinator_mode: self.context.config.coordinator_mode,
                agent_definition: self.context.config.agent_definition.as_deref(),
                proactive_mode: self.context.config.proactive_mode,
                custom_prompt: self.context.config.custom_prompt.as_deref(),
                append_prompt: None,
            };
            let system_prompt = build_effective_system_prompt(ctx);
            let system_sections = system_prompt.sections.into();

            let model = self.context.model().to_string();
            let max_tokens = self.context.config.max_tokens.unwrap_or(8192);

            let tools = if self.context.tools.is_empty() {
                None
            } else {
                Some(self.context.tools.as_slice())
            };

            let response = self.api_client.chat(&self.context.session, &model, max_tokens, tools, system_sections).await?;

            self.context.session.token_usage.add(&response.usage);
            let cost = self.api_client.estimate_cost(&response.usage, &model);
            self.context.session.cost += cost;

            if self.compactor.should_compact(&self.context.session.messages, &self.context.session.token_usage) {
                eprintln!("[Info] Compacting conversation to stay within token limit...");
                self.compactor.compact(&mut self.context.session.messages);
            }

            self.context.session.add_message(Message::Assistant {
                content: Some(response.content.clone()),
            });

            let mut tool_results = Vec::new();

            for block in &response.content.content {
                match block {
                    ContentBlock::Text { text } => {
                        if !text.is_empty() {
                            println!("{}", text);
                        }
                    }
                    ContentBlock::ToolUse { id, name, input } => {
                        let tool = match self.context.find_tool(name) {
                            Some(t) => t,
                            None => {
                                let result = ToolResult::error(format!("Unknown tool: {}", name));
                                tool_results.push((id.clone(), result));
                                continue;
                            }
                        };

                        let decision = self.permission_checker
                            .check_tool(tool.as_ref(), input, "")
                            .await;

                        match decision {
                            PermissionDecision::Allow => {
                                let tool_ctx = ToolContext {
                                    session_id: self.context.session.id.clone(),
                                    agent_id: "main".to_string(),
                                    working_directory: self.context.working_directory.clone(),
                                    can_use_tool: true,
                                    parent_message_id: None,
                                    env: self.context.env.clone(),
                                };

                                for hook in self.context.hooks_of_type(HookType::PreToolUse) {
                                    let payload = Hook::pre_tool_payload(tool.name(), input, &self.context.session.id);
                                    if let Err(e) = hook.run(&payload).await {
                                        eprintln!("[Hook warning] {}: {e}", hook.name);
                                    }
                                }

                                let result = tool.call(input.clone(), tool_ctx).await
                                    .unwrap_or_else(|e| ToolResult::error(e.to_string()));

                                for hook in self.context.hooks_of_type(HookType::PostToolUse) {
                                    let payload = Hook::post_tool_payload(tool.name(), &result, &self.context.session.id);
                                    if let Err(e) = hook.run(&payload).await {
                                        eprintln!("[Hook warning] {}: {e}", hook.name);
                                    }
                                }

                                tool_results.push((id.clone(), result));
                            }
                            PermissionDecision::Ask { message, .. } => {
                                let result = ToolResult::error(format!(
                                    "Permission required: {}\nPlease run with --permission-mode=acceptEdits to bypass.",
                                    message
                                ));
                                tool_results.push((id.clone(), result));
                            }
                            PermissionDecision::Deny(msg) => {
                                let result = ToolResult::error(format!("Permission denied: {}", msg));
                                tool_results.push((id.clone(), result));
                            }
                            PermissionDecision::Passthrough(msg) => {
                                let result = ToolResult::text(msg);
                                tool_results.push((id.clone(), result));
                            }
                        }
                    }
                    ContentBlock::Image { .. } => {}
                    ContentBlock::ToolResult { .. } => {}
                }
            }

            for (tool_use_id, result) in tool_results {
                self.context.session.add_message(Message::ToolResult {
                    tool_use_id,
                    content: result.content.iter().map(|b| b.preview()).collect::<Vec<_>>().join("\n"),
                    is_error: result.is_error,
                });
            }

            if response.stop_reason.as_deref() == Some("end_turn") {
                return Ok(AgentOutcome::Completed);
            }
            if response.stop_reason.as_deref() == Some("max_tokens") {
                continue;
            }
            if response.stop_reason.is_none() && response.content.content.is_empty() {
                return Ok(AgentOutcome::Completed);
            }
        }
    }

    /// Get session reference
    pub fn session(&self) -> &crate::types::Session {
        &self.context.session
    }

    /// Get session for persistence
    pub fn session_mut(&mut self) -> &mut crate::types::Session {
        &mut self.context.session
    }

    /// Handle a slash command if the input starts with `/`.
    /// Returns `None` if not a slash command, or `Some(result)` with the command result.
    pub fn handle_slash_command(&mut self, input: &str) -> Result<Option<SlashCommandResult>, CliError> {
        let Some(cmd) = parse_slash_command(input) else {
            return Ok(None);
        };

        let slash_ctx = SlashCommandContext {
            session_id: self.context.session.id.clone(),
            model: self.context.model().to_string(),
            message_count: self.context.session.messages.len(),
            cost: self.context.session.cost,
        };

        let result = execute_slash_command(&cmd, &slash_ctx);

        // Handle async results inline for now
        match &result {
            SlashCommandResult::Async(_) => {
                // In a real implementation, this would be awaited
                // For now, forward to agent
                return Ok(None);
            }
            _ => {}
        }

        Ok(Some(result))
    }
}
