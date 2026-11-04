//! Gráf-betöltés: cache-ből, ha friss, különben elemzés; teljes hatókör bejárása.

use super::QueryEngine;
use crate::error::Result;
use crate::io::{CacheStore, FileWalker, JsonlCacheStore};
use crate::model::FileGraph;
use crate::project::ProjectStore;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

impl QueryEngine {
    pub(super) fn all_graphs(&self, scope: &[PathBuf]) -> Result<Vec<(String, FileGraph)>> {
        let files = self.legacy_order(
            FileWalker::for_registry(self.analyzer.registry()).collect(scope, &mut |_| {}),
        );
        let mut cache = self.cache();
        files
            .into_iter()
            .map(|path| {
                let graph = self.load(&mut cache, &path)?;
                Ok((path.to_string_lossy().into_owned(), graph))
            })
            .collect()
    }
    /// A régi callgraph sorrendje: előbb a PHP-motor (fájlrendszer-sorrend), majd a JS-motor
    /// (`readdirSync().sort()`: mappánként név szerint, ami komponensenkénti összehasonlítás).
    pub(super) fn legacy_order(&self, files: Vec<PathBuf>) -> Vec<PathBuf> {
        let registry = self.analyzer.registry();
        let (mut js, mut rest): (Vec<_>, Vec<_>) = files
            .into_iter()
            .partition(|f| registry.for_path(f).is_some_and(|l| l.id() == "js"));
        js.sort();
        rest.extend(js);
        rest
    }

    /// A projekt `.callgraph/` tárolója: létrehozza (a `*`-os `.gitignore`-ral együtt), hogy ne kerüljön gitbe.
    /// Ha nem hozható létre (pl. írásvédett projekt), a cache nélkül megy tovább.
    pub(super) fn cache(&self) -> Option<JsonlCacheStore> {
        let store = ProjectStore::at_root(&self.root);
        store.ensure().ok()?;
        Some(JsonlCacheStore::new(store.cache_path()))
    }

    /// Cache-ből, ha friss; különben elemzés, és a gráf visszaírása (az írás hibája nem állítja meg a lekérdezést).
    pub(super) fn load(
        &self,
        cache: &mut Option<JsonlCacheStore>,
        path: &Path,
    ) -> Result<FileGraph> {
        let id = self
            .analyzer
            .registry()
            .for_path(path)
            .map(|l| l.id())
            .unwrap_or("unknown");
        let seconds = mtime(path);
        if let (Some(store), Some(m)) = (cache.as_ref(), seconds)
            && let Some(graph) = store.get_for_language(path, m, id)
        {
            return Ok(graph);
        }
        let graph = self.analyzer.analyze_path(path)?.graph;
        if let (Some(store), Some(m)) = (cache.as_mut(), seconds) {
            let _ = store.put_for_language(path, m, &graph, id);
        }
        Ok(graph)
    }
}

fn mtime(path: &Path) -> Option<u64> {
    path.metadata()
        .ok()?
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}
