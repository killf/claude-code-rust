//! Bootstrap and initialization logic
//!
//! Mirrors TypeScript's layered startup:
//!   bootstrap-entry.ts  → main.tsx  →  init()  →  REPL
//!
//! The Rust equivalent:
//!   main.rs            →  Bootstrap::load()  →  inject_config_env()
//!   → setup_graceful_shutdown()  →  Bootstrap::resolve_auth()  →  REPL

use std::collections::HashMap;
use crate::config::{ConfigLoader, McpServerConfig};
use crate::error::CliError;
use crate::session::SessionStorage;
use crate::types::permission::PermissionMode;

pub struct Bootstrap {
    pub api_key: String,
    pub provider: crate::config::ModelProvider,
    pub base_url: String,
    pub permission_mode: PermissionMode,
    pub session_storage: SessionStorage,
    pub global_config: crate::config::GlobalConfig,
    pub project_config: crate::config::ProjectConfig,
    /// Extra environment variables from --add-env
    pub extra_env: HashMap<String, String>,
    pub system_prompt: Option<String>,
    pub extra_mcp_servers: Option<HashMap<String, McpServerConfig>>,
}

impl Bootstrap {
    /// Phase A: Load configs only. No network requests (no auth).
    ///
    /// After this returns, the caller should:
    ///   1. inject_config_env(&bootstrap.global_config.env)
    ///   2. setup_graceful_shutdown()
    ///   3. initialize_warning_handler()
    ///   4. init_sinks()
    ///   5. register_cleanup(...)
    ///   6. bootstrap.resolve_auth().await
    pub async fn load(
        _model: Option<String>,
        permission_mode_arg: Option<String>,
        dangerously_skip_permission: Option<String>,
        add_env_arg: Vec<String>,
        system_prompt_arg: Option<String>,
        mcp_config_arg: Option<String>,
        verbose: bool,
    ) -> Result<Self, CliError> {
        let config_loader = ConfigLoader::new();

        // Load global config (~/.claude/settings.json)
        let global_config = config_loader.load_global_config().await
            .map_err(|e| CliError::Config(format!("failed to load global config: {e}")))?;

        // Load project config (.claude.json / .clauderc)
        let project_config = config_loader.load_project_config(None).await
            .map_err(|e| CliError::Config(format!("failed to load project config: {e}")))?;

        // Parse --add-env arguments
        let extra_env: HashMap<String, String> = add_env_arg
            .iter()
            .filter_map(|kv| {
                let mut parts = kv.splitn(2, '=');
                match (parts.next(), parts.next()) {
                    (Some(k), Some(v)) => Some((k.to_string(), v.to_string())),
                    _ => None,
                }
            })
            .collect();

        // System prompt: CLI arg overrides config
        let system_prompt = system_prompt_arg.or_else(|| {
            global_config.model_preferences.system_prompt.clone()
        });

        // MCP config: CLI arg overrides config
        let extra_mcp_servers: Option<HashMap<String, McpServerConfig>> = if let Some(json_str) = mcp_config_arg {
            match serde_json::from_str(&json_str) {
                Ok(servers) => Some(servers),
                Err(e) => {
                    eprintln!("[Warning] Failed to parse --mcp-config: {e}");
                    None
                }
            }
        } else {
            None
        };

        let provider = global_config.model_preferences.provider;

        // Resolve permission mode
        let permission_mode = resolve_permission_mode(
            permission_mode_arg,
            dangerously_skip_permission.clone(),
            &global_config,
        );

        let session_storage = SessionStorage::new();

        if verbose {
            eprintln!("Bootstrap load complete:");
            eprintln!("  provider: {:?}", provider);
            eprintln!("  permission_mode: {:?}", permission_mode);
            eprintln!("  config env vars: {} keys", global_config.env.len());
        }

        // api_key and base_url are left empty — resolve_auth() fills them.
        Ok(Self {
            api_key: String::new(),
            provider,
            base_url: String::new(),
            permission_mode,
            session_storage,
            global_config,
            project_config,
            extra_env,
            system_prompt,
            extra_mcp_servers,
        })
    }

    /// Phase B: Resolve auth (API key + base URL).
    ///
    /// MUST be called AFTER inject_config_env() so that:
    ///   - ANTHROPIC_AUTH_TOKEN from global_config.env is visible via std::env::var()
    ///   - ANTHROPIC_BASE_URL from global_config.env is visible to get_base_url()
    pub async fn resolve_auth(&mut self) -> Result<(), CliError> {
        // resolve_api_key checks ANTHROPIC_AUTH_TOKEN first (Bearer token priority).
        self.api_key = crate::api::auth::resolve_api_key(
            self.provider,
            self.global_config.model_preferences.api_key.as_deref(),
        )
        .await?;

        // get_base_url checks ANTHROPIC_BASE_URL env var (injected from settings.json.env).
        self.base_url = crate::api::auth::get_base_url(
            self.provider,
            self.global_config.model_preferences.base_url.as_deref(),
        );

        Ok(())
    }
}

/// Resolve permission mode from CLI arg and config
fn resolve_permission_mode(
    arg: Option<String>,
    dangerously_skip_permission: Option<String>,
    global_config: &crate::config::GlobalConfig,
) -> PermissionMode {
    // --dangerously-skip-permission takes precedence
    if dangerously_skip_permission.is_some() {
        return PermissionMode::BypassPermissions;
    }

    if let Some(mode_str) = arg {
        return mode_str_to_permission_mode(&mode_str);
    }

    mode_str_to_permission_mode(&global_config.permission_mode.to_string())
}

fn mode_str_to_permission_mode(s: &str) -> PermissionMode {
    match s.to_lowercase().as_str() {
        "accept-edits" | "accept_edits" => PermissionMode::AcceptEdits,
        "bypass-permissions" | "bypass_permissions" | "bypass" => PermissionMode::BypassPermissions,
        "plan" => PermissionMode::Plan,
        "dont-ask" | "dont_ask" => PermissionMode::DontAsk,
        "auto" => PermissionMode::Auto,
        _ => PermissionMode::Default,
    }
}
