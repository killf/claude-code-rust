//! Hook system for pre/post tool and query callbacks

use crate::config::HookConfig;
use crate::error::CliError;
use crate::types::ToolResult;

/// A hook definition loaded from config
#[derive(Debug, Clone)]
pub struct Hook {
    pub name: String,
    pub hook_type: HookType,
    /// Shell command to run. Receives JSON payload on stdin.
    pub command: String,
    /// Timeout in seconds
    pub timeout_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookType {
    PreToolUse,
    PostToolUse,
    SessionStart,
    UserPromptSubmit,
    SessionEnd,
    Stop,
}

impl Hook {
    /// Build the JSON payload sent to a pre_tool_use hook
    pub fn pre_tool_payload(tool_name: &str, tool_input: &serde_json::Value, session_id: &str) -> serde_json::Value {
        serde_json::json!({
            "hook": "pre_tool_use",
            "tool_name": tool_name,
            "tool_input": tool_input,
            "session_id": session_id,
        })
    }

    /// Build the JSON payload sent to a post_tool_use hook
    pub fn post_tool_payload(tool_name: &str, result: &ToolResult, session_id: &str) -> serde_json::Value {
        serde_json::json!({
            "hook": "post_tool_use",
            "tool_name": tool_name,
            "result": {
                "is_error": result.is_error,
                "content": result.content.iter().map(|b| b.preview()).collect::<Vec<_>>(),
            },
            "session_id": session_id,
        })
    }

    /// Run the hook command with the given JSON payload on stdin
    pub async fn run(&self, payload: &serde_json::Value) -> Result<String, CliError> {
        let json = serde_json::to_string(payload)
            .map_err(|e| CliError::Other(format!("hook payload error: {e}")))?;

        let mut cmd = tokio::process::Command::new("sh");
        cmd.arg("-c")
           .arg(&self.command)
           .kill_on_drop(true);

        let mut child = cmd
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| CliError::Other(format!("hook spawn failed: {e}")))?;

        if let Some(mut stdin) = child.stdin.take() {
            tokio::io::AsyncWriteExt::write_all(&mut stdin, json.as_bytes()).await
                .map_err(|e| CliError::Other(format!("hook stdin error: {e}")))?;
            drop(stdin);
        }

        let timeout = std::time::Duration::from_secs(self.timeout_secs);
        let output = tokio::time::timeout(timeout, child.wait_with_output())
            .await
            .map_err(|_| CliError::Other(format!("hook '{}' timed out", self.name)))?
            .map_err(|e| CliError::Other(format!("hook '{}' failed: {e}", self.name)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(CliError::Other(format!(
                "hook '{}' returned non-zero: {}",
                self.name, stderr
            )));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

/// Load hooks from a config hooks list
pub fn load_hooks_from_config(hooks_config: &[HookConfig]) -> Vec<Hook> {
    let mut hooks = Vec::new();

    for cfg in hooks_config {
        if !cfg.enabled || cfg.command.is_empty() {
            continue;
        }

        for event in &cfg.events {
            let hook_type = match event.as_str() {
                "pre_tool_use" => HookType::PreToolUse,
                "post_tool_use" => HookType::PostToolUse,
                "session_start" => HookType::SessionStart,
                "user_prompt_submit" => HookType::UserPromptSubmit,
                "session_end" => HookType::SessionEnd,
                "stop" => HookType::Stop,
                _ => continue,
            };

            hooks.push(Hook {
                name: cfg.name.clone(),
                hook_type,
                command: cfg.command.clone(),
                timeout_secs: 10,
            });
        }
    }

    hooks
}
