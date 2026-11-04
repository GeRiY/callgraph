//! Hívás-él (függvény, metódus, statikus, `new`, konstans-elérés…).

use serde::{Deserialize, Serialize};

/// Egy hívás a `caller` metódusban (`__top__` = osztály-szint).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Call {
    pub container: Option<String>,
    pub caller: Option<String>,
    pub line: u32,
    #[serde(rename = "endLine", default, skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u32>,
    #[serde(rename = "type")]
    pub kind: String,
    pub target: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on: Option<String>,
    pub category: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    /// JS template: puszta függvénynév-eseménykezelő.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handler: Option<bool>,
}
