//! Configuration system for Claude Code CLI

pub mod global;
pub mod project;
pub mod file_lock;
pub mod backup;
pub mod recovery;

pub use backup::{create_backup, find_most_recent_backup};
pub use file_lock::ConfigLock;
pub use global::*;
pub use project::*;
pub use recovery::load_config_with_recovery;

use std::path::PathBuf;
use anyhow::Result;

use crate::error::CliError;

/// Configuration loader for global and project configs
pub struct ConfigLoader {
    global_path: PathBuf,
    project_path: PathBuf,
}

impl ConfigLoader {
    pub fn new() -> Self {
        // Use ~/.claude/settings.json (not XDG ~/.config/claude/).
        // TypeScript uses os.homedir() + '/.claude/settings.json'.
        let global_path = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".claude");
        Self {
            global_path,
            project_path: PathBuf::from(".claude"),
        }
    }

    pub fn global_path(&self) -> &PathBuf {
        &self.global_path
    }

    pub fn project_path(&self) -> &PathBuf {
        &self.project_path
    }

    /// Load global configuration
    pub async fn load_global_config(&self) -> Result<GlobalConfig, CliError> {
        let path = self.global_path.join("settings.json");
        if path.exists() {
            let content = tokio::fs::read_to_string(&path).await?;
            let config: GlobalConfig = serde_json::from_str(&content)?;
            Ok(config)
        } else {
            Ok(GlobalConfig::default())
        }
    }

    /// Save global configuration
    pub async fn save_global_config(&self, config: &GlobalConfig) -> Result<(), CliError> {
        let path = self.global_path.join("settings.json");
        tokio::fs::create_dir_all(&self.global_path).await?;
        let content = serde_json::to_string_pretty(config)?;
        tokio::fs::write(&path, content).await?;
        Ok(())
    }

    /// Load project configuration from the given repo root
    pub async fn load_project_config(
        &self,
        repo_root: Option<PathBuf>,
    ) -> Result<ProjectConfig, CliError> {
        let base = repo_root.unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        let path = base.join(".claude").join("settings.json");
        if path.exists() {
            let content = tokio::fs::read_to_string(&path).await?;
            let config: ProjectConfig = serde_json::from_str(&content)?;
            Ok(config)
        } else {
            Ok(ProjectConfig::default())
        }
    }

    /// Save project configuration
    pub async fn save_project_config(
        &self,
        config: &ProjectConfig,
        repo_root: Option<PathBuf>,
    ) -> Result<(), CliError> {
        let base = repo_root.unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        let dir = base.join(".claude");
        tokio::fs::create_dir_all(&dir).await?;
        let path = dir.join("settings.json");
        let content = serde_json::to_string_pretty(config)?;
        tokio::fs::write(&path, content).await?;
        Ok(())
    }
}

impl Default for ConfigLoader {
    fn default() -> Self {
        Self::new()
    }
}
