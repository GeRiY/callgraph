//! A query facade: projektútvonalakból elemzett vagy cache-elt gráfokat ad.

mod grep_text;
mod issues;
mod json_keys;
mod layer;
mod loader;

use super::filter::{FilterChain, pattern_regex, receiver_target};
use super::hint::{self, Catalog};
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
    /// A fájl jegyzetei (PILOT, `crate::notes`): csak az outline / `--body` előtt töltődik; üres = nincs kiírás.
    pub notes: Vec<crate::notes::Note>,
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
            notes: Vec::new(),
        }
    }

    /// Ha a hívás rokon típuson át illeszkedett a mintára: a típus rövid neve (`via Resolver`).
    pub fn via_of(&self, call: &Call) -> Option<&str> {
        // ugyanaz a cél, mint a `via` gyűjtésénél (a `$this->m()` a hívó típusán)
        let target = receiver_target(&self.graph, call)
            .unwrap_or_else(|| call.resolved.as_deref().unwrap_or(&call.target).to_owned());
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
    /// A gráfok a perzisztens projekt-gráfból jönnek (`use_project_graph`), nem fájlonként a cache-ből.
    graph: bool,
    /// A sorok a szűkített tényekből (`facts.bin`) jönnek a teljes rekordok helyett (`use_slim_rows`).
    slim: bool,
    /// A réteg-kapcsolók (`with_layers`: `--layer`, `--layers`, `--by-layer`); `None` = nincs (alapértelmezés).
    layers: Option<crate::profile::LayerScope>,
    /// A lekérdezés során beolvasott, szintaktikai hibás fájlok (gyökérhez relatív út, az első hiba sora), a
    /// beolvasás sorrendjében — a szűrés (`--grep`) előtt gyűjtve, a kimenet záró `⚠` sorához (`parse_issues`).
    parse_issues: std::sync::Mutex<Vec<(String, u32)>>,
    /// A `--grep` nulla találatánál: a lekérdezett hatókör nevei (definíciók, hívás-célok) a „erre gondoltál”
    /// javaslathoz; találatnál üres marad (`query_filtered` tölti, `grep_text` veszi ki).
    scope_catalog: std::sync::Mutex<Option<Catalog>>,
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
            graph: false,
            slim: false,
            layers: None,
            parse_issues: std::sync::Mutex::new(Vec::new()),
            scope_catalog: std::sync::Mutex::new(None),
        }
    }
    /// A sorok a projekt-gráf szűkített tényeiből jöjjenek (`facts.bin`: egyetlen fájl a teljes hatókörre) — a
    /// `--grep` / `--depth` / `--strings` / `--const` csak ezek mezőit olvassa (`FileFacts::slim`); az egyfájlos
    /// nézetek (outline, `--body` …) a teljes rekordot kapják.
    pub fn use_slim_rows(mut self, on: bool) -> Self {
        self.slim = on;
        self
    }
    /// A lekérdezés a projekt-gráfból olvasson: a változatlan fájlok gráfja a tárolóból jön (forrás-elemzés
    /// nélkül), a hívó-index a teljes projektre kész. `CALLGRAPH_NO_GRAPH=1` kikapcsolja (gráf nélküli út: minden
    /// fájl elemezve; a kimenet azonos — a tesztek ezzel vetik össze a kettőt).
    pub fn use_project_graph(mut self, on: bool) -> Self {
        self.graph = on && std::env::var_os("CALLGRAPH_NO_GRAPH").is_none();
        self
    }
    pub fn label(&self) -> &str {
        &self.label
    }
    /// A (kanonikus) projekt-gyökér.
    pub fn root(&self) -> &Path {
        &self.root
    }
    /// A `--depth` 2. szinttől kezdve keresett hatókör: a teljes projektgyökér (`--project`, különben a munkamappa).
    /// A hívók hívói bárhol lehetnek a projektben, nem csak a lekérdezett mappában (pl. `app/Services` → `routes/`).
    pub fn depth_scope(&self) -> Vec<PathBuf> {
        vec![self.root.clone()]
    }
    pub fn query(&self, selectors: &SelectorSet) -> Result<Vec<QueryResult>> {
        let files = self.files(selectors)?;
        let mut out = Vec::new();
        // a projekt-gráfból: a kiválasztott fájlok bélyege ellenőrizve (csak a változottak elemződnek), utána a
        // szűkített tényeik (`facts.bin`) vagy a teljes rekordjaik (`graph.pack`), egyetlen fájl-olvasással
        let shared = self.project_graph();
        let mut project = shared.as_ref().map(crate::graph::lock);
        if let Some(graph) = project.as_deref_mut() {
            let paths: Vec<PathBuf> = files.iter().map(|(file, _)| self.root.join(file)).collect();
            // hiányzó fájlnál a régi úton jön a „nem található” hiba
            if paths.iter().all(|path| path.is_file()) {
                graph.refresh(&paths, false, &self.analyzer)?;
                if self.slim {
                    graph.load_facts(&paths, &self.analyzer)?;
                } else {
                    graph.load_full(&paths, &self.analyzer)?;
                }
                let _ = graph.save();
            }
        }
        for (file, methods) in files {
            let abs = self.root.join(&file);
            if !abs.is_file() {
                return Err(self.not_found(&abs));
            }
            let stored = project
                .as_deref()
                .and_then(|graph| {
                    let key = abs.to_string_lossy();
                    if self.slim {
                        graph.slim_graph(&key)
                    } else {
                        graph.graph(&key)
                    }
                })
                .cloned();
            let mut graph = match stored {
                Some(graph) => graph,
                None => self.analyze(&abs)?,
            };
            // JSON: a kért kulcs-út (`package.json::scripts`) szakasz lesz, a forrás-soraival
            if let Some(names) = &methods {
                json_keys::add_key_sections(&abs, &mut graph, names);
            }
            self.note_parse_issue(&abs, &graph);
            if graph.binary {
                if selectors
                    .items
                    .iter()
                    .any(|input| self.root.join(&input.file) == abs)
                {
                    return Err(crate::lang::filetype::skipped_content_error(&abs));
                }
                continue;
            }
            let methods = methods.map(|names| {
                // névegyezés: egy név több metódusra is illeszkedhet (Rust: több impl, PHP/Swift: túlterhelés),
                // ezért a hiányt név szerint nézzük, nem darabszámra
                // pontos név; ha nincs ilyen, a pont nélküli név az objektum-literál metódusára is illeszkedik
                // (`store.ts::updateSongs` → `playableStore.updateSongs`)
                let exact = |n: &String| graph.methods.iter().any(|m| &m.name == n);
                let matches = |n: &String, name: &str| {
                    name == n || (!exact(n) && !n.contains('.') && name.rsplit_once('.').is_some_and(|(_, last)| last == n))
                };
                let found = graph.methods.iter().filter(|m| names.iter().any(|n| matches(n, &m.name))).map(|m| m.name.clone()).collect::<Vec<_>>();
                if let Some(missing) = names.iter().find(|n| !found.iter().any(|f| matches(n, f))) {
                    let message = tf!(
                        "No such method: {}::{} (see --format outline for the file's full list)",
                        "Nincs ilyen metódus: {}::{} (lásd: --format outline a fájl teljes listájához)",
                        abs.display(),
                        missing
                    );
                    // hibás fájlnál a hiány a parser feladásából is jöhet: ne tűnjön biztos „nincs ilyen”-nek
                    let hint = graph.parse_errors.first().map(|error| tf!(
                        " — note: the file has a syntax error at L{} and may be only partly mapped; read the file directly",
                        " — megjegyzés: a fájlban szintaktikai hiba van itt: L{}, a térkép hiányos lehet; olvasd közvetlenül a fájlt",
                        error.line
                    )).unwrap_or_default();
                    // a fájl valódi metódusnevei közül a hasonlók (elgépelés, kis-/nagybetű, előtag)
                    let mut seen = std::collections::BTreeSet::new();
                    let real: Vec<&str> = graph
                        .methods
                        .iter()
                        .map(|m| m.name.as_str())
                        .filter(|name| seen.insert(*name))
                        .collect();
                    // a `Típus.metódus` (objektum-literál) nevek pontos része is számít: `(utolsó tag, teljes név)`
                    let pairs = real.iter().flat_map(|name| {
                        let last = name.rsplit_once('.').map_or(*name, |(_, last)| last);
                        [(*name, *name), (last, *name)]
                    });
                    let found = hint::closest(missing, pairs, hint::MAX_SUGGESTIONS);
                    let suggestion = match hint::did_you_mean(crate::i18n::lang(), &found) {
                        line if line.is_empty() => line,
                        line => format!("\n{line}"),
                    };
                    return Err(CallgraphError::Usage(format!("{message}{hint}{suggestion}")));
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
        let mut kept_rows = Vec::new();
        for row in &rows {
            let Some(graph) = chain.apply(&row.graph) else {
                continue;
            };
            let mut kept = QueryResult::new(graph, None);
            kept.notes.clone_from(&row.notes);
            if let Some((regex, matcher)) = &via {
                kept.via = kept
                    .graph
                    .calls
                    .iter()
                    .filter_map(|call| {
                        // a saját példányon át hívott (`$this->m()`) a hívó típusán: `Ns\Osztály::m`
                        let target = receiver_target(&kept.graph, call).unwrap_or_else(|| {
                            call.resolved.as_deref().unwrap_or(&call.target).to_owned()
                        });
                        if regex.is_match(&target) {
                            return None;
                        }
                        let via = matcher.via(&target)?.to_owned();
                        Some(ViaCall {
                            line: call.line,
                            target,
                            via,
                        })
                    })
                    .collect();
            }
            kept_rows.push(kept);
        }
        // nulla találat: a hatókör nevei a „erre gondoltál” javaslathoz (a szűrés előtti gráfokból)
        if kept_rows.is_empty() && pattern.is_some() {
            let catalog = Catalog::from_graphs(rows.iter().map(|row| &row.graph));
            *self.scope_catalog.lock().unwrap_or_else(|e| e.into_inner()) = Some(catalog);
        }
        Ok(kept_rows)
    }

    /// A `query_filtered` által nulla találatnál eltett névlista (egyszer vehető ki).
    pub(super) fn take_scope_catalog(&self) -> Catalog {
        self.scope_catalog
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
            .unwrap_or_default()
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
        // Windowson a canonicalize `\\?\C:\…` alakot ad: a kiírt útból ezt elhagyjuk (az illesztés a teljes alakkal megy)
        let shown = strip_verbatim(root);
        if text.matches(&prefix).count() < 2 {
            return if shown == root {
                text.to_owned()
            } else {
                text.replace(&prefix, &format!("{shown}{}", std::path::MAIN_SEPARATOR))
            };
        }
        let mut out = tf!("root: {shown}\n", "gyökér: {shown}\n");
        out.push_str(&text.replace(&prefix, ""));
        out
    }
    fn files(&self, selectors: &SelectorSet) -> Result<Vec<(PathBuf, Option<Vec<String>>)>> {
        let mut result: Vec<(PathBuf, Option<Vec<String>>)> = Vec::new();
        let walker = FileWalker::for_registry(self.analyzer.registry());
        for input in &selectors.items {
            let abs = self.root.join(&input.file);
            if abs.is_file() && walker.excluded_by_ignore_file(&abs) {
                return Err(CallgraphError::Excluded(abs));
            }
        }
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
                    let files = walker.collect(std::slice::from_ref(&abs), &mut warn);
                    for f in self.legacy_order(files) {
                        let rel = f.strip_prefix(&self.root).unwrap_or(&f).to_path_buf();
                        // `--layer`: csak a kért rétegek fájljai (szűrő nélkül mind)
                        if self.keeps_layer(&rel) {
                            result.push((rel, None));
                        }
                    }
                } else {
                    return Err(self.not_found(&abs));
                }
            }
        }
        Ok(result)
    }
    /// A „nincs ilyen fájl/mappa” hiba (a `Not found: ` / `Nem található: ` előtagot a CLI hibaosztályozója figyeli);
    /// a második sor a hasonló útvonalakat javasolja (a projekt kódfájljaiból és mappáiból).
    fn not_found(&self, abs: &Path) -> CallgraphError {
        let (path, label) = (abs.display(), &self.label);
        let mut message = tf!(
            "Not found: {path} (relative to the --project {label} root)",
            "Nem található: {path} (--project {label} gyökeréhez képest)"
        );
        let line = hint::did_you_mean(crate::i18n::lang(), &self.similar_paths(abs));
        if !line.is_empty() {
            message.push('\n');
            message.push_str(&line);
        }
        CallgraphError::Usage(message)
    }

    /// A hiányzó útvonalhoz hasonló, létező útvonalak (gyökérhez relatívan, `/`-jellel): a hatókör kódfájljai és
    /// azok mappái. Csak a hibaágon fut, ezért nem gyorsítótárazott bejárás.
    fn similar_paths(&self, abs: &Path) -> Vec<String> {
        let relative = |path: &Path| {
            path.strip_prefix(&self.root)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/")
        };
        let mut known = std::collections::BTreeSet::new();
        for file in self.walk(std::slice::from_ref(&self.root)) {
            let file = relative(&file);
            let mut dir = file.as_str();
            while let Some((parent, _)) = dir.rsplit_once('/') {
                known.insert(parent.to_owned());
                dir = parent;
            }
            known.insert(file);
        }
        let known: Vec<String> = known.into_iter().collect();
        hint::similar_paths(&relative(abs), &known)
    }
}

/// `\\?\C:\x` → `C:\x` (a Windows-féle „verbatim” előtag a meghajtós utaknál; UNC- és más utak változatlanok).
pub(crate) fn strip_verbatim(path: &str) -> &str {
    match path.strip_prefix(r"\\?\") {
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => rest,
        _ => path,
    }
}

#[cfg(test)]
mod verbatim_tests {
    #[test]
    fn strips_only_the_drive_verbatim_prefix() {
        assert_eq!(super::strip_verbatim(r"\\?\C:\Users\x"), r"C:\Users\x");
        assert_eq!(
            super::strip_verbatim(r"\\?\UNC\srv\share"),
            r"\\?\UNC\srv\share"
        );
        assert_eq!(super::strip_verbatim("/home/x"), "/home/x");
    }
}
