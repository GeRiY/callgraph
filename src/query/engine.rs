//! A query facade: projektútvonalakból elemzett vagy cache-elt gráfokat ad.

mod grep_text;
mod loader;

use super::filter::{FilterChain, pattern_regex};
use super::selector::SelectorSet;
use crate::analysis::Analyzer;
use crate::error::{CallgraphError, Result};
use crate::index::{TypeHierarchy, ViaMatcher};
use crate::io::FileWalker;
use crate::model::{Call, FileGraph};
use crate::tf;
use std::path::{Path, PathBuf};

pub struct QueryResult {
    pub graph: FileGraph,
    pub methods: Option<Vec<String>>,
    /// A `--grep` találatai közül azok, amelyek a mintára nem, csak rokon típuson (interfész, trait, szülő)
    /// át illeszkedtek.
    pub via: Vec<ViaCall>,
}

/// Egy rokon típuson át illesztett hívás: sor + cél (a `resolved` / `target`), és a hívás típusának rövid neve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViaCall {
    pub line: u32,
    pub target: String,
    pub via: String,
}

impl QueryResult {
    pub fn new(graph: FileGraph, methods: Option<Vec<String>>) -> Self {
        Self {
            graph,
            methods,
            via: Vec::new(),
        }
    }

    /// Ha a hívás rokon típuson át illeszkedett a mintára: a típus rövid neve (`via Resolver`).
    pub fn via_of(&self, call: &Call) -> Option<&str> {
        let target = call.resolved.as_deref().unwrap_or(&call.target);
        self.via
            .iter()
            .find(|v| v.line == call.line && v.target == target)
            .map(|v| v.via.as_str())
    }
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
    /// A `--depth` 2. szinttől kezdve keresett hatókör: a teljes projektgyökér (`--project`, különben a munkamappa).
    /// A hívók hívói bárhol lehetnek a projektben, nem csak a lekérdezett mappában (pl. `app/Services` → `routes/`).
    pub fn depth_scope(&self) -> Vec<PathBuf> {
        vec![self.root.clone()]
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
            out.push(QueryResult::new(graph, methods));
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
        if pattern.is_none() && categories.is_none() {
            return Ok(rows);
        }
        let invalid = |e: regex::Error| {
            CallgraphError::Usage(tf!("invalid pattern: {e}", "hibás minta: {e}"))
        };
        // Interfészen át: a hatókör típus-hierarchiájából (a mintára nem illeszkedő, de rokon típuson át hívott metódus).
        let via = match pattern {
            // csak `Típus::metódus` alakú mintára van értelme (lásd `ViaMatcher::new`)
            Some(pattern) if pattern.contains("::") => {
                let regex = pattern_regex(pattern).map_err(invalid)?;
                let hierarchy = TypeHierarchy::from_graphs(rows.iter().map(|row| &row.graph));
                ViaMatcher::new(&hierarchy, pattern, &regex).map(|via| (regex, via))
            }
            _ => None,
        };
        let chain = FilterChain::new(pattern, categories, via.as_ref().map(|(_, v)| v.clone()))
            .map_err(invalid)?;
        Ok(rows
            .into_iter()
            .filter_map(|mut row| {
                row.graph = chain.apply(&row.graph)?;
                row.methods = None;
                if let Some((regex, matcher)) = &via {
                    row.via = row
                        .graph
                        .calls
                        .iter()
                        .filter_map(|call| {
                            let target = call.resolved.as_deref().unwrap_or(&call.target);
                            if regex.is_match(target) {
                                return None;
                            }
                            Some(ViaCall {
                                line: call.line,
                                target: target.to_owned(),
                                via: matcher.via(target)?.to_owned(),
                            })
                        })
                        .collect();
                }
                Some(row)
            })
            .collect())
    }

    /// A szöveges kimenet útvonalai a projekt-gyökérhez relatívan: ha az abszolút gyökér legalább kétszer
    /// szerepelne, egyszer, az első sorban (`root: /abs/projekt`) áll, utána minden út relatív. Egyetlen út
    /// változatlan marad (az önmagában is egyszer írja ki a gyökeret). A gépi formátumokra (json, mermaid…) nem kell.
    pub fn relativize(&self, text: &str) -> String {
        let root = self.root.to_string_lossy();
        let root = root.trim_end_matches(std::path::MAIN_SEPARATOR);
        if root.is_empty() {
            return text.to_owned();
        }
        let prefix = format!("{root}{}", std::path::MAIN_SEPARATOR);
        if text.matches(&prefix).count() < 2 {
            return text.to_owned();
        }
        let mut out = tf!("root: {root}\n", "gyökér: {root}\n");
        out.push_str(&text.replace(&prefix, ""));
        out
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
