//! Tool registry

use std::collections::HashMap;
use std::sync::Arc;

use crate::types::Tool;

pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
    aliases: HashMap<String, String>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
            aliases: HashMap::new(),
        }
    }

    /// Register a tool
    pub fn register(&mut self, tool: impl Tool + 'static) {
        let name = tool.name().to_string();
        let arc: Arc<dyn Tool> = Arc::new(tool);
        self.tools.insert(name.clone(), Arc::clone(&arc));

        for alias in arc.aliases() {
            self.aliases.insert(alias, name.clone());
        }
    }

    /// Get a tool by name or alias
    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools
            .get(name)
            .cloned()
            .or_else(|| {
                self.aliases
                    .get(name)
                    .and_then(|n| self.tools.get(n).cloned())
            })
    }

    /// Get all registered tools
    pub fn get_all(&self) -> Vec<Arc<dyn Tool>> {
        self.tools.values().cloned().collect()
    }

    /// Filter tools by allowed/denied lists
    pub fn filter(&self, allowed: &[String], denied: &[String]) -> Vec<Arc<dyn Tool>> {
        self.tools
            .values()
            .filter(|t| {
                let name = t.name();
                if denied.iter().any(|d| d == name || d == "*") {
                    return false;
                }
                if allowed.is_empty() || allowed.iter().any(|a| a == name || a == "*") {
                    return true;
                }
                false
            })
            .cloned()
            .collect()
    }

    /// Get tool names
    pub fn names(&self) -> Vec<String> {
        self.tools.keys().cloned().collect()
    }

    /// Register all built-in tools
    pub fn register_builtins() -> Self {
        let mut registry = Self::new();

        registry.register(super::bash::BashTool::new());
        registry.register(super::file_read::FileReadTool::new());
        registry.register(super::file_write::FileWriteTool::new());
        registry.register(super::file_edit::FileEditTool::new());
        registry.register(super::grep::GrepTool::new());
        registry.register(super::glob::GlobTool::new());
        registry.register(super::task_tool::TaskTool::new(std::env::temp_dir()));
        registry.register(super::web_fetch::WebFetchTool::new());
        registry.register(super::web_search::WebSearchTool::new());
        registry.register(super::send_user_message::SendUserMessageTool::new());
        registry.register(super::send_message::SendMessageTool::new());
        registry.register(super::ask_question::AskQuestionTool::new());
        registry.register(super::config_tool::ConfigTool::new());

        registry
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
