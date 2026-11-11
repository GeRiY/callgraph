//! Python frontend: tree-sitter-python parser (Adapter), Visitor a közös fájlgráfba, hívás-kategorizálás.
//! Leképezés: `docs/LANG-PYTHON.md`.

mod categorize;
mod frontend;
mod known;
mod parser;
mod visitor;

pub use frontend::PythonFrontend;
