//! `ProjectGraph`: a projekt fájljainak gráfjai egy helyen, a `.callgraph/index.jsonl`-ben tárolva, fájlonként
//! (mtime + méret) frissítve, és a belőlük épített, projekt-szintű fordított hívó-index.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use rayon::prelude::*;

use super::record::{FileFacts, FileStamp, Header, RecordBody, RecordHead, RecordOut};
use crate::analysis::Analyzer;
use crate::error::Result;
use crate::index::CallerIndex;
use crate::io::{CacheStore, JsonlCacheStore};
use crate::model::FileGraph;
use crate::query::snippet;

/// Egy fájl állapota: a bélyeg, a tárolt sor (betöltéskor; mentéskor változatlanul visszaírható) és az értelmezett
/// tények (csak ha egy lekérdezés kérte, vagy friss elemzésből jön).
#[derive(Debug)]
struct Entry {
    stamp: FileStamp,
    raw: Option<String>,
    facts: Option<Box<FileFacts>>,
}

/// A projekt-gráf: fájlonkénti gráfok (+ hívás-sorok), a teljes projekt bejárási sorrendje és a hívó-index.
pub(crate) struct ProjectGraph {
    file: PathBuf,
    entries: HashMap<String, Entry>,
    /// Az utolsó teljes frissítés fájl-sorrendje (a régi bejárás sorrendje: a hívó-index holtversenyei ezt követik).
    order: Vec<String>,
    callers: CallerIndex,
    callers_fresh: bool,
    dirty: bool,
}

/// Egy elemzendő fájl: az út, a frissítés előtti bélyeg, és hogy volt-e már (elavult) rekordja.
type Pending<'a> = (&'a Path, FileStamp, bool);

impl ProjectGraph {
    /// Betöltés a tárolt fájlból. Hiányzó, sérült, régi formátumú vagy más build által írt fájl: üres gráf (a
    /// következő frissítés csendben újraépíti). A rekordoknak itt csak a fejét értelmezzük.
    pub(crate) fn load(file: PathBuf) -> Self {
        let mut graph = Self {
            file,
            entries: HashMap::new(),
            order: Vec::new(),
            callers: CallerIndex::build(std::iter::empty()),
            callers_fresh: false,
            dirty: true,
        };
        let Ok(text) = fs::read_to_string(&graph.file) else {
            return graph;
        };
        let mut lines = text.lines().filter(|line| !line.trim().is_empty());
        let current = lines
            .next()
            .and_then(|line| serde_json::from_str::<Header>(line).ok())
            .is_some_and(|header| header == Header::current());
        if !current {
            return graph;
        }
        graph.dirty = false;
        for line in lines {
            match serde_json::from_str::<RecordHead>(line) {
                Ok(head) => {
                    graph.entries.insert(
                        head.path,
                        Entry {
                            stamp: FileStamp {
                                mtime: head.mtime,
                                size: head.size,
                            },
                            raw: Some(line.to_owned()),
                            facts: None,
                        },
                    );
                }
                Err(_) => graph.dirty = true,
            }
        }
        graph
    }

    /// A megadott fájlok naprakészre hozása: a változatlan bélyegűek tárolt gráfja (forrás-elemzés nélkül), a
    /// változottak / újak elemzése (új fájlnál előbb a fájlonkénti cache). `full`: a `files` a teljes projekt a
    /// bejárás sorrendjében — a benne nem szereplő (törölt, átnevezett) fájlok kikerülnek, és ez lesz a hívó-index
    /// sorrendje. Utána a `files` minden gráfja elérhető (`graph`).
    pub(crate) fn sync(
        &mut self,
        files: &[PathBuf],
        full: bool,
        analyzer: &Analyzer,
        cache: &mut Option<JsonlCacheStore>,
    ) -> Result<()> {
        let mut pending: Vec<Pending<'_>> = Vec::new();
        let mut parse: Vec<(usize, String)> = Vec::new();
        for (at, file) in files.iter().enumerate() {
            let key = file.to_string_lossy().into_owned();
            let stamp = FileStamp::of(file);
            match self.entries.get(&key) {
                Some(entry) if entry.stamp == stamp => {
                    if entry.facts.is_none() {
                        parse.push((at, key));
                    }
                }
                Some(_) => pending.push((file.as_path(), stamp, true)),
                None => pending.push((file.as_path(), stamp, false)),
            }
        }
        let parsed: Vec<(usize, String, Option<FileFacts>)> = {
            let entries = &self.entries;
            parse
                .into_par_iter()
                .map(|(at, key)| {
                    let facts = entries
                        .get(&key)
                        .and_then(|entry| entry.raw.as_deref())
                        .and_then(|raw| serde_json::from_str::<RecordBody>(raw).ok())
                        .map(FileFacts::from);
                    (at, key, facts)
                })
                .collect()
        };
        for (at, key, facts) in parsed {
            if let Some(facts) = facts
                && let Some(entry) = self.entries.get_mut(&key)
            {
                entry.facts = Some(Box::new(facts));
                continue;
            }
            // sérült rekord: újraelemzés
            self.entries.remove(&key);
            let file = files[at].as_path();
            pending.push((file, FileStamp::of(file), true));
        }
        for (file, stamp, changed) in pending {
            let graph = analyze(file, changed, analyzer, cache)?;
            let snippets = snippet::call_snippets(file, graph.calls.iter().map(|call| call.line));
            self.entries.insert(
                file.to_string_lossy().into_owned(),
                Entry {
                    stamp,
                    raw: None,
                    facts: Some(Box::new(FileFacts { graph, snippets })),
                },
            );
            self.dirty = true;
            self.callers_fresh = false;
        }
        if full {
            let order: Vec<String> = files
                .iter()
                .map(|file| file.to_string_lossy().into_owned())
                .collect();
            let before = self.entries.len();
            {
                let keep: HashSet<&str> = order.iter().map(String::as_str).collect();
                self.entries.retain(|path, _| keep.contains(path.as_str()));
            }
            if self.entries.len() != before {
                self.dirty = true;
                self.callers_fresh = false;
            }
            if order != self.order {
                self.order = order;
                self.callers_fresh = false;
            }
        }
        Ok(())
    }

    /// A fájl gráfja (a `sync` után; az út a bejárás / lekérdezés alakjában).
    pub(crate) fn graph(&self, path: &str) -> Option<&FileGraph> {
        Some(&self.entries.get(path)?.facts.as_ref()?.graph)
    }

    /// A fájl hívás-sorai tömörítve (`(sor, kód)`), a `SourceLines::remember`-hez.
    pub(crate) fn snippets(&self, path: &str) -> Option<&[(u32, String)]> {
        self.entries
            .get(path)?
            .facts
            .as_ref()
            .map(|facts| facts.snippets.as_slice())
    }

    /// Szimbólum-keresés: egy metódus / függvény sortartománya (az első egyező definíció).
    pub(crate) fn method_range(
        &self,
        file: &str,
        container: Option<&str>,
        name: &str,
    ) -> Option<(u32, u32)> {
        self.graph(file)?
            .methods
            .iter()
            .find(|method| method.container.as_deref() == container && method.name == name)
            .map(|method| (method.line, method.end_line.unwrap_or(method.line)))
    }

    /// A hívó-index újraépítése, ha egy fájl vagy a sorrend változott az utolsó építés óta (különben a régi marad —
    /// a folyamatban élő gráf, pl. az MCP-szerveré, így kérésenként csak a változást fizeti).
    pub(crate) fn ensure_callers(&mut self) {
        if self.callers_fresh {
            return;
        }
        let built = CallerIndex::build(self.order.iter().filter_map(|path| {
            Some((
                path.as_str(),
                &self.entries.get(path)?.facts.as_ref()?.graph,
            ))
        }));
        self.callers = built;
        self.callers_fresh = true;
    }

    /// A teljes projekt fordított hívó-indexe (az `ensure_callers` után naprakész).
    pub(crate) fn caller_index(&self) -> &CallerIndex {
        &self.callers
    }

    /// Kiírás, ha változott valami: ideiglenes fájlba, majd átnevezés (egy félbeszakadt írás nem rontja el).
    pub(crate) fn save(&mut self) -> Result<()> {
        if !self.dirty {
            return Ok(());
        }
        let mut keys: Vec<String> = self.entries.keys().cloned().collect();
        keys.sort();
        let mut text = serde_json::to_string(&Header::current())?;
        text.push('\n');
        for key in keys {
            let Some(entry) = self.entries.get_mut(&key) else {
                continue;
            };
            if entry.raw.is_none()
                && let Some(facts) = entry.facts.as_ref()
            {
                entry.raw = Some(serde_json::to_string(&RecordOut {
                    path: &key,
                    mtime: entry.stamp.mtime,
                    size: entry.stamp.size,
                    snippets: &facts.snippets,
                    graph: &facts.graph,
                })?);
            }
            if let Some(raw) = entry.raw.as_deref() {
                text.push_str(raw);
                text.push('\n');
            }
        }
        let mut tmp = self.file.as_os_str().to_owned();
        tmp.push(format!(".tmp{}", std::process::id()));
        let tmp = PathBuf::from(tmp);
        fs::write(&tmp, text)?;
        fs::rename(&tmp, &self.file)?;
        self.dirty = false;
        Ok(())
    }
}

/// Egy fájl gráfja: új fájlnál a fájlonkénti cache-ből, ha friss (másodperces mtime), különben elemzés; a már
/// ismert, de változott fájl mindig újraelemződik (a másodperces cache egy gyors szerkesztést nem venne észre).
fn analyze(
    file: &Path,
    changed: bool,
    analyzer: &Analyzer,
    cache: &mut Option<JsonlCacheStore>,
) -> Result<FileGraph> {
    let seconds = mtime_seconds(file);
    let language = analyzer
        .registry()
        .for_path(file)
        .map(|language| language.id())
        .unwrap_or("unknown");
    if !changed
        && let (Some(store), Some(mtime)) = (cache.as_ref(), seconds)
        && let Some(graph) = store.get_for_language(file, mtime, language)
    {
        return Ok(graph);
    }
    let graph = analyzer.analyze_path(file)?.graph;
    if let (Some(store), Some(mtime)) = (cache.as_mut(), seconds) {
        let _ = store.put_for_language(file, mtime, &graph, language);
    }
    Ok(graph)
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
