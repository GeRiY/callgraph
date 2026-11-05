//! Az elemző parancs: fájlok / mappák → JSONL (fájlonként egy sor) a kimenetre.
//!
//! Az elemzés `rayon`-nal párhuzamos, de a kimenet a bejáró sorrendjét követi (az indexelt
//! párhuzamos iterátor `collect`-je megőrzi a sorrendet). A cache (mtime + eszközverzió) találatnál
//! kihagyja az elemzést; a cache-ből jövő fájlokról a szintaktikai figyelmeztetések nem ismétlődnek.

use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::time::UNIX_EPOCH;

use rayon::prelude::*;

use crate::analysis::Analyzer;
use crate::cli::Command;
use crate::error::Result;
use crate::io::{CacheStore, FileWalker, JsonlWriter};
use crate::lang::LanguageRegistry;
use crate::model::FileGraph;
use crate::project::{ProjectLocator, ProjectStore};

/// Egy fájl feldolgozásának eredménye (a kiírás sorrendben, szekvenciálisan történik).
enum Outcome {
    Graph(Box<FileGraph>, Vec<String>),
    Failed(String),
}

/// Elemzi a megadott útvonalakat; hibánál a hibacsatornára ír és folytatja.
pub struct AnalyzeCommand {
    paths: Vec<PathBuf>,
    language_ids: Option<HashSet<String>>,
    cache: CacheMode,
    locator: ProjectLocator,
    walker: FileWalker,
    analyzer: Analyzer,
}

enum CacheMode {
    None,
    Explicit(RwLock<Box<dyn CacheStore>>),
    Project,
}

impl AnalyzeCommand {
    /// `cache`: a cache-fájl útja, `None` = cache nélkül. A nyelveket a `registry` adja.
    pub fn new(
        paths: Vec<PathBuf>,
        language_ids: Option<Vec<String>>,
        cache: Option<PathBuf>,
        no_cache: bool,
        registry: LanguageRegistry,
    ) -> Self {
        let language_ids: Option<HashSet<String>> =
            language_ids.map(|ids| ids.into_iter().collect());
        let extensions = if let Some(ids) = language_ids.as_ref() {
            let ids: Vec<&str> = ids.iter().map(String::as_str).collect();
            registry.extensions_for(&ids)
        } else {
            registry.extensions()
        };
        Self {
            paths,
            language_ids,
            cache: if no_cache {
                CacheMode::None
            } else if let Some(path) = cache {
                CacheMode::Explicit(RwLock::new(Box::new(crate::io::JsonlCacheStore::new(path))))
            } else {
                CacheMode::Project
            },
            locator: ProjectLocator::default(),
            walker: FileWalker::for_extensions(extensions),
            analyzer: Analyzer::new(registry),
        }
    }

    fn process(
        analyzer: &Analyzer,
        cache: Option<&RwLock<Box<dyn CacheStore>>>,
        file: &Path,
        language_id: &str,
    ) -> Outcome {
        let mtime = mtime_secs(file);
        if let (Some(cache), Some(m)) = (cache, mtime)
            && let Ok(store) = cache.read()
            && let Some(graph) = store.get_for_language(file, m, language_id)
        {
            return Outcome::Graph(Box::new(graph), Vec::new());
        }
        match analyzer.analyze_path(file) {
            Ok(analysis) => {
                if let (Some(cache), Some(m)) = (cache, mtime)
                    && let Ok(mut store) = cache.write()
                {
                    // A cache-írás hibája nem végzetes: a kimenet enélkül is helyes.
                    let _ = store.put_for_language(file, m, &analysis.graph, language_id);
                }
                Outcome::Graph(Box::new(analysis.graph), analysis.warnings)
            }
            Err(e) => Outcome::Failed(e.to_string()),
        }
    }

    fn project_caches(
        &self,
        files: &[PathBuf],
    ) -> std::collections::HashMap<PathBuf, RwLock<Box<dyn CacheStore>>> {
        let mut caches = std::collections::HashMap::new();
        for group in self.locator.group(files) {
            let store = ProjectStore::new(&group.project);
            if store.ensure().is_ok() {
                caches.insert(
                    group.project.root().to_path_buf(),
                    RwLock::new(
                        Box::new(crate::io::JsonlCacheStore::new(store.cache_path()))
                            as Box<dyn CacheStore>,
                    ),
                );
            }
        }
        caches
    }
}

/// A fájl módosítási ideje másodpercben (a cache érvényességi kulcsa).
fn mtime_secs(path: &Path) -> Option<u64> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    modified
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}

impl Command for AnalyzeCommand {
    /// Kilépési kód: 0, ha minden fájl elemezhető volt; 1, ha valamelyik nem olvasható / nem írható.
    fn run(&self, out: &mut dyn Write, err: &mut dyn Write) -> Result<i32> {
        let mut failed = false;
        let mut note = |msg: String| {
            let _ = writeln!(err, "{msg}");
        };
        if let Some(language_ids) = &self.language_ids {
            for path in &self.paths {
                if path.is_file()
                    && let Some(language) = self.analyzer.registry().for_path(path)
                    && !language_ids.contains(language.id())
                {
                    note(format!(
                        "{}: kihagyva a --lang szűrő miatt (nyelv: {})",
                        path.display(),
                        language.id()
                    ));
                }
            }
        }
        let files = self.walker.collect(&self.paths, &mut note);
        let project_caches = match &self.cache {
            CacheMode::Project => Some(self.project_caches(&files)),
            _ => None,
        };

        // A nyelvek `Send + Sync`-ek: egy megosztott Analyzer szolgál ki minden szálat.
        let outcomes: Vec<Outcome> = files
            .par_iter()
            .map(|file| {
                let language_id = self
                    .analyzer
                    .registry()
                    .for_path(file)
                    .map(|language| language.id())
                    .unwrap_or("unknown");
                let cache = match &self.cache {
                    CacheMode::None => None,
                    CacheMode::Explicit(cache) => Some(cache),
                    CacheMode::Project => {
                        let root = self.locator.locate(file).root().to_path_buf();
                        project_caches.as_ref().and_then(|c| c.get(&root))
                    }
                };
                Self::process(&self.analyzer, cache, file, language_id)
            })
            .collect();

        let mut writer = JsonlWriter::new(out);
        for (file, outcome) in files.iter().zip(outcomes) {
            match outcome {
                Outcome::Graph(graph, warnings) => {
                    warnings.into_iter().for_each(&mut note);
                    if let Err(e) = writer.write_line(&*graph) {
                        note(format!("{}: {e}", file.display()));
                        failed = true;
                    }
                }
                Outcome::Failed(msg) => {
                    note(msg);
                    failed = true;
                }
            }
        }
        Ok(i32::from(failed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn language_filter_selects_only_requested_files() {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("callgraph-lang-filter-{id}"));
        fs::create_dir_all(&dir).unwrap();
        for (name, contents) in [
            ("a.php", "<?php echo 1;"),
            ("b.json", "{\"ok\": true}"),
            ("c.md", "# title"),
            ("d.css", "body { color: red; }"),
        ] {
            fs::write(dir.join(name), contents).unwrap();
        }

        let run = |lang| {
            let command = AnalyzeCommand::new(
                vec![dir.clone()],
                lang,
                None,
                true,
                LanguageRegistry::with_defaults(),
            );
            let mut out = Vec::new();
            let mut err = Vec::new();
            assert_eq!(command.run(&mut out, &mut err).unwrap(), 0);
            (out, err)
        };

        assert_eq!(run(None).0.iter().filter(|byte| **byte == b'\n').count(), 4);
        assert_eq!(
            run(Some(vec!["json".into(), "markdown".into()]))
                .0
                .iter()
                .filter(|byte| **byte == b'\n')
                .count(),
            2
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn explicit_file_outside_language_filter_is_reported() {
        let path =
            std::env::temp_dir().join(format!("callgraph-lang-filter-{}.php", std::process::id()));
        fs::write(&path, "<?php echo 1;").unwrap();
        let command = AnalyzeCommand::new(
            vec![path.clone()],
            Some(vec!["json".into()]),
            None,
            true,
            LanguageRegistry::with_defaults(),
        );
        let mut out = Vec::new();
        let mut err = Vec::new();
        assert_eq!(command.run(&mut out, &mut err).unwrap(), 0);
        assert!(
            String::from_utf8(err)
                .unwrap()
                .contains("--lang szűrő miatt")
        );
        assert!(out.is_empty());
        fs::remove_file(path).unwrap();
    }
}
