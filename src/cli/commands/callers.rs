//! A projekt-indexből fordított hívókat kiíró Command.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::analysis::Analyzer;
use crate::cli::Command;
use crate::error::Result;
use crate::index::{CallerIndex, FileStamp, IndexStore};
use crate::io::{CacheStore, FileWalker, JsonlCacheStore};
use crate::lang::LanguageRegistry;
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

    fn build_index(&self, project: &Project) -> Result<CallerIndex> {
        let store = ProjectStore::new(project);
        store.ensure()?;
        let registry = LanguageRegistry::with_defaults();
        let files = FileWalker::for_registry(&registry)
            .collect(&[project.root().to_path_buf()], &mut |_| {});
        let mut index = IndexStore::new(store.index_path());
        index.retain_paths(&files);
        let analyzer = Analyzer::new(registry);
        let mut cache = JsonlCacheStore::new(store.cache_path());
        for file in &files {
            let stamp = FileStamp::of(file);
            if index.get(file, stamp).is_some() {
                continue;
            }
            let changed = index.contains(file);
            let seconds = mtime_seconds(file).unwrap_or_default();
            let language = analyzer
                .registry()
                .for_path(file)
                .map(|language| language.id())
                .unwrap_or("unknown");
            let graph = if !changed {
                match cache.get_for_language(file, seconds, language) {
                    Some(graph) => graph,
                    None => analyzer.analyze_path(file)?.graph,
                }
            } else {
                analyzer.analyze_path(file)?.graph
            };
            let _ = cache.put_for_language(file, seconds, &graph, language);
            index.put(file, stamp, graph);
        }
        index.save()?;
        Ok(CallerIndex::from_records(index.records()))
    }
}

impl Command for CallersCommand {
    fn run(&self, out: &mut dyn Write, _err: &mut dyn Write) -> Result<i32> {
        let index = self.build_index(&self.root())?;
        for row in index.callers(&self.pattern, self.depth)? {
            serde_json::to_writer(&mut *out, &row)?;
            writeln!(out)?;
        }
        Ok(0)
    }
}

fn mtime_seconds(path: &Path) -> Option<u64> {
    path.metadata()
        .ok()?
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs())
}
