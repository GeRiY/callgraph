//! A Rust nyelv-modul (`RustBackend`, kiterjesztés: `.rs`): syn-parser (Adapter), visitorok,
//! hívás-kategorizálás (`core::Chain`), framework-stratégiák (`Cargo.toml`).
//!
//! A modul neve `rs` (és nem `rust`): a fájlkiterjesztést követi, és nem keveredik a `rust`
//! toolchain- / std-elnevezésekkel; a nyelv azonosítója (`Language::id`) ettől még `"rust"`.

mod backend;
mod categorize;
mod comment_scan;
mod context;
mod framework;
mod module_path;
mod parser;
mod resolve;
mod visitor;

pub use backend::RustBackend;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_edge;
#[cfg(test)]
mod tests_receivers;
