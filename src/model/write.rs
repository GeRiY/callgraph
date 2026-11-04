//! Property-írások.

use serde::{Deserialize, Serialize};

/// Egy értékadás egy property-nek (`$obj->prop = ...`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Write {
    pub container: Option<String>,
    pub caller: String,
    pub line: u32,
    #[serde(rename = "type")]
    pub kind: String,
    pub on: String,
    pub property: String,
}
