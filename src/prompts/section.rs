//! Prompt section definitions — mirrors TypeScript src/constants/systemPromptSections.ts
//!
//! A section is a named computation that may be cached or uncached:
//! - `system_prompt_section()`: memoized, cached until clear (matching TypeScript systemPromptSection)
//! - `uncached_section()`: recomputes every turn, breaks the cache (matching TypeScript DANGEROUS_uncachedSystemPromptSection)

/// A named prompt section with optional caching.
#[derive(Debug, Clone)]
pub struct PromptSection {
    /// Unique section name.
    pub name: String,
    /// Whether this section breaks the prompt cache when recomputed.
    /// true = volatile (uncached, recomputes every turn)
    /// false = memoized (cached until /clear or /compact)
    pub cache_break: bool,
}

impl PromptSection {
    /// Create a memoized section. Cached until clear.
    /// Mirrors TypeScript: `systemPromptSection(name, compute)`
    pub fn memoized(name: &str) -> Self {
        Self {
            name: name.to_string(),
            cache_break: false,
        }
    }

    /// Create a volatile section. Recomputes every turn, breaking cache.
    /// WARNING: Only use when the section value changes frequently.
    /// Mirrors TypeScript: `DANGEROUS_uncachedSystemPromptSection(name, compute, reason)`
    pub fn uncached(name: &str) -> Self {
        Self {
            name: name.to_string(),
            cache_break: true,
        }
    }

    /// Returns true if this is a memoized (cached) section.
    pub fn is_memoized(&self) -> bool {
        !self.cache_break
    }
}
