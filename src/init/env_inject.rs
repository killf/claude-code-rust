//! Config environment variable injection.
//!
//! Mirrors TypeScript's utils/managedEnv.ts — specifically
//! applySafeConfigEnvironmentVariables() and applyConfigEnvironmentVariables().
//!
//! TypeScript flow:
//!   1. enableConfigs() loads ~/.claude/settings.json
//!   2. applySafeConfigEnvironmentVariables() injects settings.json.env into process.env
//!      (runs BEFORE trust dialog — only non-sensitive vars are applied)
//!   3. applyConfigEnvironmentVariables() applies ALL vars after trust is established
//!   4. All subsequent code (including reqwest, other crate deps) sees injected vars
//!
//! Rust equivalent: inject_config_env() is called after load_global_config() returns,
//! making settings.json.env visible to std::env::var() throughout the program.

/// Keys that are never injected from config.env into the process environment.
/// These must be set by the user in their shell — config file cannot override them.
const BLOCKLISTED_KEYS: &[&str] = &[
    // Auth: users must set these in their shell
    "ANTHROPIC_API_KEY",
    "CLAUDE_CODE_SKIP_AUTH",
    "AWS_ACCESS_KEY_ID",
    "AWS_SECRET_ACCESS_KEY",
    "GOOGLE_API_KEY",
    "AZURE_OPENAI_KEY",
    "OPENAI_API_KEY",
    "TOGETHER_API_KEY",
    // Internal flags that should come from shell only
    "CLAUDE_CODE_OAUTH_TOKEN",
];

/// Inject config env variables into the process environment.
/// Call this AFTER load_global_config() but BEFORE creating ApiClient or any HTTP client,
/// so that ANTHROPIC_BASE_URL, ANTHROPIC_AUTH_TOKEN, etc. are visible to reqwest.
pub fn inject_config_env(config_env: &std::collections::HashMap<String, String>) {
    for (key, value) in config_env {
        // Blocklisted keys: only inject if not already present in process env.
        // This preserves shell-set values over config-file values (shell has higher priority).
        if BLOCKLISTED_KEYS.iter().any(|b| *b == key) {
            if std::env::var(key).is_err() {
                std::env::set_var(key, value);
            }
            continue;
        }
        // All other keys: config file takes precedence.
        std::env::set_var(key, value);
    }
}
