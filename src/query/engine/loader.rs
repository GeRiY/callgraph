//! Gráf-betöltés gráf nélkül (`CALLGRAPH_NO_GRAPH=1`, írásvédett projekt): elemzés; teljes hatókör bejárása.
//! A gráfos út a `crate::graph::ProjectGraph`-ban (a shardok a fájlonkénti cache is).

use super::QueryEngine;
use crate::error::Result;
use crate::io::FileWalker;
use crate::model::FileGraph;
use rayon::prelude::*;
use std::path::{Path, PathBuf};

impl QueryEngine {
    /// A hatókör minden fájljának gráfja (párhuzamos elemzés; hibánál a sorrendben első hibás fájl hibája).
    pub(super) fn all_graphs(&self, scope: &[PathBuf]) -> Result<Vec<(String, FileGraph)>> {
        let files = self.walk(scope);
        let graphs: Vec<Result<(String, FileGraph)>> = files
            .par_iter()
            .map(|path| Ok((path.to_string_lossy().into_owned(), self.analyze(path)?)))
            .collect();
        graphs.into_iter().collect()
    }

    /// A hatókör kódfájljai a régi bejárási sorrendben.
    pub(super) fn walk(&self, scope: &[PathBuf]) -> Vec<PathBuf> {
        self.legacy_order(
            FileWalker::for_registry(self.analyzer.registry()).collect(scope, &mut |_| {}),
        )
    }

    /// A projekt-gráf (`crate::graph`), ha ez a lekérdezés használja (`use_project_graph`) és a `.callgraph/`
    /// tároló létrehozható; a folyamatban gyökerenként egy példány (az MCP-szerverben kérésről kérésre megmarad).
    pub(super) fn project_graph(&self) -> Option<crate::graph::SharedGraph> {
        if !self.graph {
            return None;
        }
        crate::graph::shared(&self.root)
    }

    /// A régi callgraph bejárási sorrendje (`crate::graph::legacy_order`).
    pub(super) fn legacy_order(&self, files: Vec<PathBuf>) -> Vec<PathBuf> {
        crate::graph::legacy_order(self.analyzer.registry(), files)
    }

    /// Egy fájl elemzése (gráf nélküli út).
    pub(super) fn analyze(&self, path: &Path) -> Result<FileGraph> {
        Ok(self.analyzer.analyze_path(path)?.graph)
    }
}
