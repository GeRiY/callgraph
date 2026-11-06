//! callgraph: forráskód-elemző, ami fájlonként egy JSON-gráfot (JSONL sort) állít elő.
//! Az architektúra: `ARCHITECTURE.md`.

pub mod analysis;
pub mod cli;
pub mod core;
pub mod error;
pub mod graph;
pub mod hook;
pub mod i18n;
pub mod index;
pub mod io;
pub mod lang;
pub mod locate;
pub mod mcp;
pub mod model;
pub mod notes;
pub mod project;
pub mod query;
pub mod skill;
pub mod update;
