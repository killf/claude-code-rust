use thiserror::Error;

#[derive(Error, Debug)]
pub enum CliError {
    #[error("API error: {0}")]
    Api(String),
    #[error("API key not found")]
    ApiKeyNotFound,
    #[error("Tool permission denied: {0}")]
    PermissionDenied(String),
    #[error("Tool execution failed: {0}")]
    ToolExecution(String),
    #[error("Configuration error: {0}")]
    Config(String),
    #[error("Session error: {0}")]
    Session(String),
    #[error("MCP error: {0}")]
    Mcp(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Other error: {0}")]
    Other(String),
}

impl From<String> for CliError {
    fn from(s: String) -> Self {
        CliError::Other(s)
    }
}

impl From<&str> for CliError {
    fn from(s: &str) -> Self {
        CliError::Other(s.to_string())
    }
}
