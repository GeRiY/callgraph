//! A generátor hibatípusa: egyetlen, már lefordított (kétnyelvű) üzenet. A `gen:` / `undo:` előtagot a CLI-réteg
//! teszi elé, így az üzenetek MCP-n is ugyanazok.

use std::fmt;

/// Egy sornyi, felhasználónak szánt hiba.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenError(String);

impl GenError {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }

    pub fn message(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for GenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for GenError {}

impl From<std::io::Error> for GenError {
    fn from(e: std::io::Error) -> Self {
        Self(e.to_string())
    }
}

/// A generátor eredmény-típusa.
pub type GenResult<T> = Result<T, GenError>;
