//! Közös hiba-típus: minden réteg ezt adja vissza, a CLI a stderr-re írja.

use std::path::PathBuf;

use thiserror::Error;

/// Az elemzés során előforduló, fájl-szintű hibák.
#[derive(Debug, Error)]
pub enum CallgraphError {
    /// A fájl nem olvasható.
    #[error("{}: nem olvasható: {source}", path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// A kimenet írása nem sikerült.
    #[error("kimenet írása sikertelen: {0}")]
    Write(#[from] std::io::Error),
    /// A JSON-szerializálás nem sikerült.
    #[error("JSON-hiba: {0}")]
    Json(#[from] serde_json::Error),
    /// Nincs a kiterjesztést kezelő regisztrált nyelv.
    #[error("{}: nincs hozzá regisztrált nyelv", .0.display())]
    Unsupported(PathBuf),
    /// Hibás parancssori használat.
    #[error("használat: {0}")]
    Usage(String),
}

/// A crate eredmény-típusa.
pub type Result<T> = std::result::Result<T, CallgraphError>;
