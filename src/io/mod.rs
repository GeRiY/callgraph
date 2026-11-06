//! I/O-réteg: fájlbejárás, JSONL-kiírás, cache.

mod cache;
mod jsonl_writer;
mod walker;

pub use cache::{CacheStore, CompactStats, JsonlCacheStore, SCHEMA, tool_version};
pub use jsonl_writer::JsonlWriter;
pub use walker::{FileWalker, IGNORE_FILE, excluded_extensions, parse_ignore};
