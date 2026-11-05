//! `ProjectGraph`: a projekt fájljainak gráfjai fájlonkénti bináris shardokban (`.callgraph/graph/<név>.bin`), egy
//! kis manifesztben (`index.bin`: út → bélyeg + shard) és a hívó-index bemenetében (`callers.bin`). Indításkor csak
//! a manifest töltődik; a shardok csak a lekérdezés hatókörére, a hívó-index csak a `--depth` / `callers` kérésre.

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
/// A hívó-index bemenete.
const CALLERS_FILE: &str = "callers.bin";
/// A shardok mappája.
const SHARDS_DIR: &str = "graph";
/// A korábbi tárolók (JSONL gráf, fájlonkénti JSON-cache): az új tároló első építésekor törlődnek.
const LEGACY_FILES: [&str; 3] = ["index.jsonl", "cache.jsonl", "cache.jsonl.lock"];

/// Egy ismert fájl: a bélyeg és a shard neve.
#[derive(Debug, Clone)]
struct Tracked {
    stamp: FileStamp,
    shard: String,
}

/// A hívó-index egy fájlra eső bemenete (a `FileFacts::slim` alak) és a tárolt kerete (újraírás kódolás nélkül).
struct CallerEntry {
    stamp: FileStamp,
    facts: FileFacts,
    frame: Option<Vec<u8>>,
}

/// Egy elemzendő fájl: az út és a frissítés előtti bélyeg.
type Work<'a> = (&'a Path, FileStamp);

/// Egy elemzés eredménye: kulcs, bélyeg, tények, és a shard neve, ha a kiírás sikerült.
type Analyzed = (String, FileStamp, FileFacts, Option<String>);

/// A projekt-gráf (a folyamatban gyökerenként egy példány: `graph::shared`).
pub(crate) struct ProjectGraph {
    dir: PathBuf,
    tracked: HashMap<String, Tracked>,
    /// Az utolsó teljes frissítés fájl-sorrendje (a régi bejárás sorrendje: a hívó-index holtversenyei ezt követik).
    order: Vec<String>,
    /// A memóriában lévő shardok (a lekérdezett fájlok; az MCP-szerverben kérésről kérésre megmaradnak).
    loaded: HashMap<String, FileFacts>,
    /// A `callers.bin` tartalma, ha már kellett.
    callers_facts: Option<HashMap<String, CallerEntry>>,
    callers: CallerIndex,
    callers_fresh: bool,
    manifest_dirty: bool,
    callers_dirty: bool,
}

impl ProjectGraph {
    /// Megnyitás: csak a manifest olvasódik be. Hiányzó, sérült, régi formátumú vagy más build által írt manifest:
    /// üres gráf, a régi tárolók (shardok, `callers.bin`, a JSONL gráf és cache) törlődnek, és a következő
    /// frissítés csendben újraépít.
    pub(crate) fn open(dir: PathBuf) -> Self {
        let mut graph = Self {
            dir,
            tracked: HashMap::new(),
            order: Vec::new(),
            loaded: HashMap::new(),
            callers_facts: None,
            callers: CallerIndex::build(std::iter::empty()),
            callers_fresh: false,
            manifest_dirty: false,
            callers_dirty: false,
        };
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
                            shard: entry.shard,
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
        let _ = fs::remove_dir_all(self.dir.join(SHARDS_DIR));
        let _ = fs::remove_file(self.dir.join(CALLERS_FILE));
        for name in LEGACY_FILES {
            let _ = fs::remove_file(self.dir.join(name));
        }
    }

    fn shard_path(&self, shard: &str) -> PathBuf {
        self.dir.join(SHARDS_DIR).join(shard)
    }

    /// A megadott fájlok bélyegének ellenőrzése (stat, tartalom-olvasás nélkül); a változott / új fájlok elemzése
    /// (párhuzamosan) és csak az ő shardjuk újraírása. `full`: a `files` a teljes projekt a bejárás sorrendjében —
    /// a benne nem szereplő (törölt, átnevezett) fájlok és shardjaik kikerülnek, és ez lesz a hívó-index sorrendje.
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

    /// A teljes projekt-listában nem szereplő fájlok (és shardjaik) eltávolítása.
    fn prune(&mut self, order: &[String]) {
        let keep: HashSet<&str> = order.iter().map(String::as_str).collect();
        let gone: Vec<String> = self
            .tracked
            .keys()
            .filter(|path| !keep.contains(path.as_str()))
            .cloned()
            .collect();
        for path in &gone {
            if let Some(tracked) = self.tracked.remove(path) {
                let _ = fs::remove_file(self.shard_path(&tracked.shard));
            }
            self.loaded.remove(path);
            self.manifest_dirty = true;
            self.callers_fresh = false;
        }
        self.loaded.retain(|path, _| keep.contains(path.as_str()));
        if let Some(facts) = self.callers_facts.as_mut() {
            let before = facts.len();
            facts.retain(|path, _| keep.contains(path.as_str()));
            if facts.len() != before {
                self.callers_dirty = true;
                self.callers_fresh = false;
            }
        }
    }

    /// Elemzés párhuzamosan, a shardok kiírása; hibánál a sorrendben első hibás fájl hibája (mint a régi úton).
    fn analyze(&mut self, work: &[Work<'_>], analyzer: &Analyzer) -> Result<()> {
        if work.is_empty() {
            return Ok(());
        }
        let shards = self.dir.join(SHARDS_DIR);
        let _ = fs::create_dir_all(&shards);
        let results: Vec<Result<Analyzed>> = work
            .par_iter()
            .map(|&(path, stamp)| {
                let graph = analyzer.analyze_path(path)?.graph;
                let snippets =
                    snippet::call_snippets(path, graph.calls.iter().map(|call| call.line));
                let key = path.to_string_lossy().into_owned();
                let facts = FileFacts { graph, snippets };
                let shard = codec::shard_name(&key);
                // a kiírás hibája nem állítja meg a lekérdezést (a fájl legközelebb újraelemződik)
                let written = codec::write_shard(&shards.join(&shard), &key, stamp, &facts).is_ok();
                Ok((key, stamp, facts, written.then_some(shard)))
            })
            .collect();
        for result in results {
            let (key, stamp, facts, shard) = result?;
            match shard {
                Some(shard) => {
                    self.tracked.insert(key.clone(), Tracked { stamp, shard });
                }
                None => {
                    self.tracked.remove(&key);
                }
            }
            if let Some(entries) = self.callers_facts.as_mut() {
                entries.insert(
                    key.clone(),
                    CallerEntry {
                        stamp,
                        facts: facts.slim(),
                        frame: None,
                    },
                );
                self.callers_dirty = true;
            }
            self.manifest_dirty = true;
            self.callers_fresh = false;
            self.loaded.insert(key, facts);
        }
        Ok(())
    }

    /// A megadott fájlok shardjai a memóriába (párhuzamosan); a sérült / hiányzó shard fájlja újraelemződik.
    /// A `refresh` után hívandó (a bélyeg akkor már a manifestben van).
    pub(crate) fn load(&mut self, files: &[PathBuf], analyzer: &Analyzer) -> Result<()> {
        let want: Vec<(&Path, String)> = files
            .iter()
            .map(|file| (file.as_path(), file.to_string_lossy().into_owned()))
            .filter(|(_, key)| !self.loaded.contains_key(key))
            .collect();
        if want.is_empty() {
            return Ok(());
        }
        let read: Vec<Option<FileFacts>> = {
            let tracked = &self.tracked;
            let shards = self.dir.join(SHARDS_DIR);
            want.par_iter()
                .map(|(_, key)| {
                    let entry = tracked.get(key)?;
                    codec::read_shard(&shards.join(&entry.shard), key, entry.stamp)
                })
                .collect()
        };
        let mut broken: Vec<Work<'_>> = Vec::new();
        for ((path, key), facts) in want.into_iter().zip(read) {
            match facts {
                Some(facts) => {
                    self.loaded.insert(key, facts);
                }
                None => broken.push((path, FileStamp::of(path))),
            }
        }
        self.analyze(&broken, analyzer)
    }

    /// A fájl gráfja (a `load` után; az út a bejárás / lekérdezés alakjában).
    pub(crate) fn graph(&self, path: &str) -> Option<&FileGraph> {
        self.loaded.get(path).map(|facts| &facts.graph)
    }

    /// A fájl hívás-sorai tömörítve (`(sor, kód)`), a `SourceLines::remember`-hez: a betöltött shardból, vagy a
    /// hívó-index bemenetéből (a `--depth` hívói így shard nélkül is kiírhatók).
    pub(crate) fn snippets(&self, path: &str) -> Option<&[(u32, String)]> {
        self.facts(path).map(|facts| facts.snippets.as_slice())
    }

    fn facts(&self, path: &str) -> Option<&FileFacts> {
        self.loaded.get(path).or_else(|| {
            self.callers_facts
                .as_ref()?
                .get(path)
                .map(|entry| &entry.facts)
        })
    }

    /// Szimbólum-keresés: egy metódus / függvény sortartománya (az első egyező definíció).
    pub(crate) fn method_range(
        &self,
        file: &str,
        container: Option<&str>,
        name: &str,
    ) -> Option<(u32, u32)> {
        self.facts(file)?
            .graph
            .methods
            .iter()
            .find(|method| method.container.as_deref() == container && method.name == name)
            .map(|method| (method.line, method.end_line.unwrap_or(method.line)))
    }

    /// A hívó-index naprakészre hozása (a teljes `refresh` után): a `callers.bin`-ből, a változott fájlok bemenete
    /// a shardjukból (vagy a friss elemzésből). Ha semmi sem változott az utolsó építés óta, a régi marad — a
    /// folyamatban élő gráf (pl. az MCP-szerveré) így kérésenként csak a változást fizeti.
    pub(crate) fn ensure_callers(&mut self, analyzer: &Analyzer) -> Result<()> {
        if self.callers_fresh {
            return Ok(());
        }
        if self.callers_facts.is_none() {
            self.callers_facts = Some(self.read_callers());
        }
        // a hiányzó vagy elavult bemenetű fájlok
        let stale: Vec<PathBuf> = {
            let entries = self.callers_facts.as_ref();
            self.order
                .iter()
                .filter(|path| {
                    let stored = entries.and_then(|entries| entries.get(path.as_str()));
                    let current = self.tracked.get(path.as_str()).map(|tracked| tracked.stamp);
                    stored.is_none_or(|entry| Some(entry.stamp) != current)
                })
                .map(PathBuf::from)
                .collect()
        };
        if !stale.is_empty() {
            let borrowed: HashSet<String> = stale
                .iter()
                .map(|path| path.to_string_lossy().into_owned())
                .filter(|key| !self.loaded.contains_key(key))
                .collect();
            self.load(&stale, analyzer)?;
            let mut entries = self.callers_facts.take().unwrap_or_default();
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
                    CallerEntry {
                        stamp,
                        facts: facts.slim(),
                        frame: None,
                    },
                );
            }
            // a csak a hívó-indexhez betöltött shardok nem maradnak a memóriában
            self.loaded.retain(|key, _| !borrowed.contains(key));
            self.callers_facts = Some(entries);
            self.callers_dirty = true;
        }
        if let Some(entries) = self.callers_facts.as_mut() {
            let keep: HashSet<&str> = self.order.iter().map(String::as_str).collect();
            let before = entries.len();
            entries.retain(|path, _| keep.contains(path.as_str()));
            if entries.len() != before {
                self.callers_dirty = true;
            }
        }
        let built = {
            let entries = self.callers_facts.as_ref();
            CallerIndex::build(self.order.iter().filter_map(|path| {
                let entry = entries?.get(path)?;
                Some((path.as_str(), &entry.facts.graph))
            }))
        };
        self.callers = built;
        self.callers_fresh = true;
        Ok(())
    }

    /// A `callers.bin` beolvasása (a keretek párhuzamos dekódolásával); a sérült keret kimarad (újraépül).
    fn read_callers(&self) -> HashMap<String, CallerEntry> {
        let frames = codec::read_callers(&self.dir.join(CALLERS_FILE));
        frames
            .into_par_iter()
            .filter_map(|frame| {
                let (path, stamp, facts) = codec::decode_record(&frame)?;
                Some((
                    path,
                    CallerEntry {
                        stamp,
                        facts,
                        frame: Some(frame),
                    },
                ))
            })
            .collect()
    }

    /// A teljes projekt fordított hívó-indexe (az `ensure_callers` után naprakész).
    pub(crate) fn caller_index(&self) -> &CallerIndex {
        &self.callers
    }

    /// Kiírás, ha változott valami: a manifest és (ha kellett) a `callers.bin`, mindkettő atomikusan. A shardok már
    /// az elemzéskor kiíródtak (csak a változott fájloké).
    pub(crate) fn save(&mut self) -> Result<()> {
        if self.manifest_dirty {
            let mut entries: Vec<ManifestEntry> = self
                .tracked
                .iter()
                .map(|(path, tracked)| ManifestEntry {
                    path: path.clone(),
                    mtime: tracked.stamp.mtime,
                    size: tracked.stamp.size,
                    shard: tracked.shard.clone(),
                })
                .collect();
            entries.sort_by(|a, b| a.path.cmp(&b.path));
            codec::write_manifest(&self.dir.join(MANIFEST_FILE), &entries)?;
            self.manifest_dirty = false;
        }
        if self.callers_dirty
            && let Some(entries) = self.callers_facts.as_mut()
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
            codec::write_callers(&self.dir.join(CALLERS_FILE), &frames)?;
            self.callers_dirty = false;
        }
        Ok(())
    }
}
