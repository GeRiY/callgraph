//! I/O-réteg: fájlbejárás (a kizárási szabályokkal), JSONL-kiírás, cache.

mod cache;
mod ignore_rules;
mod jsonl_writer;
mod legacy_tables;
mod source_text;
mod walker;

pub use cache::{CacheStore, CompactStats, JsonlCacheStore, SCHEMA, tool_version};
pub use ignore_rules::{
    GITIGNORE_ENV, IGNORE_FILE, PROTECT_MARK, deps_ignore_patterns, excluded_extensions,
    gitignore_enabled, is_protected, parse_ignore, path_patterns, protect_mark,
};
pub use jsonl_writer::JsonlWriter;
pub use source_text::{decode_source, read_source_lossy};
pub use walker::FileWalker;
