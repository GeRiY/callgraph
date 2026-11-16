//! Kotlin frontend (Android / Jetpack Compose-tudatos): tree-sitter-kotlin-ng parser (Adapter), Visitor a közös
//! fájlgráfba, hívás-kategorizálás a `core::Chain`-en. Leképezés a Swift-frontend mintájára.

mod categorize;
mod frontend;
mod known;
mod parser;
mod visitor;

pub use frontend::KotlinFrontend;
