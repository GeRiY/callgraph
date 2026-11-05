//! Közös hiba-típus: minden réteg ezt adja vissza, a CLI a stderr-re írja (a szöveg a beállított nyelven).

use std::path::PathBuf;

use thiserror::Error;

/// Az elemzés során előforduló, fájl-szintű hibák.
#[derive(Debug, Error)]
pub enum CallgraphError {
    /// A fájl nem olvasható.
    #[error("{}", read_msg(path, source))]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// A kimenet írása nem sikerült.
    #[error("{}", write_msg(.0))]
    Write(#[from] std::io::Error),
    /// A JSON-szerializálás nem sikerült.
    #[error("{}", json_msg(.0))]
    Json(#[from] serde_json::Error),
    /// Nincs a kiterjesztést kezelő regisztrált nyelv.
    #[error("{}", unsupported_msg(.0))]
    Unsupported(PathBuf),
    /// Hibás parancssori használat.
    #[error("{}", usage_msg(.0))]
    Usage(String),
}

fn read_msg(path: &std::path::Path, source: &std::io::Error) -> String {
    crate::tf!(
        "{}: cannot be read: {source}",
        "{}: nem olvasható: {source}",
        path.display()
    )
}

fn write_msg(e: &std::io::Error) -> String {
    crate::tf!(
        "writing output failed: {e}",
        "kimenet írása sikertelen: {e}"
    )
}

fn json_msg(e: &serde_json::Error) -> String {
    crate::tf!("JSON error: {e}", "JSON-hiba: {e}")
}

fn unsupported_msg(path: &std::path::Path) -> String {
    crate::tf!(
        "{}: no language registered for it",
        "{}: nincs hozzá regisztrált nyelv",
        path.display()
    )
}

fn usage_msg(message: &str) -> String {
    crate::tf!("usage: {message}", "használat: {message}")
}

/// A crate eredmény-típusa.
pub type Result<T> = std::result::Result<T, CallgraphError>;
