//! Global configuration types

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::types::PermissionMode;

/// Global configuration (stored in ~/.claude/settings.json)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalConfig {
    pub version: String,
    pub theme: ThemeSetting,
    pub verbose: bool,
    pub mcp_servers: HashMap<String, McpServerConfig>,
    pub oauth_account: Option<OAuthAccount>,
    pub env: HashMap<String, String>,
    pub auto_compact_enabled: bool,
    pub todo_feature_enabled: bool,
    pub model_preferences: ModelPreferences,
    pub permission_mode: PermissionMode,
    pub hooks: Vec<HookConfig>,
    pub allowed_tools: Vec<String>,
    pub denied_tools: Vec<String>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub uri_open_timeout_ms: Option<u64>,
    pub lsp_servers: HashMap<String, LspServerConfig>,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION").to_string(),
            theme: ThemeSetting::default(),
            verbose: false,
            mcp_servers: HashMap::new(),
            oauth_account: None,
            env: HashMap::new(),
            auto_compact_enabled: true,
            todo_feature_enabled: true,
            model_preferences: ModelPreferences::default(),
            permission_mode: PermissionMode::Default,
            hooks: Vec::new(),
            allowed_tools: Vec::new(),
            denied_tools: Vec::new(),
            max_tokens: None,
            temperature: None,
            uri_open_timeout_ms: Some(30_000),
            lsp_servers: HashMap::new(),
        }
    }
}

/// Theme settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeSetting {
    pub variant: ThemeVariant,
}

impl Default for ThemeSetting {
    fn default() -> Self {
        Self {
            variant: ThemeVariant::System,
        }
    }
}

impl std::fmt::Display for ThemeVariant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Auto => write!(f, "auto"),
            Self::Dark => write!(f, "dark"),
            Self::Light => write!(f, "light"),
            Self::System => write!(f, "system"),
        }
    }
}

impl std::str::FromStr for ThemeVariant {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "auto" => Ok(Self::Auto),
            "dark" => Ok(Self::Dark),
            "light" => Ok(Self::Light),
            "system" => Ok(Self::System),
            other => Err(format!("invalid theme variant: {other}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ThemeVariant {
    Auto,
    Dark,
    Light,
    #[default]
    System,
}

/// MCP server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    #[serde(rename = "type")]
    pub config_type: McpServerType,
    pub command: Option<String>,
    pub args: Option<Vec<String>>,
    pub env: Option<HashMap<String, String>>,
    pub url: Option<String>,
    pub headers: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpServerType {
    Stdio,
    Sse,
    Http,
    Ws,
    Sdk,
}

/// Model provider preferences
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPreferences {
    pub provider: ModelProvider,
    pub model: Option<String>,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub system_prompt: Option<String>,
}

impl Default for ModelPreferences {
    fn default() -> Self {
        Self {
            provider: ModelProvider::Anthropic,
            model: None,
            api_key: None,
            base_url: None,
            system_prompt: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ModelProvider {
    #[default]
    Anthropic,
    AwsBedrock,
    GcpVertex,
    Azure,
    OpenAi,
    Ollama,
    Together,
}

impl std::fmt::Display for ModelProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Anthropic => write!(f, "anthropic"),
            Self::AwsBedrock => write!(f, "bedrock"),
            Self::GcpVertex => write!(f, "vertex"),
            Self::Azure => write!(f, "azure"),
            Self::OpenAi => write!(f, "openai"),
            Self::Ollama => write!(f, "ollama"),
            Self::Together => write!(f, "together"),
        }
    }
}

/// OAuth account information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthAccount {
    pub provider: String,
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<i64>,
}

/// Hook configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookConfig {
    pub name: String,
    pub events: Vec<String>,
    pub command: String,
    pub working_directory: Option<String>,
    pub enabled: bool,
}

/// LSP server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspServerConfig {
    pub command: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
}
