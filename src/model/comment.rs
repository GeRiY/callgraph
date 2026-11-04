//! Kommentek.

use serde::{Deserialize, Serialize};

/// Egy (nem docblock) komment a `caller` környezetében.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Comment {
    pub container: Option<String>,
    pub caller: Option<String>,
    pub line: u32,
    pub tag: Option<String>,
    pub text: String,
}
