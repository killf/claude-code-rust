//! SendMessage tool - inter-agent messaging within the multi-agent system

use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Mutex;

use crate::error::CliError;
use crate::types::{Tool, ToolContext, ToolResult};

/// In-memory message stored for a teammate
#[derive(Debug, Clone)]
pub struct AgentMessage {
    pub from: String,
    pub to: String,
    pub message: String,
    pub message_type: String,
    pub request_id: Option<String>,
    pub timestamp: i64,
}

/// Thread-safe singleton mailbox mapping teammate name -> their message queue
#[derive(Debug)]
pub struct Mailbox {
    /// Map from recipient name to received messages.
    /// Special keys: "all" is used for broadcast; recipients receive copies
    /// when a message is sent to "all".
    messages: Mutex<HashMap<String, Vec<AgentMessage>>>,
}

impl Mailbox {
    #[allow(dead_code)]
    fn new() -> Self {
        Self { messages: Mutex::new(HashMap::new()) }
    }

    /// Returns the global mailbox instance.
    /// Uses a Mutex to allow concurrent access from multiple threads while
    /// ensuring all threads see the same mailbox state.
    /// Returns the global mailbox instance for the current thread.
    /// Uses a Mutex per thread to ensure test isolation in parallel execution.
    fn global() -> &'static Mailbox {
        use std::cell::RefCell;
        thread_local! {
            static MAILBOX: RefCell<Mailbox> = RefCell::new(Mailbox::new());
        }
        // SAFETY: thread-local RefCell is only accessed by the current thread.
        // The 'static lifetime is technically scoped to the thread, but rustc
        // accepts this pattern because Mailbox is !Send + !Sync, preventing
        // cross-thread access. The RefCell borrows are checked at runtime.
        MAILBOX.with(|cell| {
            // Leak a reference so we can return &'static
            let ptr: *const Mailbox = &*cell.borrow() as *const Mailbox;
            // SAFETY: the RefCell lives for the thread's lifetime; we extend
            // to 'static which is safe because Mailbox is !Send + !Sync.
            unsafe { &*ptr }
        })
    }

    /// Push a message into a recipient's queue.
    fn push(&self, recipient: &str, msg: AgentMessage) {
        let mut guard = self.messages.lock().unwrap();
        guard.entry(recipient.to_string()).or_default().push(msg);
    }

    /// Retrieve and drain all messages for a recipient.
    #[allow(dead_code)]
    fn drain(&self, recipient: &str) -> Vec<AgentMessage> {
        let mut guard = self.messages.lock().unwrap();
        guard.entry(recipient.to_string()).or_default().drain(..).collect()
    }

    /// Check if a recipient has any messages queued.
    #[allow(dead_code)]
    fn has_messages(&self, recipient: &str) -> bool {
        let guard = self.messages.lock().unwrap();
        guard.get(recipient).map(|v| !v.is_empty()).unwrap_or(false)
    }

    /// List all known teammate names currently in the mailbox.
    fn teammates(&self) -> Vec<String> {
        let guard = self.messages.lock().unwrap();
        guard.keys().cloned().collect()
    }

    /// Clear all messages (useful in tests).
    #[cfg(test)]
    fn clear(&self) {
        let mut guard = self.messages.lock().unwrap();
        guard.clear();
    }
}

/// Deserializable input for SendMessageTool
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct SendMessageInput {
    /// Recipient: teammate name, "all" (broadcast), or "team-lead"
    to: String,
    /// Text content of the message
    message: String,
    /// Message type controlling protocol semantics
    #[serde(default = "default_message_type")]
    message_type: String,
    /// Optional request ID for structured protocol messages
    #[serde(default)]
    request_id: Option<String>,
}

fn default_message_type() -> String {
    "plain".to_string()
}

/// SendMessageTool: delivers a message to a teammate or broadcasts to all.
pub struct SendMessageTool {
    /// Sender identity filled in at call time from ToolContext.agent_id
    _priv: (),
}

impl SendMessageTool {
    pub fn new() -> Self {
        Self { _priv: () }
    }

    fn deliver(
        &self,
        from: &str,
        input: &SendMessageInput,
    ) -> String {
        let msg = AgentMessage {
            from: from.to_string(),
            to: input.to.clone(),
            message: input.message.clone(),
            message_type: input.message_type.clone(),
            request_id: input.request_id.clone(),
            timestamp: chrono::Utc::now().timestamp_millis(),
        };

        let mailbox = Mailbox::global();

        if input.to == "all" {
            // Broadcast: push a copy to every known teammate plus "team-lead"
            let teammates: Vec<String> = mailbox.teammates();
            let delivered: Vec<&str> = teammates
                .iter()
                .map(|s| s.as_str())
                .filter(|&name| name != from) // don't echo to self
                .collect();

            if delivered.is_empty() {
                // No teammates registered yet — store under the "all" sentinel
                mailbox.push("all", msg.clone());
                return "Broadcast queued for future teammates. No current teammates registered.".to_string();
            }

            for name in &delivered {
                mailbox.push(name, msg.clone());
            }
            format!(
                "Message broadcast to {} teammate(s): {}",
                delivered.len(),
                delivered.join(", ")
            )
        } else {
            mailbox.push(&input.to, msg);
            format!("Message delivered to '{}'.", input.to)
        }
    }

    fn input_schema() -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "to": {
                    "type": "string",
                    "description": "Recipient name, or 'all' to broadcast, or 'team-lead'"
                },
                "message": {
                    "type": "string",
                    "description": "Text content of the message"
                },
                "message_type": {
                    "type": "string",
                    "description": "Message type: plain | shutdown_request | plan_response | plan_approval_response",
                    "default": "plain"
                },
                "request_id": {
                    "type": "string",
                    "description": "Optional request ID for structured protocol messages"
                }
            },
            "required": ["to", "message"]
        })
    }
}

impl Default for SendMessageTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for SendMessageTool {
    fn name(&self) -> &str {
        "SendMessage"
    }

    fn description(&self) -> String {
        "Sends a message to a teammate, the team-lead, or broadcasts to all teammates. \
         Messages are queued in memory and can be retrieved by the recipient. \
         Use message_type to signal structured protocol messages such as \
         'shutdown_request', 'plan_response', or 'plan_approval_response'."
            .to_string()
    }

    fn input_schema(&self) -> serde_json::Value {
        Self::input_schema()
    }

    fn is_read_only(&self) -> bool {
        false
    }

    async fn call(
        &self,
        args: serde_json::Value,
        context: ToolContext,
    ) -> Result<ToolResult, CliError> {
        let input: SendMessageInput = serde_json::from_value(args)
            .map_err(|e| CliError::ToolExecution(format!("invalid input: {e}")))?;

        let confirmation = self.deliver(&context.agent_id, &input);

        Ok(ToolResult::text(confirmation))
    }

    fn render_use_message(&self, args: &serde_json::Value) -> String {
        if let Ok(input) = serde_json::from_value::<SendMessageInput>(args.clone()) {
            let preview = if input.message.len() > 60 {
                format!("{}...", &input.message[..60])
            } else {
                input.message.clone()
            };
            format!("Sending message to '{}': {}", input.to, preview)
        } else {
            "Sending a message".to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_mailbox() -> &'static Mailbox {
        Mailbox::global()
    }

    #[test]
    fn test_send_to_specific_teammate() {
        let mb = fresh_mailbox();
        mb.clear();

        let tool = SendMessageTool::new();
        let input = SendMessageInput {
            to: "coder".to_string(),
            message: "Please refactor the auth module".to_string(),
            message_type: "plain".to_string(),
            request_id: None,
        };

        let result = tool.deliver("team-lead", &input);
        assert!(result.contains("coder"));
        assert!(result.contains("delivered"));

        let messages = mb.drain("coder");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].from, "team-lead");
        assert_eq!(messages[0].to, "coder");
        assert_eq!(messages[0].message_type, "plain");
    }

    #[test]
    fn test_broadcast_to_all() {
        let mb = fresh_mailbox();
        mb.clear();

        // Pre-register teammates by pushing a sentinel message to each
        mb.push("alice", AgentMessage { from: String::new(), to: String::new(), message: String::new(), message_type: String::new(), request_id: None, timestamp: 0 });
        mb.push("bob", AgentMessage { from: String::new(), to: String::new(), message: String::new(), message_type: String::new(), request_id: None, timestamp: 0 });

        let tool = SendMessageTool::new();
        let input = SendMessageInput {
            to: "all".to_string(),
            message: "Deploy in 5 minutes".to_string(),
            message_type: "plain".to_string(),
            request_id: None,
        };

        let result = tool.deliver("team-lead", &input);
        assert!(result.contains("broadcast"));
        assert!(result.contains("alice") && result.contains("bob"));

        // Neither alice nor bob should get an echo from themselves.
        // Each teammate receives: 1 sentinel (pre-inserted) + 1 broadcast = 2.
        let alice_msgs = mb.drain("alice");
        let bob_msgs = mb.drain("bob");
        assert_eq!(alice_msgs.len(), 2, "alice: sentinel + broadcast");
        assert_eq!(bob_msgs.len(), 2, "bob: sentinel + broadcast");
        assert_eq!(alice_msgs[1].from, "team-lead");
        assert_eq!(bob_msgs[1].from, "team-lead");
    }

    #[test]
    fn test_structured_message_types() {
        let mb = fresh_mailbox();
        mb.clear();

        let tool = SendMessageTool::new();

        let shutdown = SendMessageInput {
            to: "worker-1".to_string(),
            message: "please stop".to_string(),
            message_type: "shutdown_request".to_string(),
            request_id: Some("req-42".to_string()),
        };

        tool.deliver("team-lead", &shutdown);

        let msgs = mb.drain("worker-1");
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].message_type, "shutdown_request");
        assert_eq!(msgs[0].request_id.as_deref(), Some("req-42"));
    }

    #[test]
    fn test_message_timestamp() {
        let mb = fresh_mailbox();
        mb.clear();

        let tool = SendMessageTool::new();
        let before = chrono::Utc::now().timestamp_millis();

        tool.deliver("sender", &SendMessageInput {
            to: "receiver".to_string(),
            message: "hello".to_string(),
            message_type: "plain".to_string(),
            request_id: None,
        });

        let after = chrono::Utc::now().timestamp_millis();
        let msgs = mb.drain("receiver");
        assert_eq!(msgs.len(), 1);
        assert!(msgs[0].timestamp >= before);
        assert!(msgs[0].timestamp <= after);
    }
}
