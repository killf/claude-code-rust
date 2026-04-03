//! Agent execution context

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use super::hooks::Hook;
use super::lsp::LspBackend;
use crate::config::{GlobalConfig, ProjectConfig};
use crate::session::SessionStorage;
use crate::types::{PermissionMode, Session, SessionConfig, Tool};

/// Context for agent execution
pub struct AgentContext {
    pub session: Session,
    pub config: SessionConfig,
    pub tools: Vec<Arc<dyn Tool>>,
    pub working_directory: PathBuf,
    pub env: HashMap<String, String>,
    pub global_config: GlobalConfig,
    pub project_config: ProjectConfig,
    pub session_storage: SessionStorage,
    pub hooks: Vec<Hook>,
    pub lsp_backend: Option<Arc<dyn LspBackend>>,
}

impl AgentContext {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        session: Session,
        config: SessionConfig,
        tools: Vec<Arc<dyn Tool>>,
        working_directory: PathBuf,
        global_config: GlobalConfig,
        project_config: ProjectConfig,
        env: std::collections::HashMap<String, String>,
        lsp_backend: Option<Arc<dyn LspBackend>>,
    ) -> Self {
        let hooks = super::hooks::load_hooks_from_config(&global_config.hooks);
        Self {
            session,
            config,
            tools,
            working_directory,
            env,
            global_config,
            project_config,
            session_storage: SessionStorage::new(),
            hooks,
            lsp_backend,
        }
    }

    pub fn tool_names(&self) -> Vec<String> {
        self.tools.iter().map(|t| t.name().to_string()).collect()
    }

    pub fn find_tool(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.iter().find(|t| t.name() == name || t.aliases().contains(&name.to_string())).cloned()
    }

    pub fn model(&self) -> &str {
        &self.session.model
    }

    pub fn permission_mode(&self) -> PermissionMode {
        self.config.permission_mode
    }

    pub fn hooks_of_type(&self, hook_type: super::hooks::HookType) -> Vec<&Hook> {
        self.hooks.iter().filter(|h| h.hook_type == hook_type).collect()
    }
}
