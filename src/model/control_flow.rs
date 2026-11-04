//! Vezérlési szerkezetek (if / switch / match / ...).

use serde::{Deserialize, Serialize};

/// Egy elágazás a `caller` metódusban.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ControlFlow {
    pub container: Option<String>,
    pub caller: Option<String>,
    #[serde(rename = "type")]
    pub kind: String,
    pub line: u32,
    pub cond: Option<String>,
    pub branches: Vec<Branch>,
}

/// Az elágazás egy ága.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Branch {
    pub label: String,
    #[serde(rename = "startLine")]
    pub start_line: u32,
    #[serde(rename = "endLine")]
    pub end_line: u32,
    #[serde(
        rename = "fallThrough",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub fall_through: Option<bool>,
}
