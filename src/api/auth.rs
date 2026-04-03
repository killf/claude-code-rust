//! Authentication utilities
//!
//! Mirrors TypeScript's utils/auth.ts:
//!   getAnthropicApiKeyWithSource(), getAuthTokenSource(), getBaseUrl()
//!
//! Auth priority (matching TypeScript):
//!   1. ANTHROPIC_AUTH_TOKEN   — Bearer token (proxy setups)
//!   2. explicit_key           — CLI override
//!   3. Provider env var       — ANTHROPIC_API_KEY, AWS_ACCESS_KEY_ID, etc.
//!   4. global_config.env[key] — config file env section
//!   5. ANTHROPIC_API_KEY fallback (non-Anthropic providers only)
//!
//! Base URL priority:
//!   1. explicit_url override
//!   2. ANTHROPIC_BASE_URL (injected from settings.json.env via inject_config_env)
//!   3. Provider defaults

use std::env;

use crate::config::ModelProvider;
use crate::error::CliError;

/// Resolve API key with full priority chain (matches TypeScript).
///
/// After inject_config_env() has run, global_config.env vars are already
/// visible via std::env::var(), so we can use a unified env lookup.
pub async fn resolve_api_key(
    provider: ModelProvider,
    explicit_key: Option<&str>,
) -> Result<String, CliError> {
    resolve_api_key_with_config(provider, explicit_key, &std::collections::HashMap::new()).await
}

/// Resolve API key with config file env map access.
/// Kept for backward compatibility — global_config.env vars should be injected
/// via inject_config_env() before calling resolve_api_key().
pub async fn resolve_api_key_with_config(
    provider: ModelProvider,
    explicit_key: Option<&str>,
    _config_env: &std::collections::HashMap<String, String>,
) -> Result<String, CliError> {
    resolve_api_key_inner(provider, explicit_key).await
}

/// Internal auth resolution — mirrors TypeScript's getAnthropicApiKeyWithSource().
/// After inject_config_env(), std::env::var() already sees config.env vars.
async fn resolve_api_key_inner(
    provider: ModelProvider,
    explicit_key: Option<&str>,
) -> Result<String, CliError> {
    // 1. ANTHROPIC_AUTH_TOKEN — Bearer token (proxy setups, user's case)
    // TypeScript: `if (process.env.ANTHROPIC_AUTH_TOKEN) return Bearer`
    if let Ok(token) = env::var("ANTHROPIC_AUTH_TOKEN") {
        if !token.is_empty() {
            return Ok(token);
        }
    }

    // 2. Explicitly provided key
    if let Some(key) = explicit_key {
        if !key.is_empty() {
            return Ok(key.to_string());
        }
    }

    // 3. Provider-specific environment variable
    let env_var = match provider {
        ModelProvider::Anthropic => "ANTHROPIC_API_KEY",
        ModelProvider::AwsBedrock => "AWS_ACCESS_KEY_ID",
        ModelProvider::GcpVertex => "GOOGLE_API_KEY",
        ModelProvider::Azure => "AZURE_OPENAI_KEY",
        ModelProvider::OpenAi => "OPENAI_API_KEY",
        ModelProvider::Ollama => "OLLAMA_API_KEY",
        ModelProvider::Together => "TOGETHER_API_KEY",
    };

    if let Ok(key) = env::var(env_var) {
        if !key.is_empty() {
            return Ok(key);
        }
    }

    // 4. ANTHROPIC_API_KEY fallback for non-Anthropic providers
    if provider != ModelProvider::Anthropic {
        if let Ok(key) = env::var("ANTHROPIC_API_KEY") {
            if !key.is_empty() {
                return Ok(key);
            }
        }
    }

    Err(CliError::ApiKeyNotFound)
}

/// Get base URL for a provider.
/// Priority: explicit override > ANTHROPIC_BASE_URL env var > provider defaults.
pub fn get_base_url(provider: ModelProvider, explicit_url: Option<&str>) -> String {
    if let Some(url) = explicit_url {
        if !url.is_empty() {
            return url.to_string();
        }
    }

    // Check for ANTHROPIC_BASE_URL (injected from settings.json.env via inject_config_env).
    // This is the URL used by proxy setups like the user's.
    if let Ok(url) = env::var("ANTHROPIC_BASE_URL") {
        if !url.is_empty() {
            return url;
        }
    }

    match provider {
        ModelProvider::Anthropic => "https://api.anthropic.com".to_string(),
        ModelProvider::AwsBedrock => "https://bedrock.us-east-1.amazonaws.com".to_string(),
        ModelProvider::GcpVertex => "https://us-central1-aiplatform.googleapis.com/v1".to_string(),
        ModelProvider::Azure => "https://{resource}.openai.azure.com".to_string(),
        ModelProvider::OpenAi => "https://api.openai.com/v1".to_string(),
        ModelProvider::Ollama => "http://localhost:11434".to_string(),
        ModelProvider::Together => "https://api.together.xyz/v1".to_string(),
    }
}
