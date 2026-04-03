//! Config backup management.
//!
//! Creates timestamped backups before writing config files.
//! Mirrors TypeScript's backup logic in config.ts:
//!   - Backups stored in ~/.claude/backups/
//!   - Max 5 backups, oldest deleted on overflow
//!   - 60-second minimum interval between backups

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::CliError;

/// Maximum number of backup files to keep per config.
const MAX_BACKUPS: usize = 5;

/// Minimum interval between creating new backups (ms).
const MIN_BACKUP_INTERVAL_MS: u64 = 60_000;

/// Returns the backup directory path: `~/.claude/backups/`.
pub fn backup_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("claude")
        .join("backups")
}

/// Create a timestamped backup using the default backup directory.
pub fn create_backup(config_path: &Path) -> Result<Option<PathBuf>, CliError> {
    create_backup_with_dir(config_path, None)
}

/// Create a timestamped backup of a config file.
/// Skips if a backup was created within the last MIN_BACKUP_INTERVAL_MS.
/// After creating, cleans up backups keeping only the `MAX_BACKUPS` most recent.
/// Uses `backup_dir()` (~/.claude/backups/) by default; override with `backup_dir_override`.
pub fn create_backup_with_dir(
    config_path: &Path,
    backup_dir_override: Option<&Path>,
) -> Result<Option<PathBuf>, CliError> {
    if !config_path.exists() {
        return Ok(None);
    }

    let backup_dir = backup_dir_override.map(|p| p.to_path_buf()).unwrap_or_else(backup_dir);

    // Ensure backup directory exists
    fs::create_dir_all(&backup_dir).map_err(|e| {
        CliError::Config(format!("failed to create backup directory {}: {e}", backup_dir.display()))
    })?;

    let file_base = config_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("config");

    // Check if a recent backup exists
    let existing_backups = list_backups(&backup_dir, file_base);
    let should_create = if let Some(most_recent) = existing_backups.last() {
        let ts = most_recent
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|s| s.split_once(".backup."))
            .and_then(|(_, ts)| ts.parse::<i64>().ok())
            .unwrap_or(0);
        let elapsed = chrono::Utc::now().timestamp_millis() as u64 - ts as u64;
        elapsed >= MIN_BACKUP_INTERVAL_MS
    } else {
        true
    };

    if !should_create {
        return Ok(None);
    }

    // Create new backup
    let timestamp = chrono::Utc::now().timestamp_millis();
    let backup_path = backup_dir.join(format!("{file_base}.backup.{timestamp}"));
    fs::copy(config_path, &backup_path).map_err(|e| {
        CliError::Config(format!("failed to copy {} to {}: {e}", config_path.display(), backup_path.display()))
    })?;

    // Clean up old backups — keep MAX_BACKUPS most recent
    let all_backups = list_backups(&backup_dir, file_base);
    for old_backup in all_backups.iter().take(all_backups.len().saturating_sub(MAX_BACKUPS)) {
        if let Err(e) = fs::remove_file(old_backup) {
            // Non-fatal: just log and continue
            eprintln!("[Warning] failed to remove old backup {}: {e}", old_backup.display());
        }
    }

    Ok(Some(backup_path))
}

/// List all backup files for a given base name, sorted oldest-first.
fn list_backups(backup_dir: &Path, file_base: &str) -> Vec<PathBuf> {
    let prefix = format!("{file_base}.backup.");
    let mut backups: Vec<PathBuf> = fs::read_dir(backup_dir)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.starts_with(&prefix))
                .unwrap_or(false)
        })
        .collect();

    backups.sort_by_key(|p| {
        p.file_name()
            .and_then(|n| n.to_str())
            .and_then(|s| s.split_once(".backup."))
            .and_then(|(_, ts)| ts.parse::<i64>().ok())
            .unwrap_or(0)
    });
    backups
}

/// Find the most recent backup for a config file.
/// Checks backup_dir first, then falls back to legacy location next to config.
pub fn find_most_recent_backup(config_path: &Path) -> Option<PathBuf> {
    let file_base = config_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("config");

    // Check backup dir first
    let backup_dir = backup_dir();
    if let Some(path) = list_backups(&backup_dir, file_base).last() {
        return Some(path.clone());
    }

    // Fall back to legacy location next to config file
    let legacy = config_path.with_extension("backup");
    if legacy.exists() {
        return Some(legacy);
    }

    None
}

/// Recover a config from a backup file.
pub fn recover_from_backup(backup: &Path, target: &Path) -> Result<(), CliError> {
    fs::copy(backup, target).map_err(|e| {
        CliError::Config(format!("failed to recover {} from {}: {e}", target.display(), backup.display()))
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_create_and_find_backup() {
        let tmp = TempDir::new().unwrap();
        let backup_dir = tmp.path().join("backups");
        let config = tmp.path().join("settings.json");
        std::fs::write(&config, r#"{"key": "value"}"#).unwrap();

        let result = create_backup_with_dir(&config, Some(&backup_dir)).unwrap();
        assert!(result.is_some());

        // Use a custom find function for the test-specific dir
        let backups: Vec<_> = std::fs::read_dir(&backup_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name().to_str().map(|s| s.starts_with("settings.json.backup.")).unwrap_or(false)
            })
            .collect();
        assert!(!backups.is_empty());
        let content = std::fs::read_to_string(backups.last().unwrap().path()).unwrap();
        assert!(content.contains("value"));
    }

    #[test]
    fn test_create_backup_no_file() {
        let tmp = TempDir::new().unwrap();
        let backup_dir = tmp.path().join("backups");
        let config = tmp.path().join("settings.json");
        let result = create_backup_with_dir(&config, Some(&backup_dir)).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_max_backups_enforced() {
        let tmp = TempDir::new().unwrap();
        let backup_dir = tmp.path().join("backups");
        let config = tmp.path().join("settings.json");
        std::fs::write(&config, "{}").unwrap();

        // Create many backups
        for i in 0..10 {
            std::fs::write(&config, &format!(r#"{{"v": {i}}}"#)).unwrap();
            create_backup_with_dir(&config, Some(&backup_dir)).unwrap();
        }

        // Should have at most MAX_BACKUPS
        let backups: Vec<_> = std::fs::read_dir(&backup_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name().to_str().map(|s| s.starts_with("settings.json.backup.")).unwrap_or(false)
            })
            .collect();
        assert!(backups.len() <= MAX_BACKUPS);
    }
}
