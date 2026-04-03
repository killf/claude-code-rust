//! Config corruption detection and recovery.
//!
//! Mirrors TypeScript's corrupted config recovery in config.ts:
//!   1. Try to parse the JSON config
//!   2. If corrupted, back it up as .corrupted.{timestamp}
//!   3. Try the most recent good backup
//!   4. If all fail, return defaults

use std::fs;
use std::path::{Path, PathBuf};

use crate::config::backup::{find_most_recent_backup, recover_from_backup};
use crate::config::GlobalConfig;
use crate::error::CliError;

/// Error indicating the config file could not be parsed.
#[derive(Debug)]
pub struct ConfigParseError {
    pub message: String,
    pub path: String,
}

impl std::fmt::Display for ConfigParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} at {}", self.message, self.path)
    }
}

impl std::error::Error for ConfigParseError {}

/// Load a config file, recovering from corruption if needed.
/// Strategy:
///   1. Try parsing the config directly
///   2. On parse error, back up corrupted file and try most recent backup
///   3. If all fail, return defaults and log
pub fn load_config_with_recovery(path: &Path) -> Result<GlobalConfig, CliError> {
    // Try to parse directly first
    if let Ok(config) = try_load_config(path) {
        return Ok(config);
    }

    // File exists but couldn't be parsed — it's corrupted
    eprintln!(
        "[Warning] Failed to parse config at {} — attempting recovery from backup",
        path.display()
    );

    // Back up the corrupted file
    if let Some(corrupted_backup) = backup_corrupted(path) {
        eprintln!("[Info] Corrupted config backed up to {}", corrupted_backup.display());
    }

    // Try to recover from most recent good backup
    if let Some(backup_path) = find_most_recent_backup(path) {
        eprintln!(
            "[Info] Attempting to recover from backup: {}",
            backup_path.display()
        );

        // Copy backup to target location
        recover_from_backup(&backup_path, path)?;

        // Try to parse the recovered file
        match try_load_config(path) {
            Ok(config) => {
                eprintln!("[Info] Config successfully recovered from backup.");
                return Ok(config);
            }
            Err(e) => {
                eprintln!(
                    "[Warning] Recovered config also failed to parse: {e} — using defaults"
                );
            }
        }
    } else {
        eprintln!("[Warning] No backup found for {} — using defaults", path.display());
    }

    // All recovery attempts failed — return defaults
    Ok(GlobalConfig::default())
}

/// Try to load and parse a config file.
fn try_load_config(path: &Path) -> Result<GlobalConfig, CliError> {
    let content = fs::read_to_string(path).map_err(|e| {
        CliError::Config(format!("failed to read {}: {e}", path.display()))
    })?;

    serde_json::from_str(&content).map_err(|e| {
        CliError::Parse(format!("failed to parse {}: {e}", path.display()))
    })
}

/// Back up a corrupted config file to .corrupted.{timestamp}.
fn backup_corrupted(path: &Path) -> Option<PathBuf> {
    let timestamp = chrono::Utc::now().timestamp_millis();
    let corrupted_path = format!("{}.corrupted.{}", path.display(), timestamp);
    fs::copy(path, &corrupted_path).ok()?;
    Some(Path::new(&corrupted_path).to_path_buf())
}
