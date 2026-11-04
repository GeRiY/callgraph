//! Repository: mtime- és nyelvazonosító-alapú JSONL cache.

mod jsonl;
mod locking;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::model::FileGraph;

/// A gráf-séma verziója.
pub const SCHEMA: u32 = 1;

/// Az eszköz alapverziója a cache-ben.
pub fn tool_version() -> String {
    // + a forrás ujjlenyomata (build.rs): egy új build a régi elemzéseket nem használja újra
    format!(
        "{}+s{}+{}",
        env!("CARGO_PKG_VERSION"),
        SCHEMA,
        env!("CALLGRAPH_SRC_HASH")
    )
}

pub(crate) fn language_version(language_id: &str) -> String {
    format!("{}+lang={language_id}", tool_version())
}

/// Mit változtatott egy `compact()` / `prune()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CompactStats {
    pub kept: usize,
    pub dropped: usize,
}

/// Cserélhető tároló az elemzett gráfokhoz.
pub trait CacheStore: Send + Sync {
    fn get(&self, path: &Path, mtime_secs: u64) -> Option<FileGraph>;
    fn put(&mut self, path: &Path, mtime_secs: u64, graph: &FileGraph) -> Result<()>;
    fn compact(&mut self) -> Result<CompactStats>;
    fn prune(&mut self) -> Result<CompactStats>;

    /// Nyelvspecifikus olvasás; az alapértelmezés a régi, egynyelvű API.
    fn get_for_language(
        &self,
        path: &Path,
        mtime_secs: u64,
        _language_id: &str,
    ) -> Option<FileGraph> {
        self.get(path, mtime_secs)
    }

    /// Nyelvspecifikus írás; az alapértelmezés a régi, egynyelvű API.
    fn put_for_language(
        &mut self,
        path: &Path,
        mtime_secs: u64,
        graph: &FileGraph,
        _language_id: &str,
    ) -> Result<()> {
        self.put(path, mtime_secs, graph)
    }
}

#[derive(Serialize)]
pub(crate) struct RowOut<'a> {
    pub path: &'a str,
    pub mtime: u64,
    pub version: &'a str,
    pub graph: &'a FileGraph,
}

#[derive(Deserialize)]
pub(crate) struct RowIn {
    pub graph: FileGraph,
}

#[derive(Deserialize)]
pub(crate) struct Header {
    pub path: String,
    pub mtime: u64,
    pub version: String,
}

#[derive(Debug, Clone)]
pub(crate) struct Slot {
    pub mtime: u64,
    pub version: String,
    pub line: String,
}

/// JSONL-fájl alapú [`CacheStore`].
#[derive(Debug)]
pub struct JsonlCacheStore {
    pub(crate) file: PathBuf,
    pub(crate) version: String,
    pub(crate) index: HashMap<String, Slot>,
}

impl JsonlCacheStore {
    pub fn new(file: PathBuf) -> Self {
        Self::with_version(file, tool_version())
    }

    pub fn with_version(file: PathBuf, version: String) -> Self {
        let mut store = Self {
            file,
            version,
            index: HashMap::new(),
        };
        // a cache csak hozzáfűz: ha az elavult / ismétlődő sor több, mint a használható, tömörítünk
        if let Ok(stale) = store.reload()
            && stale > store.index.len().max(200)
        {
            let _ = store.compact();
        }
        store
    }

    /// Újraolvasás; a visszatérési érték az elavult vagy ismétlődő (nem használt) sorok száma.
    pub fn reload(&mut self) -> Result<usize> {
        let _lock = self.lock(false)?;
        let (index, total) = self.read_index()?;
        self.index = index;
        Ok(total.saturating_sub(self.index.len()))
    }

    pub fn len(&self) -> usize {
        self.index.len()
    }
    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    pub(crate) fn read_index(&self) -> Result<(HashMap<String, Slot>, usize)> {
        jsonl::read_index(self)
    }

    pub(crate) fn rewrite(&mut self, keep: impl Fn(&str) -> bool) -> Result<CompactStats> {
        jsonl::rewrite(self, keep)
    }
}

impl CacheStore for JsonlCacheStore {
    fn get(&self, path: &Path, mtime_secs: u64) -> Option<FileGraph> {
        self.get_for_version(path, mtime_secs, &self.version)
    }

    fn put(&mut self, path: &Path, mtime_secs: u64, graph: &FileGraph) -> Result<()> {
        let version = self.version.clone();
        self.put_for_version(path, mtime_secs, graph, &version)
    }

    fn get_for_language(
        &self,
        path: &Path,
        mtime_secs: u64,
        language_id: &str,
    ) -> Option<FileGraph> {
        self.get_for_version(path, mtime_secs, &language_version(language_id))
    }

    fn put_for_language(
        &mut self,
        path: &Path,
        mtime_secs: u64,
        graph: &FileGraph,
        language_id: &str,
    ) -> Result<()> {
        let version = language_version(language_id);
        self.put_for_version(path, mtime_secs, graph, &version)
    }

    fn compact(&mut self) -> Result<CompactStats> {
        let _lock = self.lock(true)?;
        self.rewrite(|_| true)
    }

    fn prune(&mut self) -> Result<CompactStats> {
        let _lock = self.lock(true)?;
        self.rewrite(|p| Path::new(p).exists())
    }
}
