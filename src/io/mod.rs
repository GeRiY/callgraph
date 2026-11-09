//! I/O-réteg: fájlbejárás (a kizárási szabályokkal), JSONL-kiírás, cache.

mod cache;
mod ignore_rules;
mod jsonl_writer;
mod walker;

pub use cache::{CacheStore, CompactStats, JsonlCacheStore, SCHEMA, tool_version};
pub use ignore_rules::{
    GITIGNORE_ENV, IGNORE_FILE, excluded_extensions, gitignore_enabled, parse_ignore,
    path_patterns,
};
pub use jsonl_writer::JsonlWriter;
pub use walker::FileWalker;
