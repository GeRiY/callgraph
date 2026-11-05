//! A JSONL cache elavult és már nem létező bejegyzéseinek eltávolítása.

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::cli::Command;
use crate::error::Result;
use crate::io::{CacheStore, JsonlCacheStore};
use crate::project::{ProjectLocator, ProjectStore};

/// Tömöríti a cache-t, opcionálisan csak a várható eredményt írva ki.
pub struct PruneCommand {
    paths: Vec<PathBuf>,
    cache: Option<PathBuf>,
    dry_run: bool,
}

impl PruneCommand {
    /// Új cache-takarító parancs.
    pub fn new(paths: Vec<PathBuf>, cache: Option<PathBuf>, dry_run: bool) -> Self {
        Self {
            paths,
            cache,
            dry_run,
        }
    }
}

impl Command for PruneCommand {
    fn run(&self, out: &mut dyn Write, _err: &mut dyn Write) -> Result<i32> {
        let paths = if self.paths.is_empty() {
            vec![PathBuf::from(".")]
        } else {
            self.paths.clone()
        };
        let caches = if let Some(cache) = &self.cache {
            vec![cache.clone()]
        } else {
            ProjectLocator::default()
                .group(&paths)
                .into_iter()
                .map(|group| ProjectStore::new(&group.project).cache_path())
                .collect()
        };
        let mut total = crate::io::CompactStats::default();
        for cache in caches {
            let stats = if self.dry_run {
                count_prune(&cache)?
            } else {
                JsonlCacheStore::new(cache).prune()?
            };
            total.kept += stats.kept;
            total.dropped += stats.dropped;
        }
        writeln!(
            out,
            "{}",
            crate::tf!(
                "{} entries kept / {} dropped",
                "{} bejegyzés maradt / {} dobódott",
                total.kept,
                total.dropped
            )
        )?;
        Ok(0)
    }
}

#[derive(Deserialize)]
struct Header {
    path: String,
    version: String,
}

#[derive(Deserialize)]
struct Row {
    graph: crate::model::FileGraph,
}

#[derive(Default)]
struct Counts {
    rows: usize,
    current: HashMap<String, String>,
}

/// A prune művelet eredményét számolja ki anélkül, hogy fájlt nyitna írásra.
fn count_prune(file: &Path) -> Result<crate::io::CompactStats> {
    let text = match fs::read_to_string(file) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(crate::io::CompactStats::default());
        }
        Err(error) => return Err(error.into()),
    };
    let version = crate::io::tool_version();
    let mut counts = Counts::default();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        counts.rows += 1;
        let Ok(header) = serde_json::from_str::<Header>(line) else {
            continue;
        };
        if (header.version == version || header.version.starts_with(&format!("{version}+lang=")))
            && Path::new(&header.path).exists()
        {
            counts.current.insert(header.path, line.to_owned());
        }
    }
    counts.current.retain(|_, line| {
        serde_json::from_str::<Row>(line)
            .map(|row| {
                let _ = row.graph;
                true
            })
            .unwrap_or(false)
    });
    Ok(crate::io::CompactStats {
        kept: counts.current.len(),
        dropped: counts.rows.saturating_sub(counts.current.len()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{FileGraph, Imports};
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let n = COUNTER.fetch_add(1, Ordering::SeqCst);
            let path =
                std::env::temp_dir().join(format!("callgraph-prune-{}-{n}", std::process::id()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn path(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn graph() -> FileGraph {
        FileGraph {
            file: "alive.php".into(),
            framework: None,
            namespace: None,
            imports: Imports::default(),
            containers: vec![],
            properties: vec![],
            constants: vec![],
            methods: vec![],
            calls: vec![],
            comments: vec![],
            control_flow: vec![],
            strings: vec![],
            writes: vec![],
            module: None,
            sfc: None,
        }
    }

    #[test]
    fn dry_run_counts_prunable_rows_without_modifying_cache() {
        let temp = TempDir::new();
        let alive = temp.path("alive.php");
        let dead = temp.path("dead.php");
        fs::write(&alive, "<?php").unwrap();
        fs::write(&dead, "<?php").unwrap();
        let cache = temp.path("cache.jsonl");
        let mut store = JsonlCacheStore::new(cache.clone());
        store.put(&alive, 1, &graph()).unwrap();
        store.put(&dead, 1, &graph()).unwrap();
        fs::remove_file(&dead).unwrap();
        let before = fs::read(&cache).unwrap();

        let stats = count_prune(&cache).unwrap();

        assert_eq!(stats.kept, 1);
        assert_eq!(stats.dropped, 1);
        assert_eq!(fs::read(&cache).unwrap(), before);
    }
}
