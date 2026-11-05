//! Projekt-index Repository: mtime- és méret-alapú, fájlonkénti JSONL rekordok.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::model::FileGraph;

use super::record::{FileStamp, IndexRecord};

/// A `.callgraph/index.jsonl` tárolója.
#[derive(Debug)]
pub struct IndexStore {
    path: PathBuf,
    records: HashMap<String, IndexRecord>,
}

impl IndexStore {
    pub fn new(path: PathBuf) -> Self {
        let records = fs::read_to_string(&path)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str::<IndexRecord>(line).ok())
            .map(|record| (record.path.clone(), record))
            .collect();
        Self { path, records }
    }

    pub fn get(&self, path: &Path, stamp: FileStamp) -> Option<FileGraph> {
        self.records
            .get(path.to_str()?)
            .filter(|record| record.stamp() == stamp)
            .map(|record| record.graph.clone())
    }

    pub fn contains(&self, path: &Path) -> bool {
        path.to_str()
            .is_some_and(|key| self.records.contains_key(key))
    }

    pub fn put(&mut self, path: &Path, stamp: FileStamp, graph: FileGraph) {
        let key = path.to_string_lossy().into_owned();
        self.records
            .insert(key.clone(), IndexRecord::new(key, stamp, graph));
    }

    pub fn retain_paths(&mut self, paths: &[PathBuf]) {
        self.records.retain(|path, _| {
            paths
                .iter()
                .any(|candidate| candidate.to_string_lossy() == *path)
        });
    }

    pub fn records(&self) -> impl Iterator<Item = IndexRecord> + '_ {
        self.records.values().cloned()
    }

    pub fn save(&self) -> Result<()> {
        let mut rows: Vec<_> = self.records.values().collect();
        rows.sort_by(|a, b| a.path.cmp(&b.path));
        let text = rows
            .into_iter()
            .map(serde_json::to_string)
            .collect::<std::result::Result<Vec<_>, _>>()?
            .join("\n");
        fs::write(
            &self.path,
            if text.is_empty() {
                text
            } else {
                format!("{text}\n")
            },
        )?;
        Ok(())
    }
}
