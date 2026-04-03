//! Config tool — read and write Claude Code settings in ~/.claude/settings.json

use async_trait::async_trait;
use serde::Deserialize;

use crate::config::{GlobalConfig, ThemeVariant};
use crate::error::CliError;
use crate::types::{PermissionMode, Tool, ToolContext, ToolResult};

/// Supported setting keys
const SUPPORTED_KEYS: &[&str] = &[
    "model",
    "theme",
    "permission_mode",
    "max_tokens",
    "temperature",
    "verbose",
];

/// Input schema for ConfigTool
#[derive(Debug, Deserialize)]
#[serde(tag = "action")]
pub enum ConfigInput {
    /// Get the current value of a setting (or all settings if no key given)
    #[serde(rename = "get")]
    Get {
        #[serde(default)]
        key: Option<String>,
    },
    /// Set a setting to a new value
    #[serde(rename = "set")]
    Set {
        key: String,
        value: String,
    },
}

// ---------------------------------------------------------------------------
// ConfigTool
// ---------------------------------------------------------------------------

pub struct ConfigTool;

impl ConfigTool {
    pub fn new() -> Self {
        Self
    }

    /// Path to the settings.json file (~/.claude/settings.json)
    fn settings_path() -> std::path::PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("claude")
            .join("settings.json")
    }

    /// Load GlobalConfig from disk asynchronously, returning defaults if absent.
    async fn load_config() -> Result<GlobalConfig, CliError> {
        let path = Self::settings_path();
        if !path.exists() {
            return Ok(GlobalConfig::default());
        }
        let content = tokio::fs::read_to_string(&path).await
            .map_err(|e| CliError::ToolExecution(format!("failed to read settings: {e}")))?;
        serde_json::from_str(&content)
            .map_err(|e| CliError::ToolExecution(format!("failed to parse settings: {e}")))
    }

    /// Save GlobalConfig to disk asynchronously, creating parent dirs as needed.
    async fn save_config(config: &GlobalConfig) -> Result<(), CliError> {
        let path = Self::settings_path();
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let content = serde_json::to_string_pretty(config)
            .map_err(|e| CliError::ToolExecution(format!("failed to serialise settings: {e}")))?;
        tokio::fs::write(&path, content).await?;
        Ok(())
    }

    /// Get a human-readable value string for a setting key.
    fn get_value_str(config: &GlobalConfig, key: &str) -> String {
        match key {
            "model" => config
                .model_preferences
                .model
                .clone()
                .unwrap_or_else(|| "(not set)".to_string()),
            "theme" => format!("{}", config.theme.variant),
            "permission_mode" => format!("{}", config.permission_mode),
            "max_tokens" => config
                .max_tokens
                .map(|v| v.to_string())
                .unwrap_or_else(|| "(not set)".to_string()),
            "temperature" => config
                .temperature
                .map(|v| v.to_string())
                .unwrap_or_else(|| "(not set)".to_string()),
            "verbose" => config.verbose.to_string(),
            _ => "(unknown)".to_string(),
        }
    }

    /// Set a setting value from a string. Returns the updated config.
    fn set_value(config: &mut GlobalConfig, key: &str, value: &str) -> Result<(), String> {
        match key {
            "model" => {
                config.model_preferences.model = Some(value.to_string());
            }
            "theme" => {
                let variant: ThemeVariant = value
                    .parse()
                    .map_err(|_| format!("invalid theme: {value}. Options: system, dark, light"))?;
                config.theme.variant = variant;
            }
            "permission_mode" => {
                let mode: PermissionMode = value
                    .parse()
                    .map_err(|_| format!("invalid permission_mode: {value}"))?;
                config.permission_mode = mode;
            }
            "max_tokens" => {
                let v: u32 = value.parse().map_err(|_| {
                    "max_tokens must be a positive integer".to_string()
                })?;
                config.max_tokens = Some(v);
            }
            "temperature" => {
                let v: f32 = value.parse().map_err(|_| {
                    "temperature must be a number between 0 and 2".to_string()
                })?;
                if !(0.0..=2.0).contains(&v) {
                    return Err("temperature must be between 0 and 2".to_string());
                }
                config.temperature = Some(v);
            }
            "verbose" => {
                let v = match value.to_lowercase().as_str() {
                    "true" | "1" | "yes" => true,
                    "false" | "0" | "no" => false,
                    _ => return Err("verbose must be true or false".to_string()),
                };
                config.verbose = v;
            }
            _ => return Err(format!("unknown setting: {key}")),
        }
        Ok(())
    }

    /// Format all settings as a plain text list.
    fn format_all_settings(config: &GlobalConfig) -> String {
        let mut lines: Vec<String> = SUPPORTED_KEYS
            .iter()
            .map(|k| format!("  {}: {}", k, Self::get_value_str(config, k)))
            .collect();
        lines.sort();
        format!("Current settings:\n{}", lines.join("\n"))
    }
}

impl Default for ConfigTool {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tool trait impl
// ---------------------------------------------------------------------------

#[async_trait]
impl Tool for ConfigTool {
    fn name(&self) -> &str {
        "Config"
    }

    fn description(&self) -> String {
        "Read or modify Claude Code settings stored in ~/.claude/settings.json. \
         Use 'get' to retrieve the current value of a setting (or all settings), \
         and 'set' to update a setting."
            .to_string()
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "oneOf": [
                {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["get"] },
                        "key": {
                            "type": "string",
                            "description": "Setting name (optional). Omit to get all settings.",
                            "enum": SUPPORTED_KEYS
                        }
                    },
                    "required": ["action"]
                },
                {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["set"] },
                        "key": {
                            "type": "string",
                            "description": "Setting name to update",
                            "enum": SUPPORTED_KEYS
                        },
                        "value": {
                            "type": "string",
                            "description": "New value for the setting"
                        }
                    },
                    "required": ["action", "key", "value"]
                }
            ]
        })
    }

    fn is_read_only(&self) -> bool {
        false
    }

    async fn call(
        &self,
        args: serde_json::Value,
        _context: ToolContext,
    ) -> Result<ToolResult, CliError> {
        let input: ConfigInput = match serde_json::from_value(args) {
            Ok(v) => v,
            Err(e) => {
                return Ok(ToolResult::error(format!("invalid input: {e}")));
            }
        };

        match input {
            ConfigInput::Get { key } => {
                let config = Self::load_config().await?;

                if let Some(k) = key {
                    if !SUPPORTED_KEYS.contains(&k.as_str()) {
                        return Ok(ToolResult::error(format!(
                            "Unknown setting: \"{k}\". Supported: {}",
                            SUPPORTED_KEYS.join(", ")
                        )));
                    }
                    let value = Self::get_value_str(&config, &k);
                    return Ok(ToolResult::text(format!("{k} = {value}")));
                }

                // No key: return all settings
                Ok(ToolResult::text(Self::format_all_settings(&config)))
            }

            ConfigInput::Set { key, value } => {
                if !SUPPORTED_KEYS.contains(&key.as_str()) {
                    return Ok(ToolResult::error(format!(
                        "Unknown setting: \"{key}\". Supported: {}",
                        SUPPORTED_KEYS.join(", ")
                    )));
                }

                // Load, mutate, save — fully async
                let mut config = Self::load_config().await?;
                let prev = Self::get_value_str(&config, &key);
                Self::set_value(&mut config, &key, &value)
                    .map_err(CliError::ToolExecution)?;
                Self::save_config(&config).await?;
                let new = Self::get_value_str(&config, &key);
                let msg = if prev == new {
                    format!("{key} = {new} (unchanged)",)
                } else {
                    format!("{key}: {prev} -> {new}")
                };
                Ok(ToolResult::text(msg))
            }
        }
    }

    fn render_use_message(&self, args: &serde_json::Value) -> String {
        if let Ok(input) = serde_json::from_value::<ConfigInput>(args.clone()) {
            match input {
                ConfigInput::Get { key } => {
                    if let Some(k) = key {
                        format!("Reading setting: {k}")
                    } else {
                        "Reading all settings".to_string()
                    }
                }
                ConfigInput::Set { key, value } => {
                    format!("Setting {key} = {value}")
                }
            }
        } else {
            "Reading or modifying settings".to_string()
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_theme_variant_parsing() {
        assert_eq!(
            "system".parse::<ThemeVariant>().unwrap(),
            ThemeVariant::System
        );
        assert_eq!(
            "dark".parse::<ThemeVariant>().unwrap(),
            ThemeVariant::Dark
        );
        assert_eq!(
            "light".parse::<ThemeVariant>().unwrap(),
            ThemeVariant::Light
        );
        assert_eq!(
            "auto".parse::<ThemeVariant>().unwrap(),
            ThemeVariant::Auto
        );
        assert!("invalid".parse::<ThemeVariant>().is_err());
    }

    #[test]
    fn test_permission_mode_parsing() {
        assert_eq!(
            "default".parse::<PermissionMode>().unwrap(),
            PermissionMode::Default
        );
        assert_eq!(
            "acceptEdits".parse::<PermissionMode>().unwrap(),
            PermissionMode::AcceptEdits
        );
        assert_eq!(
            "bypassPermissions"
                .parse::<PermissionMode>()
                .unwrap(),
            PermissionMode::BypassPermissions
        );
        assert_eq!(
            "dontAsk".parse::<PermissionMode>().unwrap(),
            PermissionMode::DontAsk
        );
        assert_eq!(
            "plan".parse::<PermissionMode>().unwrap(),
            PermissionMode::Plan
        );
        assert!("unknown".parse::<PermissionMode>().is_err());
    }

    #[test]
    fn test_global_config_defaults() {
        let config = GlobalConfig::default();
        assert!(!config.verbose);
        assert_eq!(config.theme.variant, ThemeVariant::System);
        assert_eq!(config.permission_mode, PermissionMode::Default);
        assert!(config.max_tokens.is_none());
        assert!(config.temperature.is_none());
        assert!(config.model_preferences.model.is_none());
    }

    #[test]
    fn test_set_value_verbose() {
        let mut config = GlobalConfig::default();
        assert!(!config.verbose);
        ConfigTool::set_value(&mut config, "verbose", "true").unwrap();
        assert!(config.verbose);
        ConfigTool::set_value(&mut config, "verbose", "false").unwrap();
        assert!(!config.verbose);
        ConfigTool::set_value(&mut config, "verbose", "yes").unwrap();
        assert!(config.verbose);
        ConfigTool::set_value(&mut config, "verbose", "no").unwrap();
        assert!(!config.verbose);
    }

    #[test]
    fn test_set_value_max_tokens() {
        let mut config = GlobalConfig::default();
        ConfigTool::set_value(&mut config, "max_tokens", "4096").unwrap();
        assert_eq!(config.max_tokens, Some(4096));
    }

    #[test]
    fn test_set_value_max_tokens_invalid() {
        let mut config = GlobalConfig::default();
        let result = ConfigTool::set_value(&mut config, "max_tokens", "not-a-number");
        assert!(result.is_err());
    }

    #[test]
    fn test_set_value_temperature() {
        let mut config = GlobalConfig::default();
        ConfigTool::set_value(&mut config, "temperature", "0.7").unwrap();
        assert_eq!(config.temperature, Some(0.7));
    }

    #[test]
    fn test_set_value_temperature_out_of_range() {
        let mut config = GlobalConfig::default();
        let result = ConfigTool::set_value(&mut config, "temperature", "3.0");
        assert!(result.is_err());
        let result = ConfigTool::set_value(&mut config, "temperature", "-0.5");
        assert!(result.is_err());
    }

    #[test]
    fn test_set_value_model() {
        let mut config = GlobalConfig::default();
        ConfigTool::set_value(&mut config, "model", "claude-opus-4-5").unwrap();
        assert_eq!(config.model_preferences.model, Some("claude-opus-4-5".to_string()));
    }

    #[test]
    fn test_set_value_theme() {
        let mut config = GlobalConfig::default();
        assert_eq!(config.theme.variant, ThemeVariant::System);
        ConfigTool::set_value(&mut config, "theme", "dark").unwrap();
        assert_eq!(config.theme.variant, ThemeVariant::Dark);
        ConfigTool::set_value(&mut config, "theme", "light").unwrap();
        assert_eq!(config.theme.variant, ThemeVariant::Light);
    }

    #[test]
    fn test_set_value_theme_invalid() {
        let mut config = GlobalConfig::default();
        let result = ConfigTool::set_value(&mut config, "theme", "neon");
        assert!(result.is_err());
    }

    #[test]
    fn test_set_value_permission_mode() {
        let mut config = GlobalConfig::default();
        assert_eq!(config.permission_mode, PermissionMode::Default);
        ConfigTool::set_value(&mut config, "permission_mode", "acceptEdits").unwrap();
        assert_eq!(config.permission_mode, PermissionMode::AcceptEdits);
        ConfigTool::set_value(&mut config, "permission_mode", "bypassPermissions").unwrap();
        assert_eq!(config.permission_mode, PermissionMode::BypassPermissions);
    }

    #[test]
    fn test_set_value_unknown_key() {
        let mut config = GlobalConfig::default();
        let result = ConfigTool::set_value(&mut config, "unknown_key", "value");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unknown setting"));
    }

    #[test]
    fn test_get_value_str() {
        let mut config = GlobalConfig::default();
        assert_eq!(ConfigTool::get_value_str(&config, "verbose"), "false");
        assert_eq!(ConfigTool::get_value_str(&config, "theme"), "system");
        assert_eq!(ConfigTool::get_value_str(&config, "permission_mode"), "default");
        assert_eq!(ConfigTool::get_value_str(&config, "max_tokens"), "(not set)");
        assert_eq!(ConfigTool::get_value_str(&config, "temperature"), "(not set)");
        assert_eq!(ConfigTool::get_value_str(&config, "model"), "(not set)");

        config.verbose = true;
        config.max_tokens = Some(8192);
        config.temperature = Some(0.9);
        config.model_preferences.model = Some("claude-sonnet-4".to_string());
        assert_eq!(ConfigTool::get_value_str(&config, "verbose"), "true");
        assert_eq!(ConfigTool::get_value_str(&config, "max_tokens"), "8192");
        assert_eq!(ConfigTool::get_value_str(&config, "temperature"), "0.9");
        assert_eq!(ConfigTool::get_value_str(&config, "model"), "claude-sonnet-4");
    }

    #[test]
    fn test_format_all_settings() {
        let config = GlobalConfig::default();
        let output = ConfigTool::format_all_settings(&config);
        assert!(output.contains("max_tokens: (not set)"));
        assert!(output.contains("verbose: false"));
        assert!(output.contains("theme: system"));
    }

    #[test]
    fn test_config_input_get_deserialize() {
        let json = serde_json::json!({ "action": "get" });
        let input: ConfigInput = serde_json::from_value(json).unwrap();
        assert!(matches!(input, ConfigInput::Get { key: None }));

        let json = serde_json::json!({ "action": "get", "key": "theme" });
        let input: ConfigInput = serde_json::from_value(json).unwrap();
        assert!(matches!(input, ConfigInput::Get { key: Some(k) } if k == "theme"));
    }

    #[test]
    fn test_config_input_set_deserialize() {
        let json = serde_json::json!({ "action": "set", "key": "verbose", "value": "true" });
        let input: ConfigInput = serde_json::from_value(json).unwrap();
        assert!(matches!(
            input,
            ConfigInput::Set { key, value } if key == "verbose" && value == "true"
        ));
    }

    #[test]
    fn test_input_schema_shape() {
        let tool = ConfigTool::new();
        let schema = tool.input_schema();
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["oneOf"].as_array().unwrap().len(), 2);
    }
}
