//! Go frontend: tree-sitter-go parser (Adapter), Visitor a közös fájlgráfba, hívás-kategorizálás.
//! Leképezés: `docs/LANG-GO.md`.

mod categorize;
mod frontend;
mod known;
mod parser;
mod visitor;

pub use frontend::GoFrontend;
