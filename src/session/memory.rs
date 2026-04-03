//! Session memory — CLAUDE.md loading and merging.
//!
//! Mirrors TypeScript's src/memdir/sessionMemory.ts:
//! - Load CLAUDE.md from the project root
//! - Merge CLAUDE.md content into the system prompt
//! - Track memory file modifications

use std::path::Path;

use crate::error::CliError;

/// Memory file name
pub const MEMORY_FILE: &str = "CLAUDE.md";

/// Load session memory from the project directory.
/// Returns the content of CLAUDE.md if found, None otherwise.
pub fn load_session_memory(cwd: &Path) -> Result<Option<String>, CliError> {
    let memory_path = cwd.join(MEMORY_FILE);

    if !memory_path.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(&memory_path)
        .map_err(|e| crate::error::CliError::Io(e))?;

    if content.trim().is_empty() {
        return Ok(None);
    }

    Ok(Some(content))
}

/// Build the memory section for the system prompt.
/// Returns a string formatted as a markdown section, or None if no memory.
pub fn build_memory_section(cwd: &Path) -> Result<Option<String>, CliError> {
    let memory = load_session_memory(cwd)?;

    Ok(memory.map(|content| {
        format!(
            r#"# Project Memory
{}
"#,
            content.trim()
        )
    }))
}

/// Merge new content into a CLAUDE.md memory file.
/// Adds content under a "## Recent Context" heading.
#[allow(dead_code)]
pub fn merge_memory(existing: &str, new_content: &str) -> String {
    // If existing has a "## Recent Context" section, append to it
    if existing.contains("## Recent Context") {
        existing.to_string() + "\n" + new_content.trim() + "\n"
    } else {
        format!(
            "{}\n\n## Recent Context\n{}\n",
            existing.trim_end(),
            new_content.trim()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_load_session_memory_found() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join(MEMORY_FILE), "Remember to write tests.\n").unwrap();

        let memory = load_session_memory(tmp.path()).unwrap();
        assert!(memory.is_some());
        assert!(memory.unwrap().contains("Remember to write tests"));
    }

    #[test]
    fn test_load_session_memory_not_found() {
        let tmp = TempDir::new().unwrap();
        let memory = load_session_memory(tmp.path()).unwrap();
        assert!(memory.is_none());
    }

    #[test]
    fn test_build_memory_section() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join(MEMORY_FILE), "Project instructions here.").unwrap();

        let section = build_memory_section(tmp.path()).unwrap().unwrap();
        assert!(section.contains("# Project Memory"));
        assert!(section.contains("Project instructions here"));
    }

    #[test]
    fn test_merge_memory() {
        let existing = "# Project Notes\n\nHello world.";
        let merged = merge_memory(existing, "Additional note.");
        assert!(merged.contains("## Recent Context"));
        assert!(merged.contains("Additional note"));
    }
}
