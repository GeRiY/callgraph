//! `ProjectGraph`: a projekt fájljainak gráfjai egy hozzáfűzős packben (`.callgraph/graph.pack`), egy kis
//! manifesztben (`index.bin`: út → bélyeg + a rekord helye) és a szűkített fájlonkénti tényekben (`facts.bin`).
//! Indításkor csak a manifest töltődik; a teljes hatókörű lekérdezések (`--grep`, `--depth`, `callers`,
//! `--strings`, `--const`) a `facts.bin`-ből válaszolnak, a pack csak az egyfájlos nézetekhez (outline, `--body` …)
//! és a változott fájlokhoz kell. Egy meleg lekérdezés így a projekt méretétől függetlenül állandó számú fájlt nyit.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use rayon::prelude::*;

use super::codec::{self, ManifestEntry};
use super::record::{FileFacts, FileStamp};
use crate::analysis::Analyzer;
use crate::error::Result;
use crate::index::CallerIndex;
use crate::model::FileGraph;
use crate::query::snippet;

/// A manifest fájlneve a `.callgraph/` mappában.
const MANIFEST_FILE: &str = "index.bin";
/// A teljes rekordok packje.
const PACK_FILE: &str = "graph.pack";
/// A szűkített fájlonkénti tények.
const FACTS_FILE: &str = "facts.bin";
/// A korábbi tárolók (JSONL gráf, JSON-cache, shardos változat): az új tároló első építésekor törlődnek.
const LEGACY_FILES: [&str; 4] = [
    "index.jsonl",
    "cache.jsonl",
    "cache.jsonl.lock",
    "callers.bin",
];
const LEGACY_DIR: &str = "graph";
/// A pack ennél kisebb méretnél nem tömörül (a holt terület ott nem számít); a tesztekben kicsi, hogy a tömörítés
/// kis fájlokkal is kipróbálható legyen.
const COMPACT_MIN: u64 = if cfg!(test) { 4096 } else { 1 << 20 };

/// Egy ismert fájl: a bélyeg és a teljes rekordja helye a packben (`len == 0`: nincs a packben).
#[derive(Debug, Clone, Copy)]
struct Tracked {
    stamp: FileStamp,
    offset: u64,
    len: u32,
}

/// Egy fájl szűkített tényei (`FileFacts::slim`) és a tárolt kerete (újraírás kódolás nélkül).
struct FactEntry {
    stamp: FileStamp,
    facts: FileFacts,
    frame: Option<Vec<u8>>,
}

/// Egy elemzendő fájl: az út és a frissítés előtti bélyeg.
type Work<'a> = (&'a Path, FileStamp);

/// Egy elemzés eredménye: kulcs, bélyeg, tények és a pack-keret.
type Analyzed = (String, FileStamp, FileFacts, Vec<u8>);

/// Egy `ProjectGraph::update` eredménye: hány fájl elemződött újra, hány került ki (törölt), hány volt naprakész.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Refreshed {
    pub updated: usize,
    pub removed: usize,
    pub unchanged: usize,
}

/// Mérőszámok a tesztekhez: hány tároló-fájlt nyitott meg és hány teljes rekordot olvasott a példány.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct Stats {
    pub files_opened: usize,
    pub records_read: usize,
    pub analyzed: usize,
}

/// A projekt-gráf (a folyamatban gyökerenként egy példány: `graph::shared`).
pub(crate) struct ProjectGraph {
    dir: PathBuf,
    tracked: HashMap<String, Tracked>,
    /// Az utolsó teljes frissítés fájl-sorrendje (a régi bejárás sorrendje: a hívó-index holtversenyei ezt követik).
    order: Vec<String>,
    /// A memóriában lévő teljes rekordok (az egyfájlos nézetekhez; az MCP-szerverben kérésről kérésre megmaradnak).
    loaded: HashMap<String, FileFacts>,
    /// A `facts.bin` tartalma, ha már kellett.
    facts: Option<HashMap<String, FactEntry>>,
    callers: CallerIndex,
    callers_fresh: bool,
    manifest_dirty: bool,
    facts_dirty: bool,
    stats: Stats,
}

impl ProjectGraph {
    /// Megnyitás: csak a manifest olvasódik be. Hiányzó, sérült, régi formátumú vagy más build által írt manifest:
    /// üres gráf, a régi tárolók (pack, `facts.bin`, a korábbi JSONL gráf, JSON-cache és shardok) törlődnek, és a
    /// következő frissítés csendben újraépít.
    pub(crate) fn open(dir: PathBuf) -> Self {
        let mut graph = Self {
            dir,
            tracked: HashMap::new(),
            order: Vec::new(),
            loaded: HashMap::new(),
            facts: None,
            callers: CallerIndex::build(std::iter::empty()),
            callers_fresh: false,
            manifest_dirty: false,
            facts_dirty: false,
            stats: Stats::default(),
        };
        graph.stats.files_opened += 1;
        match codec::read_manifest(&graph.dir.join(MANIFEST_FILE)) {
            Some(entries) => {
                for entry in entries {
                    graph.tracked.insert(
                        entry.path,
                        Tracked {
                            stamp: FileStamp {
                                mtime: entry.mtime,
                                size: entry.size,
                            },
                            offset: entry.offset,
                            len: entry.len,
                        },
                    );
                }
            }
            None => {
                graph.manifest_dirty = true;
                graph.reset_storage();
            }
        }
        graph
    }

    /// Az érvénytelen tároló eltakarítása (a `notes.jsonl` és a `.gitignore` marad).
    fn reset_storage(&self) {
        let _ = fs::remove_file(self.dir.join(PACK_FILE));
        let _ = fs::remove_file(self.dir.join(FACTS_FILE));
        let _ = fs::remove_dir_all(self.dir.join(LEGACY_DIR));
        for name in LEGACY_FILES {
            let _ = fs::remove_file(self.dir.join(name));
        }
    }

    /// A példány mérőszámai (a tesztek ezzel ellenőrzik, hogy a meleg lekérdezés állandó számú fájlt nyit).
    #[cfg(test)]
    pub(crate) fn stats(&self) -> Stats {
        self.stats
    }

    /// A megadott fájlok bélyegének ellenőrzése (stat, tartalom-olvasás nélkül); a változott / új fájlok elemzése
    /// (párhuzamosan), a rekordjaik egyetlen hozzáfűzéssel a packbe. `full`: a `files` a teljes projekt a bejárás
    /// sorrendjében — a benne nem szereplő (törölt, átnevezett) fájlok kikerülnek, és ez lesz a hívó-index sorrendje.
    pub(crate) fn refresh(
        &mut self,
        files: &[PathBuf],
        full: bool,
        analyzer: &Analyzer,
    ) -> Result<()> {
        let stamps: Vec<FileStamp> = files.par_iter().map(|file| FileStamp::of(file)).collect();
        let work: Vec<Work<'_>> = files
            .iter()
            .zip(stamps)
            .filter(|(file, stamp)| {
                self.tracked
                    .get(file.to_string_lossy().as_ref())
                    .is_none_or(|tracked| tracked.stamp != *stamp)
            })
            .map(|(file, stamp)| (file.as_path(), stamp))
            .collect();
        self.analyze(&work, analyzer)?;
        if full {
            let order: Vec<String> = files
                .iter()
                .map(|file| file.to_string_lossy().into_owned())
                .collect();
            self.prune(&order);
            if order != self.order {
                self.order = order;
                self.callers_fresh = false;
            }
        }
        Ok(())
    }

    /// Frissítés a `callgraph refresh` / az MCP-figyelő számára: a `files` bélyegének ellenőrzése, a változott /
    /// új fájlok elemzése (mint a `refresh`), a `gone` kulcsok (törölt fájlok) eltávolítása, és a `files` szűkített
    /// tényei naprakészen (`facts.bin`) — így a következő lekérdezés semmit sem elemez. `full`: mint a `refresh`.
    pub(crate) fn update(
        &mut self,
        files: &[PathBuf],
        gone: &[String],
        full: bool,
        analyzer: &Analyzer,
    ) -> Result<Refreshed> {
        let pruned = if full {
            let keep: HashSet<String> = files
                .iter()
                .map(|file| file.to_string_lossy().into_owned())
                .collect();
            self.tracked
                .keys()
                .filter(|key| !keep.contains(*key))
                .count()
        } else {
            0
        };
        let analyzed = self.stats.analyzed;
        self.refresh(files, full, analyzer)?;
        let updated = self.stats.analyzed - analyzed;
        self.load_facts(files, analyzer)?;
        let removed = pruned + self.forget(gone);
        Ok(Refreshed {
            updated,
            removed,
            unchanged: files.len().saturating_sub(updated),
        })
    }

    /// A megadott kulcsú (törölt) fájlok eltávolítása a gráfból; a ténylegesen ismert fájlok száma.
    fn forget(&mut self, keys: &[String]) -> usize {
        if keys.is_empty() {
            return 0;
        }
        let drop: HashSet<&str> = keys.iter().map(String::as_str).collect();
        let before = self.tracked.len();
        self.tracked.retain(|path, _| !drop.contains(path.as_str()));
        let removed = before - self.tracked.len();
        self.loaded.retain(|path, _| !drop.contains(path.as_str()));
        if let Some(facts) = self.facts.as_mut() {
            let before = facts.len();
            facts.retain(|path, _| !drop.contains(path.as_str()));
            self.facts_dirty |= facts.len() != before;
        }
        let order = self.order.len();
        self.order.retain(|path| !drop.contains(path.as_str()));
        if removed > 0 || order != self.order.len() {
            self.manifest_dirty = true;
            self.callers_fresh = false;
        }
        removed
    }

    /// Az ismert fájlok kulcsai, amelyek a megadott út (fájl vagy mappa) alatt vannak — a `refresh` ezekből tudja,
    /// melyik fájl tűnt el.
    pub(crate) fn tracked_under(&self, path: &Path) -> Vec<String> {
        let mut keys: Vec<String> = self
            .tracked
            .keys()
            .filter(|key| Path::new(key.as_str()).starts_with(path))
            .cloned()
            .collect();
        keys.sort();
        keys
    }

    /// A teljes projekt-listában nem szereplő fájlok eltávolítása (a pack-rekordjuk holt terület lesz).
    fn prune(&mut self, order: &[String]) {
        let keep: HashSet<&str> = order.iter().map(String::as_str).collect();
        let before = self.tracked.len();
        self.tracked.retain(|path, _| keep.contains(path.as_str()));
        if self.tracked.len() != before {
            self.manifest_dirty = true;
            self.callers_fresh = false;
        }
        self.loaded.retain(|path, _| keep.contains(path.as_str()));
        if let Some(facts) = self.facts.as_mut() {
            let before = facts.len();
            facts.retain(|path, _| keep.contains(path.as_str()));
            if facts.len() != before {
                self.facts_dirty = true;
                self.callers_fresh = false;
            }
        }
    }

    /// Elemzés párhuzamosan; a rekordok egyetlen írással a packbe (üres manifestnél új pack). Hibánál a sorrendben
    /// első hibás fájl hibája (mint a gráf nélküli úton).
    fn analyze(&mut self, work: &[Work<'_>], analyzer: &Analyzer) -> Result<()> {
        if work.is_empty() {
            return Ok(());
        }
        let results: Vec<Result<Analyzed>> = work
            .par_iter()
            .map(|&(path, stamp)| {
                let graph = analyzer.analyze_path(path)?.graph;
                let snippets =
                    snippet::call_snippets(path, graph.calls.iter().map(|call| call.line));
                let key = path.to_string_lossy().into_owned();
                let facts = FileFacts { graph, snippets };
                let frame = codec::encode_record(&key, stamp, &facts)?;
                Ok((key, stamp, facts, frame))
            })
            .collect();
        let done = results.into_iter().collect::<Result<Vec<Analyzed>>>()?;
        self.stats.analyzed += done.len();
        let places = {
            let frames: Vec<&[u8]> = done
                .iter()
                .map(|(_, _, _, frame)| frame.as_slice())
                .collect();
            let pack = self.dir.join(PACK_FILE);
            // a kiírás hibája nem állítja meg a lekérdezést (a fájl legközelebb újraelemződik)
            if self.tracked.is_empty() {
                codec::write_pack(&pack, &frames).ok()
            } else {
                codec::append_pack(&pack, &frames).ok()
            }
        };
        for (at, (key, stamp, facts, _)) in done.into_iter().enumerate() {
            match places.as_ref().and_then(|places| places.get(at)) {
                Some(&(offset, len)) => {
                    self.tracked
                        .insert(key.clone(), Tracked { stamp, offset, len });
                }
                None => {
                    self.tracked.remove(&key);
                }
            }
            if let Some(entries) = self.facts.as_mut() {
                entries.insert(
                    key.clone(),
                    FactEntry {
                        stamp,
                        facts: facts.slim(),
                        frame: None,
                    },
                );
                self.facts_dirty = true;
            }
            self.manifest_dirty = true;
            self.callers_fresh = false;
            self.loaded.insert(key, facts);
        }
        Ok(())
    }

    /// A megadott fájlok teljes rekordjai a memóriába: a packből egyetlen megnyitással, párhuzamos dekódolással; a
    /// sérült / hiányzó rekord fájlja újraelemződik. A `refresh` után hívandó.
    pub(crate) fn load_full(&mut self, files: &[PathBuf], analyzer: &Analyzer) -> Result<()> {
        let want: Vec<(&Path, String)> = files
            .iter()
            .map(|file| (file.as_path(), file.to_string_lossy().into_owned()))
            .filter(|(_, key)| !self.loaded.contains_key(key))
            .collect();
        if want.is_empty() {
            return Ok(());
        }
        let places: Vec<Option<(u64, u32)>> = want
            .iter()
            .map(|(_, key)| {
                self.tracked
                    .get(key)
                    .filter(|tracked| tracked.len > 0)
                    .map(|tracked| (tracked.offset, tracked.len))
            })
            .collect();
        let ask: Vec<(u64, u32)> = places.iter().flatten().copied().collect();
        let fetched = if ask.is_empty() {
            Vec::new()
        } else {
            self.stats.files_opened += 1;
            self.stats.records_read += ask.len();
            codec::read_pack(&self.dir.join(PACK_FILE), &ask)
        };
        let mut read = fetched.into_iter();
        let frames: Vec<Option<Vec<u8>>> = places
            .iter()
            .map(|place| place.and_then(|_| read.next().flatten()))
            .collect();
        let decoded: Vec<Option<FileFacts>> = {
            let tracked = &self.tracked;
            frames
                .par_iter()
                .zip(want.par_iter())
                .map(|(frame, (_, key))| {
                    let (path, stamp, facts) = codec::decode_record(frame.as_deref()?)?;
                    let expected = tracked.get(key)?.stamp;
                    (path == *key && stamp == expected).then_some(facts)
                })
                .collect()
        };
        let mut broken: Vec<Work<'_>> = Vec::new();
        for ((path, key), facts) in want.into_iter().zip(decoded) {
            match facts {
                Some(facts) => {
                    self.loaded.insert(key, facts);
                }
                None => broken.push((path, FileStamp::of(path))),
            }
        }
        self.analyze(&broken, analyzer)
    }

    /// A megadott fájlok szűkített tényei naprakészen: a `facts.bin`-ből (egyetlen olvasás, párhuzamos dekódolás);
    /// a hiányzó / elavult fájloké a teljes rekordjukból (csak ezekhez kell a pack). A `refresh` után hívandó.
    pub(crate) fn load_facts(&mut self, files: &[PathBuf], analyzer: &Analyzer) -> Result<()> {
        if self.facts.is_none() {
            self.facts = Some(self.read_facts());
        }
        let stale: Vec<PathBuf> = {
            let entries = self.facts.as_ref();
            files
                .iter()
                .filter(|file| {
                    let key = file.to_string_lossy();
                    let stored = entries.and_then(|entries| entries.get(key.as_ref()));
                    let current = self.tracked.get(key.as_ref()).map(|tracked| tracked.stamp);
                    stored.is_none_or(|entry| Some(entry.stamp) != current)
                })
                .cloned()
                .collect()
        };
        if stale.is_empty() {
            return Ok(());
        }
        let borrowed: HashSet<String> = stale
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .filter(|key| !self.loaded.contains_key(key))
            .collect();
        self.load_full(&stale, analyzer)?;
        let mut entries = self.facts.take().unwrap_or_default();
        for path in &stale {
            let key = path.to_string_lossy().into_owned();
            let Some(facts) = self.loaded.get(&key) else {
                continue;
            };
            let stamp = self
                .tracked
                .get(&key)
                .map(|tracked| tracked.stamp)
                .unwrap_or_default();
            entries.insert(
                key,
                FactEntry {
                    stamp,
                    facts: facts.slim(),
                    frame: None,
                },
            );
        }
        // a csak a tényekhez betöltött teljes rekordok nem maradnak a memóriában
        self.loaded.retain(|key, _| !borrowed.contains(key));
        self.facts = Some(entries);
        self.facts_dirty = true;
        Ok(())
    }

    /// A `facts.bin` beolvasása (egyetlen olvasás, a keretek párhuzamos dekódolása); a sérült keret kimarad.
    fn read_facts(&mut self) -> HashMap<String, FactEntry> {
        self.stats.files_opened += 1;
        codec::read_facts(&self.dir.join(FACTS_FILE))
            .into_par_iter()
            .filter_map(|frame| {
                let (path, stamp, facts) = codec::decode_record(&frame)?;
                Some((
                    path,
                    FactEntry {
                        stamp,
                        facts,
                        frame: Some(frame),
                    },
                ))
            })
            .collect()
    }

    /// A fájl teljes gráfja (a `load_full` után; az út a bejárás / lekérdezés alakjában).
    pub(crate) fn graph(&self, path: &str) -> Option<&FileGraph> {
        self.loaded.get(path).map(|facts| &facts.graph)
    }

    /// A fájl szűkített gráfja (a `load_facts` után) — a teljes hatókörű lekérdezések sorai.
    pub(crate) fn slim_graph(&self, path: &str) -> Option<&FileGraph> {
        self.facts
            .as_ref()?
            .get(path)
            .map(|entry| &entry.facts.graph)
    }

    /// A fájl hívás-sorai tömörítve (`(sor, kód)`), a `SourceLines::remember`-hez.
    pub(crate) fn snippets(&self, path: &str) -> Option<&[(u32, String)]> {
        self.facts_of(path).map(|facts| facts.snippets.as_slice())
    }

    fn facts_of(&self, path: &str) -> Option<&FileFacts> {
        self.loaded
            .get(path)
            .or_else(|| self.facts.as_ref()?.get(path).map(|entry| &entry.facts))
    }

    /// Szimbólum-keresés: egy metódus / függvény sortartománya (az első egyező definíció).
    pub(crate) fn method_range(
        &self,
        file: &str,
        container: Option<&str>,
        name: &str,
    ) -> Option<(u32, u32)> {
        self.facts_of(file)?
            .graph
            .methods
            .iter()
            .find(|method| method.container.as_deref() == container && method.name == name)
            .map(|method| (method.line, method.end_line.unwrap_or(method.line)))
    }

    /// A hívó-index naprakészre hozása (a teljes `refresh` után) a `facts.bin`-ből. Ha semmi sem változott az utolsó
    /// építés óta, a régi marad — a folyamatban élő gráf (pl. az MCP-szerveré) így kérésenként csak a változást fizeti.
    pub(crate) fn ensure_callers(&mut self, analyzer: &Analyzer) -> Result<()> {
        if self.callers_fresh {
            return Ok(());
        }
        let files: Vec<PathBuf> = self.order.iter().map(PathBuf::from).collect();
        self.load_facts(&files, analyzer)?;
        if let Some(entries) = self.facts.as_mut() {
            let keep: HashSet<&str> = self.order.iter().map(String::as_str).collect();
            let before = entries.len();
            entries.retain(|path, _| keep.contains(path.as_str()));
            if entries.len() != before {
                self.facts_dirty = true;
            }
        }
        let built = {
            let entries = self.facts.as_ref();
            CallerIndex::build(self.order.iter().filter_map(|path| {
                let entry = entries?.get(path)?;
                Some((path.as_str(), &entry.facts.graph))
            }))
        };
        self.callers = built;
        self.callers_fresh = true;
        Ok(())
    }

    /// A teljes projekt fordított hívó-indexe (az `ensure_callers` után naprakész).
    pub(crate) fn caller_index(&self) -> &CallerIndex {
        &self.callers
    }

    /// Kiírás, ha változott valami: ha a packben a holt terület a felénél több, előbb tömörítés (a pack
    /// újraírása); utána a manifest és a `facts.bin` — mind atomikusan.
    pub(crate) fn save(&mut self) -> Result<()> {
        if self.manifest_dirty {
            self.compact_if_needed();
            let mut entries: Vec<ManifestEntry> = self
                .tracked
                .iter()
                .map(|(path, tracked)| ManifestEntry {
                    path: path.clone(),
                    mtime: tracked.stamp.mtime,
                    size: tracked.stamp.size,
                    offset: tracked.offset,
                    len: tracked.len,
                })
                .collect();
            entries.sort_by(|a, b| a.path.cmp(&b.path));
            codec::write_manifest(&self.dir.join(MANIFEST_FILE), &entries)?;
            self.manifest_dirty = false;
        }
        if self.facts_dirty
            && let Some(entries) = self.facts.as_mut()
        {
            entries.par_iter_mut().for_each(|(path, entry)| {
                if entry.frame.is_none() {
                    entry.frame = codec::encode_record(path, entry.stamp, &entry.facts).ok();
                }
            });
            let mut frames: Vec<(&String, &[u8])> = entries
                .iter()
                .filter_map(|(path, entry)| Some((path, entry.frame.as_deref()?)))
                .collect();
            frames.sort_by(|a, b| a.0.cmp(b.0));
            let frames: Vec<&[u8]> = frames.into_iter().map(|(_, frame)| frame).collect();
            codec::write_facts(&self.dir.join(FACTS_FILE), &frames)?;
            self.facts_dirty = false;
        }
        Ok(())
    }

    /// A pack újraírása csak az élő rekordokkal, ha a holt terület (felülírt / törölt fájlok régi rekordjai) a pack
    /// felénél több. Hibánál a régi pack marad (a holt terület nem baj).
    fn compact_if_needed(&mut self) {
        let pack = self.dir.join(PACK_FILE);
        let size = codec::pack_len(&pack);
        let live: u64 = self
            .tracked
            .values()
            .filter(|tracked| tracked.len > 0)
            .map(|tracked| u64::from(tracked.len) + 4)
            .sum();
        if size < COMPACT_MIN || live.saturating_mul(2) >= size {
            return;
        }
        let mut keys: Vec<(String, (u64, u32))> = self
            .tracked
            .iter()
            .filter(|(_, tracked)| tracked.len > 0)
            .map(|(path, tracked)| (path.clone(), (tracked.offset, tracked.len)))
            .collect();
        keys.sort_by_key(|(_, place)| place.0);
        let places: Vec<(u64, u32)> = keys.iter().map(|(_, place)| *place).collect();
        let frames = codec::read_pack(&pack, &places);
        let kept: Vec<(&String, &[u8])> = keys
            .iter()
            .zip(&frames)
            .filter_map(|((path, _), frame)| Some((path, frame.as_deref()?)))
            .collect();
        let bytes: Vec<&[u8]> = kept.iter().map(|(_, frame)| *frame).collect();
        let Ok(written) = codec::write_pack(&pack, &bytes) else {
            return;
        };
        let moved: HashMap<&str, (u64, u32)> = kept
            .iter()
            .map(|(path, _)| path.as_str())
            .zip(written)
            .collect();
        for (path, tracked) in &mut self.tracked {
            match moved.get(path.as_str()) {
                Some(&(offset, len)) => {
                    tracked.offset = offset;
                    tracked.len = len;
                }
                None => tracked.len = 0,
            }
        }
    }
}
