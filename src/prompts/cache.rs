//! Prompt section cache — mirrors TypeScript src/constants/systemPromptSections.ts
//!
//! Sections are cached until cleared. Two types:
//! - `system_prompt_section()`: memoized (cached until /clear or /compact)
//! - `uncached_section()`: volatile (recomputes every turn, breaks cache)

use std::collections::HashMap;
use std::sync::{LazyLock, RwLock};

/// Global prompt section cache.
/// TypeScript: `STATE.systemPromptSectionCache` in bootstrap/state.ts
static PROMPT_CACHE: LazyLock<RwLock<HashMap<String, Option<String>>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// Beta header latches — per-section boolean flags reset on /clear and /compact.
/// TypeScript: `STATE.betaHeaderLatches` in bootstrap/state.ts
static BETA_LATCHES: LazyLock<RwLock<HashMap<String, bool>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// Get a cached section value. Returns None if not in cache.
pub fn get_cached(name: &str) -> Option<String> {
    PROMPT_CACHE.read().unwrap().get(name).cloned().flatten()
}

/// Set a cached section value.
pub fn set_cached(name: &str, value: Option<String>) {
    PROMPT_CACHE.write().unwrap().insert(name.to_string(), value);
}

/// Check if a section is in cache.
pub fn is_cached(name: &str) -> bool {
    PROMPT_CACHE.read().unwrap().contains_key(name)
}

/// Get a beta header latch value.
pub fn get_beta_latch(name: &str) -> bool {
    BETA_LATCHES.read().unwrap().get(name).copied().unwrap_or(false)
}

/// Set a beta header latch.
pub fn set_beta_latch(name: &str, value: bool) {
    BETA_LATCHES.write().unwrap().insert(name.to_string(), value);
}

/// Clear all prompt section cache. Called on /clear and /compact.
/// TypeScript: clearSystemPromptSections() → clearSystemPromptSectionState() + clearBetaHeaderLatches()
pub fn clear_cache() {
    PROMPT_CACHE.write().unwrap().clear();
    clear_beta_latches();
}

/// Clear all beta header latches.
pub fn clear_beta_latches() {
    BETA_LATCHES.write().unwrap().clear();
}

/// Thread-safe cache wrapper for use in PromptContext.
pub struct PromptCache;

impl PromptCache {
    /// Resolve a section, using cache if available and not volatile.
    /// Mirrors TypeScript: `resolveSystemPromptSections()`
    pub fn resolve_section<F>(name: &str, cache_break: bool, compute: F) -> Option<String>
    where
        F: FnOnce() -> Option<String>,
    {
        if !cache_break {
            if let Some(cached) = get_cached(name) {
                return Some(cached);
            }
        }
        let value = compute();
        set_cached(name, value.clone());
        value
    }

    /// Clear all cached sections and beta latches.
    pub fn clear_all() {
        clear_cache();
    }
}
