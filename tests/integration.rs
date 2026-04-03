//! Integration tests for Claude Code library

use open_cc::error::CliError;
use open_cc::session::compaction::{CompactionConfig, SessionCompactor};
use open_cc::types::{
    ContentBlock, Message, PermissionMode, Session, SessionConfig, TokenUsage, UserContent,
};

#[test]
fn test_session_new() {
    let session = Session::new("test-123".to_string(), "claude-opus-4-5".to_string());
    assert_eq!(session.id, "test-123");
    assert_eq!(session.model, "claude-opus-4-5");
    assert!(session.messages.is_empty());
    assert_eq!(session.cost, 0.0);
}

#[test]
fn test_session_add_message() {
    let mut session = Session::new("test-123".to_string(), "claude-opus-4-5".to_string());
    session.add_message(Message::User {
        content: UserContent::text("Hello"),
    });
    assert_eq!(session.messages.len(), 1);
}

#[test]
fn test_session_token_tracking() {
    let mut session = Session::new("test-123".to_string(), "claude-opus-4-5".to_string());
    session.token_usage.input_tokens = 100;
    session.token_usage.output_tokens = 50;
    assert_eq!(session.input_tokens(), 100);
    assert_eq!(session.output_tokens(), 50);
    assert_eq!(session.total_tokens(), 150);
}

#[test]
fn test_session_config_default() {
    let config = SessionConfig::default();
    assert_eq!(config.model, "claude-opus-4-5");
    assert_eq!(config.max_tokens, Some(8192));
}

#[test]
fn test_error_display() {
    let err = CliError::ApiKeyNotFound;
    let display = format!("{err}");
    assert!(!display.is_empty());
}

#[test]
fn test_message_serialization() {
    let msg = Message::User {
        content: UserContent::text("Hello, world!"),
    };
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("Hello"));
    let deserialized: Message = serde_json::from_str(&json).unwrap();
    assert!(matches!(deserialized, Message::User { .. }));
}

#[test]
fn test_user_content_text() {
    let content = UserContent::text("Test message");
    assert_eq!(content.content.len(), 1);
    assert!(matches!(&content.content[0], open_cc::types::ContentBlock::Text { .. }));
}

#[test]
fn test_tool_registry_builtins() {
    use open_cc::tools::ToolRegistry;

    let registry = ToolRegistry::register_builtins();
    let tools = registry.get_all();
    assert!(!tools.is_empty(), "should register built-in tools");

    let names: Vec<_> = registry.names();
    assert!(names.iter().any(|n| n == "Bash"), "should have Bash tool");
    assert!(names.iter().any(|n| n == "Glob"), "should have Glob tool");
    assert!(names.iter().any(|n| n == "Grep"), "should have Grep tool");
}

#[test]
fn test_tool_registry_get_by_name() {
    use open_cc::tools::ToolRegistry;

    let registry = ToolRegistry::register_builtins();
    let bash = registry.get("Bash");
    assert!(bash.is_some(), "should find Bash by name");
    assert_eq!(bash.unwrap().name(), "Bash");
}

#[test]
fn test_tool_registry_get_by_alias() {
    use open_cc::tools::ToolRegistry;

    let registry = ToolRegistry::register_builtins();
    let shell = registry.get("shell");
    assert!(shell.is_some(), "should find Bash by alias 'shell'");
    assert_eq!(shell.unwrap().name(), "Bash");
}

#[test]
fn test_tool_registry_filter_allowed() {
    use open_cc::tools::ToolRegistry;

    let registry = ToolRegistry::register_builtins();
    let filtered = registry.filter(&["Bash".to_string()], &[]);
    assert_eq!(filtered.len(), 1, "should filter to only Bash");
    assert_eq!(filtered[0].name(), "Bash");
}

#[test]
fn test_tool_registry_filter_denied() {
    use open_cc::tools::ToolRegistry;

    let registry = ToolRegistry::register_builtins();
    let filtered = registry.filter(&[], &["Bash".to_string()]);
    let names: Vec<_> = filtered.iter().map(|t| t.name()).collect();
    assert!(!names.contains(&"Bash"), "Bash should be denied");
}

#[test]
fn test_tool_registry_filter_wildcard() {
    use open_cc::tools::ToolRegistry;

    let registry = ToolRegistry::register_builtins();
    let filtered = registry.filter(&[], &["*".to_string()]);
    assert!(filtered.is_empty(), "wildcard deny should block all tools");
}

#[test]
fn test_session_with_permission_mode() {
    use open_cc::types::PermissionMode;

    let config = SessionConfig::default().with_permission_mode(PermissionMode::BypassPermissions);
    assert_eq!(config.permission_mode, PermissionMode::BypassPermissions);
}

#[test]
fn test_message_tool_result_serialization() {
    let msg = Message::ToolResult {
        tool_use_id: "tool_123".to_string(),
        content: "Result content".to_string(),
        is_error: false,
    };
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("tool_result"));
    assert!(json.contains("tool_123"));
    let deserialized: Message = serde_json::from_str(&json).unwrap();
    assert!(matches!(deserialized, Message::ToolResult { .. }));
}

#[test]
fn test_content_block_tool_use() {
    let block = ContentBlock::ToolUse {
        id: "tool_abc".to_string(),
        name: "Bash".to_string(),
        input: serde_json::json!({"command": "ls"}),
    };
    let json = serde_json::to_string(&block).unwrap();
    assert!(json.contains("tool_use"));
    assert!(json.contains("tool_abc"));
}

#[test]
fn test_api_tool_creation() {
    use open_cc::tools::ToolRegistry;

    // Verify the API tool schema is correct
    let registry = ToolRegistry::register_builtins();
    let bash = registry.get("Bash").unwrap();
    let schema = bash.input_schema();
    assert!(schema.get("type").is_some(), "schema should have type field");
    assert!(schema.get("properties").is_some(), "schema should have properties");
}

#[test]
fn test_message_hooks_and_progress() {
    let msg = Message::Progress {
        data: open_cc::types::ProgressData {
            message: Some("Thinking...".to_string()),
            progress: Some(0.5),
        },
    };
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("progress"));
    assert!(json.contains("Thinking"));
}

#[test]
fn test_session_cost_tracking() {
    let mut session = Session::new("test-123".to_string(), "claude-opus-4-5".to_string());
    assert_eq!(session.cost, 0.0);
    session.cost += 0.05;
    assert_eq!(session.cost, 0.05);
}

#[test]
fn test_session_messages_full_flow() {
    let mut session = Session::new("test-flow".to_string(), "claude-opus-4-5".to_string());

    session.add_message(Message::User {
        content: UserContent::text("Hello"),
    });
    session.add_message(Message::Assistant {
        content: Some(open_cc::types::AssistantContent {
            content: vec![ContentBlock::Text { text: "Hi there!".to_string() }],
            model: "claude-opus-4-5".to_string(),
            stop_reason: Some("end_turn".to_string()),
        }),
    });
    session.add_message(Message::ToolResult {
        tool_use_id: "tool_1".to_string(),
        content: "done".to_string(),
        is_error: false,
    });

    assert_eq!(session.messages.len(), 3);
}

// ─────────────────────────────────────────────────────────────────────────────
// Session round-trip serialization
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_session_full_roundtrip() {
    let mut session = Session::new("roundtrip-123".to_string(), "claude-sonnet-4".to_string());
    session.system_prompt = Some("You are helpful.".to_string());
    session.cost = 1.23;
    session.token_usage.input_tokens = 500;
    session.token_usage.output_tokens = 200;
    session.token_usage.cache_read_tokens = 100;

    session.add_message(Message::User {
        content: UserContent::text("Hello"),
    });
    session.add_message(Message::Assistant {
        content: Some(open_cc::types::AssistantContent {
            content: vec![ContentBlock::Text {
                text: "Hi!".to_string(),
            }],
            model: "claude-sonnet-4".to_string(),
            stop_reason: Some("end_turn".to_string()),
        }),
    });
    session.add_message(Message::ToolUse {
        id: "tool_x".to_string(),
        name: "Read".to_string(),
        input: serde_json::json!({"path": "foo.txt"}),
    });

    let json = serde_json::to_string(&session).unwrap();
    let deserialized: Session = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.id, "roundtrip-123");
    assert_eq!(deserialized.model, "claude-sonnet-4");
    assert_eq!(deserialized.system_prompt, Some("You are helpful.".to_string()));
    assert_eq!(deserialized.cost, 1.23);
    assert_eq!(deserialized.token_usage.input_tokens, 500);
    assert_eq!(deserialized.token_usage.output_tokens, 200);
    assert_eq!(deserialized.token_usage.cache_read_tokens, 100);
    assert_eq!(deserialized.messages.len(), 3);
}

// ─────────────────────────────────────────────────────────────────────────────
// Message serialization
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_message_user_roundtrip() {
    let msg = Message::User {
        content: UserContent::text("test input"),
    };
    let json = serde_json::to_string(&msg).unwrap();
    let back: Message = serde_json::from_str(&json).unwrap();
    assert!(matches!(back, Message::User { .. }));
}

#[test]
fn test_message_assistant_roundtrip() {
    let msg = Message::Assistant {
        content: Some(open_cc::types::AssistantContent {
            content: vec![ContentBlock::Text {
                text: "assistant reply".to_string(),
            }],
            model: "claude-opus-4-5".to_string(),
            stop_reason: Some("end_turn".to_string()),
        }),
    };
    let json = serde_json::to_string(&msg).unwrap();
    let back: Message = serde_json::from_str(&json).unwrap();
    assert!(matches!(back, Message::Assistant { .. }));
}

#[test]
fn test_message_system_roundtrip() {
    let msg = Message::System {
        subtype: "session_compaction".to_string(),
        level: None,
        message: "Summarised conversation".to_string(),
    };
    let json = serde_json::to_string(&msg).unwrap();
    let back: Message = serde_json::from_str(&json).unwrap();
    assert!(matches!(back, Message::System { .. }));
}

#[test]
fn test_message_progress_roundtrip() {
    let msg = Message::Progress {
        data: open_cc::types::ProgressData {
            message: Some("Thinking...".to_string()),
            progress: Some(0.75),
        },
    };
    let json = serde_json::to_string(&msg).unwrap();
    let back: Message = serde_json::from_str(&json).unwrap();
    assert!(matches!(back, Message::Progress { .. }));
}

#[test]
fn test_message_tombstone_roundtrip() {
    let msg = Message::Tombstone;
    let json = serde_json::to_string(&msg).unwrap();
    let back: Message = serde_json::from_str(&json).unwrap();
    assert!(matches!(back, Message::Tombstone));
}

// ─────────────────────────────────────────────────────────────────────────────
// ToolResult round-trip
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_tool_result_roundtrip() {
    use open_cc::types::ToolResult;

    let result = ToolResult::error("something went wrong".to_string());
    let json = serde_json::to_string(&result).unwrap();
    assert!(json.contains("error"));
    let back: ToolResult = serde_json::from_str(&json).unwrap();
    assert!(back.is_error);
    assert_eq!(back.content.len(), 1);
}

#[test]
fn test_tool_result_success_roundtrip() {
    use open_cc::types::ToolResult;

    let result = ToolResult::text("file contents here");
    let json = serde_json::to_string(&result).unwrap();
    let back: ToolResult = serde_json::from_str(&json).unwrap();
    assert!(!back.is_error);
}

// ─────────────────────────────────────────────────────────────────────────────
// TokenUsage
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_token_usage_add() {
    let mut usage = TokenUsage {
        input_tokens: 100,
        output_tokens: 50,
        cache_creation_tokens: 0,
        cache_read_tokens: 0,
    };
    usage.add(&TokenUsage {
        input_tokens: 20,
        output_tokens: 10,
        cache_creation_tokens: 5,
        cache_read_tokens: 15,
    });
    assert_eq!(usage.input_tokens, 120);
    assert_eq!(usage.output_tokens, 60);
    assert_eq!(usage.cache_creation_tokens, 5);
    assert_eq!(usage.cache_read_tokens, 15);
    assert_eq!(usage.total(), 200);
}

#[test]
fn test_token_usage_total_includes_cache() {
    let usage = TokenUsage {
        input_tokens: 100,
        output_tokens: 50,
        cache_creation_tokens: 30,
        cache_read_tokens: 20,
    };
    assert_eq!(usage.total(), 200);
}

// ─────────────────────────────────────────────────────────────────────────────
// Session compaction
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_compaction_noop_when_below_threshold() {
    let compactor = SessionCompactor::new(CompactionConfig::default());
    let usage = TokenUsage {
        input_tokens: 1000,
        output_tokens: 500,
        cache_creation_tokens: 0,
        cache_read_tokens: 0,
    };
    assert!(!compactor.should_compact(&[], &usage));
}

#[test]
fn test_compaction_triggers_at_threshold() {
    let compactor = SessionCompactor::new(CompactionConfig::default());
    // threshold = 200000 * 0.80 = 160000
    let usage = TokenUsage {
        input_tokens: 160_000,
        output_tokens: 0,
        cache_creation_tokens: 0,
        cache_read_tokens: 0,
    };
    assert!(compactor.should_compact(&[], &usage));
}

#[test]
fn test_compaction_replaces_old_messages_with_summary() {
    let compactor = SessionCompactor::new(CompactionConfig::default());
    let mut messages = Vec::new();

    // Add 8+ messages so compaction triggers (requires len >= 6)
    for i in 0..8 {
        messages.push(Message::User {
            content: UserContent::text(format!("message {i}")),
        });
    }

    let original_len = messages.len();
    let result = compactor.compact(&mut messages);

    assert!(result, "compaction should return true");
    assert!(
        messages.len() < original_len,
        "should have fewer messages after compaction"
    );
    // First message should now be a system/summary message
    assert!(matches!(messages[0], Message::System { .. }));
}

#[test]
fn test_compaction_noop_when_too_few_messages() {
    let compactor = SessionCompactor::new(CompactionConfig::default());
    let mut messages = vec![
        Message::User {
            content: UserContent::text("first"),
        },
        Message::Assistant {
            content: Some(open_cc::types::AssistantContent {
                content: vec![ContentBlock::Text {
                    text: "reply".to_string(),
                }],
                model: "claude-opus-4-5".to_string(),
                stop_reason: None,
            }),
        },
    ];

    let result = compactor.compact(&mut messages);
    assert!(!result, "should not compact when fewer than 6 messages");
    assert_eq!(messages.len(), 2);
}

// ─────────────────────────────────────────────────────────────────────────────
// PermissionMode
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_permission_mode_serialize_roundtrip() {
    let modes = [
        PermissionMode::Default,
        PermissionMode::AcceptEdits,
        PermissionMode::BypassPermissions,
        PermissionMode::Plan,
        PermissionMode::DontAsk,
        PermissionMode::Auto,
        PermissionMode::Bubble,
    ];
    for mode in modes {
        let json = serde_json::to_string(&mode).unwrap();
        let back: PermissionMode = serde_json::from_str(&json).unwrap();
        assert_eq!(mode, back);
    }
}

#[test]
fn test_permission_mode_from_str() {
    assert_eq!(
        "acceptEdits".parse::<PermissionMode>().unwrap(),
        PermissionMode::AcceptEdits
    );
    assert_eq!(
        "bypassPermissions".parse::<PermissionMode>().unwrap(),
        PermissionMode::BypassPermissions
    );
    assert_eq!(
        "plan".parse::<PermissionMode>().unwrap(),
        PermissionMode::Plan
    );
    assert_eq!(
        "auto".parse::<PermissionMode>().unwrap(),
        PermissionMode::Auto
    );
    assert!("invalid_mode".parse::<PermissionMode>().is_err());
}

#[test]
fn test_permission_mode_display() {
    assert_eq!(PermissionMode::AcceptEdits.to_string(), "acceptEdits");
    assert_eq!(PermissionMode::BypassPermissions.to_string(), "bypassPermissions");
    assert_eq!(PermissionMode::Plan.to_string(), "plan");
}

// ─────────────────────────────────────────────────────────────────────────────
// Tool registry
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_tool_registry_has_all_nine_tools() {
    use open_cc::tools::ToolRegistry;

    let registry = ToolRegistry::register_builtins();
    let names: Vec<_> = registry.names();

    let expected = ["Bash", "Glob", "Grep", "Read", "Write", "Edit", "WebFetch", "WebSearch", "Task"];
    for tool in expected {
        assert!(
            names.iter().any(|n| n == tool),
            "missing tool: {tool}"
        );
    }
    assert_eq!(names.len(), 13, "should have exactly 13 built-in tools");
}

#[test]
fn test_tool_schema_has_required_fields() {
    use open_cc::tools::ToolRegistry;

    let registry = ToolRegistry::register_builtins();
    for tool in registry.get_all() {
        let schema = tool.input_schema();
        assert!(
            schema.get("type").is_some(),
            "{}: schema missing 'type'",
            tool.name()
        );
        assert!(
            schema.get("properties").is_some() || schema.get("oneOf").is_some(),
            "{}: schema missing 'properties' or 'oneOf'",
            tool.name()
        );
        // Schema should be valid JSON
        let schema_str = serde_json::to_string(&schema).unwrap();
        assert!(!schema_str.is_empty(), "{}: schema serialized to empty", tool.name());
    }
}

#[test]
fn test_tool_registry_aliases() {
    use open_cc::tools::ToolRegistry;

    let registry = ToolRegistry::register_builtins();

    // shell is alias for Bash
    let bash = registry.get("shell").expect("shell alias should resolve to Bash");
    assert_eq!(bash.name(), "Bash");

    // glob is alias for Glob
    let glob = registry.get("glob").expect("glob alias should resolve to Glob");
    assert_eq!(glob.name(), "Glob");

    // grep is alias for Grep
    let grep = registry.get("grep").expect("grep alias should resolve to Grep");
    assert_eq!(grep.name(), "Grep");
}

#[test]
fn test_tool_registry_filter_allows_wildcard() {
    use open_cc::tools::ToolRegistry;

    let registry = ToolRegistry::register_builtins();
    let all = registry.filter(&["*".to_string()], &[]);
    assert_eq!(all.len(), 13, "wildcard allow should match all tools");
}

// ─────────────────────────────────────────────────────────────────────────────
// UserContent
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_user_content_text_preview() {
    let content = UserContent::text("This is a long message");
    let preview = content.text_preview();
    assert_eq!(preview, Some("This is a long message".to_string()));
}

#[test]
fn test_user_content_empty() {
    let content = UserContent {
        content: vec![],
    };
    assert_eq!(content.text_preview(), None);
}

// ─────────────────────────────────────────────────────────────────────────────
// Config — GlobalConfig
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_global_config_default() {
    let config = open_cc::config::GlobalConfig::default();
    assert_eq!(config.version, env!("CARGO_PKG_VERSION"));
    assert!(config.auto_compact_enabled);
    assert!(config.todo_feature_enabled);
    assert!(config.mcp_servers.is_empty());
    assert!(config.hooks.is_empty());
    assert!(config.allowed_tools.is_empty());
    assert!(config.denied_tools.is_empty());
}

#[test]
fn test_global_config_serde_roundtrip() {
    use open_cc::config::GlobalConfig;

    let mut config = GlobalConfig::default();
    config.model_preferences.model = Some("claude-sonnet-4".to_string());
    config.permission_mode = PermissionMode::AcceptEdits;
    config.max_tokens = Some(4096);
    config.temperature = Some(0.7);

    let json = serde_json::to_string(&config).unwrap();
    let back: GlobalConfig = serde_json::from_str(&json).unwrap();

    assert_eq!(back.version, config.version);
    assert_eq!(back.model_preferences.model, Some("claude-sonnet-4".to_string()));
    assert_eq!(back.permission_mode, PermissionMode::AcceptEdits);
    assert_eq!(back.max_tokens, Some(4096));
    assert_eq!(back.temperature, Some(0.7));
}

// ─────────────────────────────────────────────────────────────────────────────
// Config — ModelProvider
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_model_provider_display() {
    use open_cc::config::ModelProvider;

    assert_eq!(ModelProvider::Anthropic.to_string(), "anthropic");
    assert_eq!(ModelProvider::AwsBedrock.to_string(), "bedrock");
    assert_eq!(ModelProvider::GcpVertex.to_string(), "vertex");
    assert_eq!(ModelProvider::Ollama.to_string(), "ollama");
    assert_eq!(ModelProvider::Together.to_string(), "together");
}

#[test]
fn test_model_provider_default() {
    use open_cc::config::ModelProvider;

    let mp = ModelProvider::default();
    assert_eq!(mp, ModelProvider::Anthropic);
}

// ─────────────────────────────────────────────────────────────────────────────
// Config — Theme
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_theme_variant_default() {
    use open_cc::config::ThemeVariant;

    let tv = ThemeVariant::default();
    assert_eq!(tv, ThemeVariant::System);
}

#[test]
fn test_theme_variant_serde() {
    use open_cc::config::ThemeVariant;

    let variants = [
        ThemeVariant::Auto,
        ThemeVariant::Dark,
        ThemeVariant::Light,
        ThemeVariant::System,
    ];
    for v in variants {
        let json = serde_json::to_string(&v).unwrap();
        let back: ThemeVariant = serde_json::from_str(&json).unwrap();
        assert_eq!(v, back);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Config — McpServerConfig
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_mcp_server_config_serde() {
    use open_cc::config::{McpServerConfig, McpServerType};
    use std::collections::HashMap;

    let config = McpServerConfig {
        config_type: McpServerType::Stdio,
        command: Some("npx".to_string()),
        args: Some(vec!["-y".to_string(), "@modelcontextprotocol/server-filesystem".to_string()]),
        env: Some(HashMap::from([("DEBUG".to_string(), "1".to_string())])),
        url: None,
        headers: None,
    };

    let json = serde_json::to_string(&config).unwrap();
    assert!(json.contains("stdio"));
    assert!(json.contains("npx"));

    let back: McpServerConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(back.config_type, McpServerType::Stdio);
    assert_eq!(back.command, Some("npx".to_string()));
    assert_eq!(back.args.as_ref().map(|v| v.len()), Some(2));
}

#[test]
fn test_mcp_server_type_all_variants() {
    use open_cc::config::McpServerType;

    let types = [
        McpServerType::Stdio,
        McpServerType::Sse,
        McpServerType::Http,
        McpServerType::Ws,
        McpServerType::Sdk,
    ];
    for t in types {
        let json = serde_json::to_string(&t).unwrap();
        let back: McpServerType = serde_json::from_str(&json).unwrap();
        assert_eq!(t, back);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Config — HookConfig
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_hook_config_serde() {
    use open_cc::config::HookConfig;

    let hook = HookConfig {
        name: "my-hook".to_string(),
        events: vec!["pre_tool_use".to_string()],
        command: "echo test".to_string(),
        working_directory: Some("/tmp".to_string()),
        enabled: true,
    };

    let json = serde_json::to_string(&hook).unwrap();
    let back: HookConfig = serde_json::from_str(&json).unwrap();

    assert_eq!(back.name, "my-hook");
    assert_eq!(back.events, vec!["pre_tool_use"]);
    assert!(back.enabled);
}

// ─────────────────────────────────────────────────────────────────────────────
// Config — LspServerConfig
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_lsp_server_config_serde() {
    use open_cc::config::LspServerConfig;
    use std::collections::HashMap;

    let config = LspServerConfig {
        command: "rust-analyzer".to_string(),
        args: vec![],
        env: HashMap::new(),
    };

    let json = serde_json::to_string(&config).unwrap();
    assert!(json.contains("rust-analyzer"));
    let back: LspServerConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(back.command, "rust-analyzer");
}

// ─────────────────────────────────────────────────────────────────────────────
// MCP Protocol — JSON-RPC Messages
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_mcp_request_serde() {
    use open_cc::mcp::protocol::{McpRequest, McpRequestId};

    let req = McpRequest {
        jsonrpc: "2.0".to_string(),
        id: McpRequestId::Number(1),
        method: "tools/list".to_string(),
        params: Some(serde_json::json!({})),
    };

    let json = serde_json::to_string(&req).unwrap();
    assert!(json.contains("2.0"));
    assert!(json.contains("tools/list"));
    assert!(json.contains("\"id\":1"));

    let back: McpRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(back.method, "tools/list");
}

#[test]
fn test_mcp_request_string_id() {
    use open_cc::mcp::protocol::{McpRequest, McpRequestId};

    let req = McpRequest {
        jsonrpc: "2.0".to_string(),
        id: McpRequestId::String("abc-123".to_string()),
        method: "ping".to_string(),
        params: None,
    };

    let json = serde_json::to_string(&req).unwrap();
    assert!(json.contains("abc-123"));
}

#[test]
fn test_mcp_notification_serde() {
    use open_cc::mcp::protocol::McpNotification;

    let note = McpNotification {
        jsonrpc: "2.0".to_string(),
        method: "notifications/initialized".to_string(),
        params: None,
    };

    let json = serde_json::to_string(&note).unwrap();
    let back: McpNotification = serde_json::from_str(&json).unwrap();
    assert_eq!(back.method, "notifications/initialized");
}

#[test]
fn test_mcp_response_serde() {
    use open_cc::mcp::protocol::{McpResponse, McpRequestId};

    let resp = McpResponse {
        jsonrpc: "2.0".to_string(),
        id: McpRequestId::Number(42),
        result: Some(serde_json::json!({"tools": []})),
        error: None,
    };

    let json = serde_json::to_string(&resp).unwrap();
    let back: McpResponse = serde_json::from_str(&json).unwrap();
    assert!(back.result.is_some());
    assert!(back.error.is_none());
}

#[test]
fn test_mcp_error_serde() {
    use open_cc::mcp::protocol::{McpError, McpErrorDetail, McpRequestId};

    let err = McpError {
        jsonrpc: "2.0".to_string(),
        id: Some(McpRequestId::Number(1)),
        error: McpErrorDetail {
            code: -32600,
            message: "Invalid Request".to_string(),
            data: None,
        },
    };

    let json = serde_json::to_string(&err).unwrap();
    assert!(json.contains("-32600"));
    assert!(json.contains("Invalid Request"));
}

#[test]
fn test_mcp_error_detail_serde() {
    use open_cc::mcp::protocol::McpErrorDetail;

    let detail = McpErrorDetail {
        code: -32601,
        message: "Method not found".to_string(),
        data: Some(serde_json::json!({"method": "unknown"})),
    };

    let json = serde_json::to_string(&detail).unwrap();
    let back: McpErrorDetail = serde_json::from_str(&json).unwrap();
    assert_eq!(back.code, -32601);
    assert!(back.data.is_some());
}

// ─────────────────────────────────────────────────────────────────────────────
// MCP Content Blocks
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_mcp_content_block_text() {
    use open_cc::mcp::McpContentBlock;

    let block = McpContentBlock::Text { text: "hello".to_string() };
    let json = serde_json::to_string(&block).unwrap();
    assert!(json.contains("hello"));
    let back: McpContentBlock = serde_json::from_str(&json).unwrap();
    assert!(matches!(back, McpContentBlock::Text { .. }));
}

#[test]
fn test_mcp_content_block_image() {
    use open_cc::mcp::McpContentBlock;

    let block = McpContentBlock::Image {
        data: "deadbeef".to_string(),
        mime_type: Some("image/png".to_string()),
    };
    let json = serde_json::to_string(&block).unwrap();
    assert!(json.contains("image/png"));
    let back: McpContentBlock = serde_json::from_str(&json).unwrap();
    assert!(matches!(back, McpContentBlock::Image { .. }));
}

#[test]
fn test_mcp_content_block_resource() {
    use open_cc::mcp::McpContentBlock;

    let block = McpContentBlock::Resource {
        resource: open_cc::mcp::McpResource {
            uri: "file:///tmp/test.txt".to_string(),
            mime_type: None,
            text: None,
            blob: None,
        },
    };
    let json = serde_json::to_string(&block).unwrap();
    assert!(json.contains("file:///tmp/test.txt"));
}

#[test]
fn test_mcp_resource_serde() {
    use open_cc::mcp::McpResource;

    let r = McpResource {
        uri: "file:///foo".to_string(),
        mime_type: Some("text/plain".to_string()),
        text: Some("file contents".to_string()),
        blob: None,
    };
    let json = serde_json::to_string(&r).unwrap();
    let back: McpResource = serde_json::from_str(&json).unwrap();
    assert_eq!(back.uri, "file:///foo");
    assert_eq!(back.text, Some("file contents".to_string()));
}

// ─────────────────────────────────────────────────────────────────────────────
// MCP Tool Call
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_mcp_tool_call_result_serde() {
    use open_cc::mcp::protocol::ToolCallResult;

    let result = ToolCallResult {
        content: vec![
            open_cc::mcp::McpContentBlock::Text { text: "output".to_string() },
        ],
        is_error: Some(false),
    };

    let json = serde_json::to_string(&result).unwrap();
    assert!(json.contains("output"));
    let back: ToolCallResult = serde_json::from_str(&json).unwrap();
    assert_eq!(back.is_error, Some(false));
}

#[test]
fn test_mcp_tool_call_result_error() {
    use open_cc::mcp::protocol::ToolCallResult;

    let result = ToolCallResult {
        content: vec![
            open_cc::mcp::McpContentBlock::Text { text: "something went wrong".to_string() },
        ],
        is_error: Some(true),
    };

    let back: ToolCallResult = serde_json::from_str(&serde_json::to_string(&result).unwrap()).unwrap();
    assert_eq!(back.is_error, Some(true));
}

// ─────────────────────────────────────────────────────────────────────────────
// MCP ClientCapabilities
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_mcp_client_capabilities_default() {
    use open_cc::mcp::protocol::ClientCapabilities;

    let caps = ClientCapabilities::default();
    // Default should be empty/default structs
    let json = serde_json::to_string(&caps).unwrap();
    assert!(json.contains("roots") || json.contains("{}"));
}

#[test]
fn test_mcp_client_capabilities_serde() {
    use open_cc::mcp::protocol::{ClientCapabilities, RootsCapability};

    let caps = ClientCapabilities {
        roots: Some(RootsCapability { list_changed: None }),
    };

    // This test compiles if types are correct
    let json = serde_json::to_string(&caps).unwrap();
    assert!(json.contains("roots"));
}

// ─────────────────────────────────────────────────────────────────────────────
// Session — TranscriptManager
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_transcript_manager_basic() {
    use open_cc::session::transcript::TranscriptManager;

    let manager = TranscriptManager::new("test-session");
    let path = manager.transcript_path();
    assert!(path.ends_with("test-session/transcript.ndjson"));
}

// ─────────────────────────────────────────────────────────────────────────────
// Error — CliError
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_cli_error_display() {
    use open_cc::error::CliError;

    let errors = [
        CliError::ApiKeyNotFound,
        CliError::Session("test session error".to_string()),
        CliError::Mcp("mcp error".to_string()),
        CliError::Io(std::io::Error::new(std::io::ErrorKind::NotFound, "not found")),
        CliError::Other("other error".to_string()),
    ];

    for e in errors {
        let display = format!("{e}");
        assert!(!display.is_empty(), "error display should not be empty");
    }
}

#[test]
fn test_cli_error_session_not_found() {
    use open_cc::error::CliError;

    let err = CliError::Session("session not found".to_string());
    let msg = format!("{err}");
    assert!(msg.contains("session not found") || msg.contains("Session"));
}
