//! Metódus (a törzs forrásszövegével).

use serde::{Deserialize, Serialize};

/// Egy konténer metódusa.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Method {
    pub container: Option<String>,
    pub name: String,
    pub line: u32,
    #[serde(rename = "endLine", default, skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u32>,
    pub visibility: Option<String>,
    #[serde(rename = "static")]
    pub is_static: bool,
    #[serde(rename = "abstract")]
    pub is_abstract: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub kind: Option<String>,
    pub params: Vec<String>,
    #[serde(rename = "returnType")]
    pub return_type: Option<String>,
    /// A docblock `@return` / `@param` típusa, ha pontosabb a deklaráltnál (PHP); lásd `doc_types_for`.
    #[serde(rename = "docTypes", default, skip_serializing_if = "Option::is_none")]
    pub doc_types: Option<String>,
    pub doc: Option<String>,
    pub body: Option<String>,
}
