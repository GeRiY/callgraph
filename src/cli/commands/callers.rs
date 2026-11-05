//! A projekt-indexből fordított hívókat kiíró Command.

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::analysis::Analyzer;
use crate::cli::Command;
use crate::error::Result;
use crate::graph::ProjectGraph;
use crate::io::FileWalker;
use crate::project::{Project, ProjectLocator, ProjectStore};

/// A projekten belüli, minta szerinti hívókat JSONL-ként írja ki.
pub struct CallersCommand {
    pattern: String,
    depth: usize,
    project: Option<PathBuf>,
}

impl CallersCommand {
    pub fn new(pattern: String, depth: usize, project: Option<PathBuf>) -> Self {
        Self {
            pattern,
            depth,
            project,
        }
    }

    /// A projekt-gyökér ugyanúgy, mint az elemzésnél (ProjectLocator): a megadott útról vagy a munkakönyvtárról
    /// felfelé a legközelebbi composer.json / package.json / .git — így a cache és az index egy helyen van.
    fn root(&self) -> Project {
        let start = self.project.as_deref().unwrap_or_else(|| Path::new("."));
        ProjectLocator::default().locate(start)
    }

    /// A projekt-gráfból (`.callgraph/`, a folyamatban közös példány) a minta hívói; előtte a gráf
    /// naprakészre hozva: csak a változott fájlok elemződnek újra, a törölt / átnevezett fájlok kikerülnek.
    fn callers(&self, project: &Project) -> Result<Vec<crate::index::CallerRow>> {
        let store = ProjectStore::new(project);
        store.ensure()?;
        let analyzer = Analyzer::with_defaults();
        let files = crate::graph::legacy_order(
            analyzer.registry(),
            FileWalker::for_registry(analyzer.registry())
                .collect(&[project.root().to_path_buf()], &mut |_| {}),
        );
        let shared = crate::graph::shared(project.root());
        let mut guard = shared.as_ref().map(crate::graph::lock);
        let mut owned = None;
        let graph: &mut ProjectGraph = match guard.as_deref_mut() {
            Some(graph) => graph,
            None => owned.insert(ProjectGraph::open(store.dir().to_path_buf())),
        };
        graph.refresh(&files, true, &analyzer)?;
        graph.ensure_callers(&analyzer)?;
        graph.save()?;
        graph.caller_index().callers(&self.pattern, self.depth)
    }
}

impl Command for CallersCommand {
    fn run(&self, out: &mut dyn Write, _err: &mut dyn Write) -> Result<i32> {
        for row in self.callers(&self.root())? {
            serde_json::to_writer(&mut *out, &row)?;
            writeln!(out)?;
        }
        Ok(0)
    }
}
