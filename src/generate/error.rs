//! A generátor hibatípusa: egyetlen, már lefordított (kétnyelvű) üzenet és a hiba fajtája. A `gen:` / `undo:` előtagot
//! a CLI-réteg teszi elé, így az üzenetek MCP-n is ugyanazok; a `Display` csak az üzenet.

use std::fmt;

/// A hiba fajtája: az ágens (MCP) mindegyikre mást tenne.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A cél-fájl nem található.
    NotFound,
    /// A fájl már létezik (a generátor soha nem ír felül).
    Exists,
    /// A kész szöveg szintaktikailag hibás (nem íródott semmi).
    Syntax,
    /// A cserélt sorok megváltoztak (`--expect`).
    Changed,
    /// A fájlt a `.callgraphignore` védi.
    Protected,
    /// Bármi más.
    Other,
}

/// Egy sornyi, felhasználónak szánt hiba.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenError {
    kind: Kind,
    message: String,
}

impl GenError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            kind: Kind::Other,
            message: message.into(),
        }
    }

    /// A hiba fajtájának megadása.
    #[must_use]
    pub fn with_kind(mut self, kind: Kind) -> Self {
        self.kind = kind;
        self
    }

    pub fn kind(&self) -> Kind {
        self.kind
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for GenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for GenError {}

impl From<std::io::Error> for GenError {
    fn from(e: std::io::Error) -> Self {
        Self::new(e.to_string())
    }
}

/// A generátor eredmény-típusa.
pub type GenResult<T> = Result<T, GenError>;
