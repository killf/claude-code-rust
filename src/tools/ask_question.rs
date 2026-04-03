//! AskUserQuestion tool - present multiple-choice questions to the user

use async_trait::async_trait;
use serde::Deserialize;
use serde::Serialize;

use crate::error::CliError;
use crate::types::{Tool, ToolContext, ToolResult};

/// Maximum allowed length for a question header (chip/tag label)
const HEADER_MAX_CHARS: usize = 12;

/// Minimum and maximum number of options per question
const MIN_OPTIONS: usize = 2;
const MAX_OPTIONS: usize = 4;

/// Input for a single question
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct QuestionInput {
    /// Very short label displayed as a chip/tag (max 12 characters).
    /// Examples: "Auth method", "Library", "Approach".
    header: String,
    /// The available choices for this question. Must have 2-4 options.
    options: Vec<String>,
    /// Set to true to allow the user to select multiple options.
    #[serde(default)]
    multi_select: bool,
}

/// Top-level input for the AskUserQuestion tool
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct AskQuestionInput {
    /// Questions to ask the user (1-4 questions)
    questions: Vec<QuestionInput>,
    /// Optional timeout in seconds for waiting on user answers.
    #[serde(default)]
    timeout_secs: Option<u64>,
}

pub struct AskQuestionTool;

impl AskQuestionTool {
    pub fn new() -> Self {
        Self
    }

    fn input_schema() -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "questions": {
                    "type": "array",
                    "description": "Questions to ask the user (1-4 questions)",
                    "items": {
                        "type": "object",
                        "properties": {
                            "header": {
                                "type": "string",
                                "description": "Very short label displayed as a chip/tag (max 12 chars). Examples: \"Auth method\", \"Library\", \"Approach\"."
                            },
                            "options": {
                                "type": "array",
                                "description": "The available choices for this question. Must have 2-4 options.",
                                "items": {
                                    "type": "string"
                                },
                                "minItems": 2,
                                "maxItems": 4
                            },
                            "multi_select": {
                                "type": "boolean",
                                "description": "Set to true to allow the user to select multiple options instead of just one.",
                                "default": false
                            }
                        },
                        "required": ["header", "options"],
                        "additionalProperties": false
                    },
                    "minItems": 1,
                    "maxItems": 4
                },
                "timeout_secs": {
                    "type": "number",
                    "description": "Optional timeout in seconds for waiting on user answers."
                }
            },
            "required": ["questions"]
        })
    }
}

impl Default for AskQuestionTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for AskQuestionTool {
    fn name(&self) -> &str {
        "AskUserQuestion"
    }

    fn description(&self) -> String {
        "Asks the user multiple choice questions to gather information, clarify ambiguity, \
         understand preferences, make decisions or offer them choices."
            .to_string()
    }

    fn input_schema(&self) -> serde_json::Value {
        Self::input_schema()
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn is_concurrency_safe(&self) -> bool {
        true
    }

    async fn call(
        &self,
        args: serde_json::Value,
        _context: ToolContext,
    ) -> Result<ToolResult, CliError> {
        let input: AskQuestionInput = serde_json::from_value(args)?;

        // --- Validation ---

        if input.questions.is_empty() {
            return Ok(ToolResult::error(
                "At least one question is required.".to_string(),
            ));
        }

        if input.questions.len() > 4 {
            return Ok(ToolResult::error(
                "At most 4 questions can be asked at once.".to_string(),
            ));
        }

        for (i, q) in input.questions.iter().enumerate() {
            if q.header.len() > HEADER_MAX_CHARS {
                return Ok(ToolResult::error(format!(
                    "Question {} header \"{}\" exceeds the maximum length of {} characters.",
                    i + 1,
                    q.header,
                    HEADER_MAX_CHARS
                )));
            }

            if q.options.len() < MIN_OPTIONS {
                return Ok(ToolResult::error(format!(
                    "Question {} \"{}\" must have at least {} options.",
                    i + 1,
                    q.header,
                    MIN_OPTIONS
                )));
            }

            if q.options.len() > MAX_OPTIONS {
                return Ok(ToolResult::error(format!(
                    "Question {} \"{}\" must have at most {} options.",
                    i + 1,
                    q.header,
                    MAX_OPTIONS
                )));
            }
        }

        // --- Runtime: not available outside TUI ---
        //
        // Presenting a multiple-choice dialog requires the TUI event loop to
        // intercept rendering and block the coordinator until the user responds.
        // In non-interactive or headless contexts there is nobody at the keyboard,
        // so we return an error asking the model to use a different approach.
        Ok(ToolResult::error(
            "AskUserQuestion cannot be used in the current context: \
             it requires the TUI to present a multiple-choice dialog and block \
             until the user responds. Please use a different approach that does \
             not require user interaction, or provide the answer directly."
                .to_string(),
        ))
    }

    fn render_use_message(&self, args: &serde_json::Value) -> String {
        if let Ok(input) = serde_json::from_value::<AskQuestionInput>(args.clone()) {
            let headers: Vec<_> = input
                .questions
                .iter()
                .map(|q| q.header.as_str())
                .collect();
            format!("Asking user questions: {}", headers.join(", "))
        } else {
            "Asking user questions".to_string()
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ResultContentBlock;

    fn make_args(questions: Vec<(&str, Vec<&str>, bool)>) -> serde_json::Value {
        let qs: Vec<QuestionInput> = questions
            .into_iter()
            .map(|(header, options, multi_select)| QuestionInput {
                header: header.to_string(),
                options: options.into_iter().map(|s| s.to_string()).collect(),
                multi_select,
            })
            .collect();
        serde_json::to_value(AskQuestionInput {
            questions: qs,
            timeout_secs: None,
        })
        .unwrap()
    }

    fn call(input: serde_json::Value) -> ToolResult {
        let tool = AskQuestionTool::new();
        // call is async; run it via tokio
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(tool.call(input, ToolContext {
            session_id: "test".to_string(),
            agent_id: "test".to_string(),
            working_directory: std::path::PathBuf::from("/tmp"),
            can_use_tool: true,
            parent_message_id: None,
            env: std::collections::HashMap::new(),
        }))
        .unwrap()
    }

    fn is_error(result: &ToolResult) -> bool {
        result.is_error
    }

    fn error_message(result: &ToolResult) -> String {
        match &result.content[..] {
            [ResultContentBlock::Text { text }] => text.clone(),
            _ => panic!("expected single Text block"),
        }
    }

    // --- Validation: too few options ---

    #[test]
    fn test_too_few_options() {
        let result = call(make_args(vec![("Header", vec!["Only one"], false)]));
        assert!(is_error(&result), "expected error for < 2 options");
        assert!(
            error_message(&result).contains("at least 2"),
            "got: {}",
            error_message(&result)
        );
    }

    // --- Validation: too many options ---

    #[test]
    fn test_too_many_options() {
        let result = call(make_args(vec![(
            "Header",
            vec!["A", "B", "C", "D", "E"],
            false,
        )]));
        assert!(is_error(&result), "expected error for > 4 options");
        assert!(
            error_message(&result).contains("at most 4"),
            "got: {}",
            error_message(&result)
        );
    }

    // --- Validation: header too long ---

    #[test]
    fn test_header_too_long() {
        let long_header = "ThisIsWayTooLong"; // 16 chars
        let result = call(make_args(vec![(long_header, vec!["A", "B"], false)]));
        assert!(is_error(&result), "expected error for header > 12 chars");
        let msg = error_message(&result);
        assert!(
            msg.contains("exceeds the maximum length"),
            "got: {msg}"
        );
    }

    // --- Validation: exactly 2 options is ok ---

    #[test]
    fn test_min_options_valid() {
        let result = call(make_args(vec![("Header", vec!["Yes", "No"], false)]));
        // Should NOT be a validation error; returns non-interactive error instead
        let msg = error_message(&result);
        assert!(
            !msg.contains("at least 2") && !msg.contains("at most 4"),
            "should not be a validation error, got: {msg}"
        );
    }

    // --- Validation: exactly 4 options is ok ---

    #[test]
    fn test_max_options_valid() {
        let result = call(make_args(vec![(
            "Header",
            vec!["A", "B", "C", "D"],
            false,
        )]));
        let msg = error_message(&result);
        assert!(
            !msg.contains("at least 2") && !msg.contains("at most 4"),
            "should not be a validation error, got: {msg}"
        );
    }

    // --- Validation: header exactly 12 chars is ok ---

    #[test]
    fn test_header_exactly_max_chars_valid() {
        let header = "AuthMethod12"; // 12 chars
        assert_eq!(header.len(), 12);
        let result = call(make_args(vec![(header, vec!["A", "B"], false)]));
        let msg = error_message(&result);
        assert!(
            !msg.contains("exceeds the maximum length"),
            "got: {msg}"
        );
    }

    // --- Validation: 1 question ok, 5 questions not ok ---

    #[test]
    fn test_max_questions() {
        let result = call(make_args(vec![
            ("H1", vec!["A", "B"], false),
            ("H2", vec!["A", "B"], false),
            ("H3", vec!["A", "B"], false),
            ("H4", vec!["A", "B"], false),
            ("H5", vec!["A", "B"], false),
        ]));
        assert!(is_error(&result), "expected error for > 4 questions");
        assert!(
            error_message(&result).contains("At most 4 questions"),
            "got: {}",
            error_message(&result)
        );
    }

    // --- Non-interactive context always returns the blocking error ---

    #[test]
    fn test_non_interactive_returns_blocking_error() {
        let result = call(make_args(vec![("Header", vec!["A", "B"], false)]));
        let msg = error_message(&result);
        assert!(
            msg.contains("AskUserQuestion cannot be used in the current context"),
            "got: {msg}"
        );
    }

    // --- Schema ---

    #[test]
    fn test_input_schema_is_valid_json() {
        let schema = AskQuestionTool::new().input_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"].get("questions").is_some());
        let items = &schema["properties"]["questions"];
        assert_eq!(items["type"], "array");
        assert_eq!(items["minItems"], 1);
        assert_eq!(items["maxItems"], 4);
        let q_props = &items["items"]["properties"];
        assert!(q_props.get("header").is_some());
        assert!(q_props.get("options").is_some());
        assert!(q_props.get("multi_select").is_some());
        let opts = &q_props["options"];
        assert_eq!(opts["minItems"], 2);
        assert_eq!(opts["maxItems"], 4);
    }

    // --- Tool name ---

    #[test]
    fn test_tool_name() {
        assert_eq!(AskQuestionTool::new().name(), "AskUserQuestion");
    }
}
