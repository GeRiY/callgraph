//! String-literálok.

use serde::{Deserialize, Serialize};

/// Egy string-literál értéke a `caller` környezetében.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StringLit {
    pub container: Option<String>,
    pub caller: Option<String>,
    pub line: u32,
    #[serde(rename = "endLine", default, skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u32>,
    pub value: String,
}
