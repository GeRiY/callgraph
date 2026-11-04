//! Osztály / interfész / trait / enum.

use serde::{Deserialize, Serialize};

/// Egy típus-deklaráció (`type`: class | interface | trait | enum).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Container {
    #[serde(rename = "type")]
    pub kind: String,
    pub name: String,
    pub line: u32,
    #[serde(rename = "endLine")]
    pub end_line: u32,
    #[serde(rename = "abstract", skip_serializing_if = "Option::is_none")]
    pub is_abstract: Option<bool>,
    #[serde(rename = "final", skip_serializing_if = "Option::is_none")]
    pub is_final: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extends: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub implements: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub traits: Option<Vec<String>>,
    pub doc: Option<String>,
}
