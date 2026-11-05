//! Swift frontend (SwiftUI-tudatos): tree-sitter-swift parser (Adapter), Visitor a közös fájlgráfba,
//! hívás-kategorizálás a `core::Chain`-en. Leképezés: `docs/LANG-SWIFT.md`.

mod categorize;
mod frontend;
mod known;
mod parser;
mod visitor;

pub use frontend::SwiftFrontend;
