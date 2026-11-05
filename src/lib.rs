//! callgraph: forráskód-elemző, ami fájlonként egy JSON-gráfot (JSONL sort) állít elő.
//! Az architektúra: `ARCHITECTURE.md`.

pub mod analysis;
pub mod cli;
pub mod core;
pub mod error;
pub mod index;
pub mod io;
pub mod lang;
pub mod model;
pub mod project;
pub mod query;
pub mod skill;
