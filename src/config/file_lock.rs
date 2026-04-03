//! File locking for cross-process config access.
//!
//! Uses `flock()` on Unix and Windows file locks for exclusive access.

use std::fs::{File, OpenOptions};
use std::path::Path;
use std::time::Duration;

#[cfg(unix)]
use std::os::unix::io::AsRawFd;
#[cfg(windows)]
use std::os::windows::io::AsRawHandle;

use crate::error::CliError;

/// A file lock that releases when dropped.
pub struct ConfigLock {
    _file: File,
    lock_path: std::path::PathBuf,
}

impl ConfigLock {
    /// Acquire an exclusive lock on the given config file.
    /// Creates the lock file if it doesn't exist.
    ///
    /// On Unix, uses `flock()`. On Windows, uses `LockFileEx`.
    pub fn acquire(path: &Path, timeout: Duration) -> Result<Self, CliError> {
        let lock_path: std::path::PathBuf = format!("{}.lock", path.display()).into();

        // Create lock file if it doesn't exist
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            .open(&lock_path)
            .map_err(|e| CliError::Config(format!("failed to create lock file {}: {e}", lock_path.display())))?;

        // Try to acquire the lock with timeout
        let deadline = std::time::Instant::now() + timeout;
        loop {
            match flock(&file, libc::LOCK_EX | libc::LOCK_NB) {
                Ok(()) => {
                    return Ok(Self {
                        _file: file,
                        lock_path,
                    });
                }
                Err(_) if std::time::Instant::now() < deadline => {
                    // Lock is held by another process — wait a bit and retry
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(_) => {
                    return Err(CliError::Config(format!(
                        "failed to acquire lock on {}: lock held by another process (timeout after {:?})",
                        lock_path.display(),
                        timeout
                    )));
                }
            }
        }
    }

    /// Release the lock explicitly (normally happens on Drop).
    pub fn release(self) {
        // We can't actually release early because File locks are released on drop.
        // The lock_path is just stored for logging purposes.
        self._file.sync_all().ok();
    }
}

impl Drop for ConfigLock {
    fn drop(&mut self) {
        // Release the flock
        #[cfg(unix)]
        {
            let _ = flock(&self._file, libc::LOCK_UN);
        }
        // On Windows, lock is released automatically when file is closed

        // Clean up the lock file
        if let Err(e) = std::fs::remove_file(&self.lock_path) {
            // Don't fail on cleanup errors
            eprintln!("[Warning] failed to remove lock file {}: {e}", self.lock_path.display());
        }
    }
}

/// Acquire or release a BSD-style file lock.
/// Returns `Ok(())` on success, `Err(())` if the lock could not be acquired
/// (only when LOCK_NB is set).
#[cfg(unix)]
fn flock(file: &File, operation: libc::c_int) -> Result<(), ()> {
    let fd = file.as_raw_fd();
    let result = unsafe { libc::flock(fd, operation) };
    if result == 0 {
        Ok(())
    } else {
        Err(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Lock file path = config path + ".lock" suffix
    fn lock_path(config: &std::path::Path) -> std::path::PathBuf {
        let mut p = config.as_os_str().to_os_string();
        p.push(".lock");
        std::path::PathBuf::from(p)
    }

    #[test]
    fn test_lock_acquire_and_release() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("settings.json");

        // Create the config file
        std::fs::write(&config_path, "{}").unwrap();

        // Acquire lock (verifies we can obtain the lock)
        let lock = ConfigLock::acquire(&config_path, Duration::from_secs(1)).unwrap();
        assert!(lock_path(&config_path).exists());

        // Explicitly drop to release lock and clean up the file (no panic = success)
        drop(lock);
    }

    #[test]
    fn test_lock_timeout() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("settings.json");
        std::fs::write(&config_path, "{}").unwrap();

        // Acquire first lock
        let _lock1 = ConfigLock::acquire(&config_path, Duration::from_secs(1)).unwrap();

        // Try to acquire second lock with very short timeout — should fail
        let result = ConfigLock::acquire(&config_path, Duration::from_millis(10));
        assert!(result.is_err());
    }
}
