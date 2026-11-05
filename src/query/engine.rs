//! A query facade: projektútvonalakból elemzett vagy cache-elt gráfokat ad.

mod grep_text;
mod loader;

use super::filter::FilterChain;
use super::selector::SelectorSet;
use crate::analysis::Analyzer;
use crate::error::{CallgraphError, Result};
use crate::io::FileWalker;
use crate::model::FileGraph;
use crate::tf;
use std::path::{Path, PathBuf};

pub struct QueryResult {
    pub graph: FileGraph,
    pub methods: Option<Vec<String>>,
}
pub struct QueryEngine {
    root: PathBuf,
    label: String,
    analyzer: Analyzer,
}

impl QueryEngine {
    pub fn new(project: Option<&Path>, label: Option<String>) -> Self {
        let requested = project
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        let root = requested.canonicalize().unwrap_or(requested);
        let label = label.unwrap_or_else(|| {
            project
                .and_then(|p| p.to_str())
                .unwrap_or_else(|| {
                    root.file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("project")
                })
                .to_owned()
        });
        Self {
            root,
            label,
            analyzer: Analyzer::with_defaults(),
        }
    }
    pub fn label(&self) -> &str {
        &self.label
    }
    pub fn query(&self, selectors: &SelectorSet) -> Result<Vec<QueryResult>> {
        let files = self.files(selectors)?;
        let mut out = Vec::new();
        let mut cache = self.cache();
        for (file, methods) in files {
            let abs = self.root.join(&file);
            if !abs.is_file() {
                return Err(self.not_found(&abs));
            }
            let graph = self.load(&mut cache, &abs)?;
            let methods = methods.map(|names| {
                // névegyezés: egy név több metódusra is illeszkedhet (Rust: több impl, PHP/Swift: túlterhelés),
                // ezért a hiányt név szerint nézzük, nem darabszámra
                let found = graph.methods.iter().filter(|m| names.iter().any(|n| n == &m.name)).map(|m| m.name.clone()).collect::<Vec<_>>();
                if let Some(missing) = names.iter().find(|n| !found.contains(n)) {
                    return Err(CallgraphError::Usage(tf!(
                        "No such method: {}::{} (see --format outline for the file's full list)",
                        "Nincs ilyen metódus: {}::{} (lásd: --format outline a fájl teljes listájához)",
                        abs.display(),
                        missing
                    )));
                }
                Ok(found)
            }).transpose()?;
            out.push(QueryResult { graph, methods });
        }
        Ok(out)
    }
    pub fn query_filtered(
        &self,
        selectors: &SelectorSet,
        pattern: Option<&str>,
        categories: Option<&[String]>,
    ) -> Result<Vec<QueryResult>> {
        let rows = self.query(selectors)?;
        let chain = FilterChain::new(pattern, categories)
            .map_err(|e| CallgraphError::Usage(tf!("invalid pattern: {e}", "hibás minta: {e}")))?;
        if pattern.is_none() && categories.is_none() {
            return Ok(rows);
        }
        Ok(rows
            .into_iter()
            .filter_map(|mut row| {
                row.graph = chain.apply(&row.graph)?;
                row.methods = None;
                Some(row)
            })
            .collect())
    }
    fn files(&self, selectors: &SelectorSet) -> Result<Vec<(PathBuf, Option<Vec<String>>)>> {
        let mut result: Vec<(PathBuf, Option<Vec<String>>)> = Vec::new();
        if selectors.selected {
            for s in &selectors.items {
                let rel = s.file.clone();
                let entry = result.iter_mut().find(|entry| entry.0 == rel);
                if let Some((_, ms)) = entry {
                    ms.get_or_insert_with(Vec::new).extend(s.method.clone());
                } else {
                    result.push((rel, Some(vec![s.method.clone().unwrap()])));
                }
            }
        } else {
            for input in &selectors.items {
                let abs = self.root.join(&input.file);
                if abs.is_file() {
                    result.push((input.file.clone(), None));
                } else if abs.is_dir() {
                    let mut warn = |_| {};
                    let files = FileWalker::for_registry(self.analyzer.registry())
                        .collect(std::slice::from_ref(&abs), &mut warn);
                    for f in self.legacy_order(files) {
                        result.push((f.strip_prefix(&self.root).unwrap_or(&f).to_path_buf(), None));
                    }
                } else {
                    return Err(self.not_found(&abs));
                }
            }
        }
        Ok(result)
    }
    /// A „nincs ilyen fájl/mappa” hiba (a `Not found: ` / `Nem található: ` előtagot a CLI hibaosztályozója figyeli).
    fn not_found(&self, abs: &Path) -> CallgraphError {
        let (path, label) = (abs.display(), &self.label);
        CallgraphError::Usage(tf!(
            "Not found: {path} (relative to the --project {label} root)",
            "Nem található: {path} (--project {label} gyökeréhez képest)"
        ))
    }
}
