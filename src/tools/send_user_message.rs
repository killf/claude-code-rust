//! SendUserMessage tool (BriefTool) — delivers a markdown message + file attachments to the user.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::error::CliError;
use crate::types::{ResultContentBlock, Tool, ToolContext, ToolResult};

/// Image file extensions recognized by the extension check.
const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp"];

/// Input schema for the SendUserMessage tool.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct BriefInput {
    /// The markdown message to send to the user.
    message: String,
    /// Optional list of file paths (absolute or relative to cwd) to attach.
    #[serde(default)]
    attachments: Option<Vec<String>>,
    /// Intent label: "normal" (reply to user) or "proactive" (unsolicited update).
    /// Does not affect behavior; stored for analytics / UI labeling.
    #[serde(default = "default_status")]
    status: String,
}

fn default_status() -> String {
    "normal".to_string()
}

/// Metadata for a single resolved file attachment in the tool output.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AttachmentMeta {
    /// Absolute filesystem path to the attachment.
    path: String,
    /// File size in bytes.
    size: u64,
    /// Whether the file has an image extension (.png, .jpg, .jpeg, .gif, .webp).
    is_image: bool,
    /// Optional file UUID populated by the bridge upload path.
    #[serde(skip_serializing_if = "Option::is_none")]
    file_uuid: Option<String>,
}

/// Output envelope returned by the tool's call() method.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BriefOutput {
    message: String,
    attachments: Option<Vec<AttachmentMeta>>,
    /// ISO-8601 timestamp captured at tool execution time.
    sent_at: String,
}

/// SendUserMessage (BriefTool) — the primary visible output channel for the
/// assistant to communicate markdown messages and file attachments to the user.
pub struct SendUserMessageTool;

impl SendUserMessageTool {
    pub fn new() -> Self {
        Self
    }

    /// Returns true if the given path has an image file extension.
    fn is_image_path(path: &Path) -> bool {
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| IMAGE_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
            .unwrap_or(false)
    }

    /// Resolves a raw attachment path (which may be relative to cwd) to an
    /// absolute path, validates that it exists and is a regular file, then
    /// returns its metadata.
    fn resolve_attachment(
        raw_path: &str,
        cwd: &Path,
    ) -> Result<AttachmentMeta, String> {
        let raw = PathBuf::from(raw_path);
        let absolute = if raw.is_absolute() {
            raw
        } else {
            cwd.join(&raw)
        };

        let metadata = std::fs::metadata(&absolute)
            .map_err(|e| format!("Failed to stat attachment \"{}\": {}", raw_path, e))?;

        if !metadata.is_file() {
            return Err(format!(
                "Attachment \"{}\" is not a regular file.",
                raw_path
            ));
        }

        Ok(AttachmentMeta {
            path: absolute.to_string_lossy().into_owned(),
            size: metadata.len(),
            is_image: Self::is_image_path(&absolute),
            file_uuid: None,
        })
    }

    fn input_schema() -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "message": {
                    "type": "string",
                    "description": "The message for the user. Supports markdown formatting."
                },
                "attachments": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional file paths (absolute or relative to cwd) to attach. Use for photos, screenshots, diffs, logs, or any file the user should see alongside your message."
                },
                "status": {
                    "type": "string",
                    "enum": ["normal", "proactive"],
                    "description": "Use 'proactive' when you're surfacing something the user hasn't asked for and needs to see now. Use 'normal' when replying to something the user just said."
                }
            },
            "required": ["message"]
        })
    }
}

impl Default for SendUserMessageTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for SendUserMessageTool {
    fn name(&self) -> &str {
        "SendUserMessage"
    }

    fn aliases(&self) -> Vec<String> {
        vec!["Brief".to_string()]
    }

    fn description(&self) -> String {
        "Sends a markdown message to the user, optionally with file attachments. \
         Use this as your primary visible output channel. Attachments can be images, \
         diffs, logs, or any file the user should see alongside your message."
            .to_string()
    }

    fn input_schema(&self) -> serde_json::Value {
        Self::input_schema()
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn is_concurrency_safe(&self) -> bool {
        true
    }

    async fn call(
        &self,
        args: serde_json::Value,
        context: ToolContext,
    ) -> Result<ToolResult, CliError> {
        let input: BriefInput = serde_json::from_value(args)
            .map_err(|e| CliError::ToolExecution(format!("Invalid input: {}", e)))?;

        let sent_at = chrono::Utc::now().to_rfc3339();

        let attachments = match &input.attachments {
            Some(paths) if !paths.is_empty() => {
                let mut metas = Vec::with_capacity(paths.len());
                for raw_path in paths {
                    match Self::resolve_attachment(raw_path, &context.working_directory) {
                        Ok(meta) => metas.push(meta),
                        Err(err) => return Ok(ToolResult::error(err)),
                    }
                }
                Some(metas)
            }
            _ => None,
        };

        let output = BriefOutput {
            message: input.message,
            attachments,
            sent_at,
        };

        let json = serde_json::to_string(&output)
            .map_err(|e| CliError::ToolExecution(format!("Failed to serialize output: {}", e)))?;

        Ok(ToolResult {
            content: vec![ResultContentBlock::Text { text: json }],
            is_error: false,
            metrics: None,
        })
    }

    fn render_use_message(&self, args: &serde_json::Value) -> String {
        if let Ok(input) = serde_json::from_value::<BriefInput>(args.clone()) {
            let n = input.attachments.as_ref().map_or(0, Vec::len);
            if n == 0 {
                format!("Sending message to user: {}", &input.message[..input.message.len().min(80)])
            } else {
                format!(
                    "Sending message to user ({} attachment{}): {}",
                    n,
                    if n == 1 { "" } else { "s" },
                    &input.message[..input.message.len().min(80)]
                )
            }
        } else {
            "Sending message to user".to_string()
        }
    }

    fn render_result_message(&self, result: &ToolResult) -> String {
        if result.content.is_empty() {
            return "Message sent.".to_string();
        }
        let n = result
            .content
            .iter()
            .filter_map(|b| match b {
                ResultContentBlock::Text { text } => {
                    serde_json::from_str::<BriefOutput>(text)
                        .ok()
                        .and_then(|o| o.attachments.map(|a| a.len()))
                }
                _ => None,
            })
            .sum::<usize>();
        if n == 0 {
            "Message sent.".to_string()
        } else {
            format!("Message sent ({} attachment{} included).", n, if n == 1 { "" } else { "s" })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tool_context(temp_dir: &std::path::Path) -> ToolContext {
        ToolContext {
            session_id: "test-session".to_string(),
            agent_id: "test-agent".to_string(),
            working_directory: temp_dir.to_path_buf(),
            can_use_tool: true,
            parent_message_id: None,
            env: std::collections::HashMap::new(),
        }
    }

    fn json_args(message: &str, attachments: Option<Vec<&str>>) -> serde_json::Value {
        let mut map =
            serde_json::Map::from_iter([("message".to_string(), serde_json::json!(message))]);
        if let Some(attachments) = attachments {
            map.insert("attachments".to_string(), serde_json::json!(attachments));
        }
        serde_json::Value::Object(map)
    }

    /// Creates a temporary subdirectory under the OS temp dir, returning the
    /// path and a cleanup guard that deletes it on drop.
    fn temp_subdir() -> (std::path::PathBuf, impl Drop) {
        let base = std::env::temp_dir();
        let dir = base.join(format!("claude-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        (dir.clone(), Cleanup(dir))
    }

    #[tokio::test]
    async fn message_only() {
        let tool = SendUserMessageTool::new();
        let (temp_dir, _cleanup) = temp_subdir();
        let ctx = tool_context(&temp_dir);

        let result = tool.call(json_args("Hello, world!", None), ctx).await.unwrap();

        assert!(!result.is_error);
        let output: BriefOutput = serde_json::from_str(&result.content[0].preview()).unwrap();
        assert_eq!(output.message, "Hello, world!");
        assert!(output.attachments.is_none());
        assert!(!output.sent_at.is_empty());
    }

    #[tokio::test]
    async fn message_with_one_attachment() {
        let tool = SendUserMessageTool::new();
        let (temp_dir, _cleanup) = temp_subdir();

        // Create a test file inside the temp directory
        let file_path = temp_dir.join("notes.txt");
        let mut file = std::fs::File::create(&file_path).unwrap();
        file.write_all(b"Hello from attachment").unwrap();

        let ctx = tool_context(&temp_dir);
        let args = json_args("Here is the log:", Some(vec!["notes.txt"]));

        let result = tool.call(args, ctx).await.unwrap();

        assert!(!result.is_error);
        let output: BriefOutput = serde_json::from_str(&result.content[0].preview()).unwrap();
        assert_eq!(output.message, "Here is the log:");
        let attachments = output.attachments.expect("expected attachments");
        assert_eq!(attachments.len(), 1);
        assert_eq!(attachments[0].path, file_path.to_string_lossy());
        assert_eq!(attachments[0].size, 21);
        assert!(!attachments[0].is_image);
    }

    #[tokio::test]
    async fn image_attachment_detected() {
        let tool = SendUserMessageTool::new();
        let (temp_dir, _cleanup) = temp_subdir();

        let img_path = temp_dir.join("screenshot.png");
        std::fs::write(&img_path, b"fake png data").unwrap();

        let ctx = tool_context(&temp_dir);
        let args = json_args("Screenshot:", Some(vec!["screenshot.png"]));

        let result = tool.call(args, ctx).await.unwrap();

        assert!(!result.is_error);
        let output: BriefOutput = serde_json::from_str(&result.content[0].preview()).unwrap();
        let attachments = output.attachments.expect("expected attachments");
        assert!(attachments[0].is_image);
    }

    #[tokio::test]
    async fn missing_attachment_returns_error() {
        let tool = SendUserMessageTool::new();
        let (temp_dir, _cleanup) = temp_subdir();
        let ctx = tool_context(&temp_dir);
        let args = json_args("Missing file:", Some(vec!["does-not-exist.txt"]));

        let result = tool.call(args, ctx).await.unwrap();

        assert!(result.is_error);
        let text = result.content[0].preview();
        assert!(text.contains("does-not-exist.txt"));
    }

    #[tokio::test]
    async fn proactive_status_accepted() {
        let tool = SendUserMessageTool::new();
        let (temp_dir, _cleanup) = temp_subdir();
        let ctx = tool_context(&temp_dir);

        let mut args = json_args("Unsolicited update", None);
        args.as_object_mut()
            .unwrap()
            .insert("status".to_string(), serde_json::json!("proactive"));

        let result = tool.call(args, ctx).await.unwrap();

        // Status is stored but does not affect behavior; should succeed
        assert!(!result.is_error);
    }

    #[tokio::test]
    async fn relative_path_resolved_to_absolute() {
        let tool = SendUserMessageTool::new();
        let (temp_dir, _cleanup) = temp_subdir();

        let file_path = temp_dir.join("report.log");
        std::fs::write(&file_path, b"log output").unwrap();

        let ctx = tool_context(&temp_dir);
        // Pass relative path — should be resolved against cwd
        let args = json_args("Log attached:", Some(vec!["report.log"]));

        let result = tool.call(args, ctx).await.unwrap();

        assert!(!result.is_error);
        let output: BriefOutput = serde_json::from_str(&result.content[0].preview()).unwrap();
        let attachments = output.attachments.expect("expected attachments");
        assert_eq!(attachments[0].path, file_path.to_string_lossy());
    }
}
