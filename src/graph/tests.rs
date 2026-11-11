//! A projekt-gráf tárolása és frissítése: a `FileGraph` pontos visszaállása a bináris alakból, meleg megnyitás
//! elemzés nélkül, szerkesztés → frissül, törlés → kikerül, sérült / régi tároló → csendes újraépítés, és a
//! folyamaton belüli közös példány.

use std::fs;
use std::path::{Path, PathBuf};

use std::time::{Duration, Instant};

use super::record::{FileFacts, FileStamp};
use super::{
    ProjectGraph, RefreshLock, Refreshed, codec, lock, refresh_all, refresh_paths, shared, watch,
};
use crate::analysis::Analyzer;
use crate::io::FileWalker;
use crate::lang::LanguageRegistry;

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("callgraph-graph-{}-{n}-{id}", std::process::id()));
        fs::create_dir_all(path.join(".callgraph")).unwrap();
        Self(path.canonicalize().unwrap())
    }

    fn store(&self) -> PathBuf {
        self.0.join(".callgraph")
    }

    fn write(&self, name: &str, text: &str) -> PathBuf {
        let path = self.0.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, text).unwrap();
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const ORDER: &str = "<?php\nnamespace App;\nclass Order { function save() {} }\nclass Caller { function first() { Order::save(); } }\n";

fn key(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// Teljes frissítés az alapértelmezett nyelvekkel, a hívó-indexszel, majd mentés.
fn refresh_full(graph: &mut ProjectGraph, files: &[PathBuf]) {
    let analyzer = Analyzer::with_defaults();
    graph.refresh(files, true, &analyzer).unwrap();
    graph.ensure_callers(&analyzer).unwrap();
    graph.save().unwrap();
}

fn callers_of(graph: &ProjectGraph, pattern: &str) -> Vec<String> {
    graph
        .caller_index()
        .callers(pattern, 1)
        .unwrap()
        .into_iter()
        .map(|row| row.caller)
        .collect()
}

/// A tárolt alak minden mintafájlra pontosan visszaadja a `FileGraph`-ot (a JSON-alakból kimaradó mezőket is:
/// `Property::js`, `Constant::caller` / `js`, az üres `Imports` objektum-jelzője, a `.vue` SFC-leírás).
#[test]
fn stored_graphs_round_trip_exactly() {
    let analyzer = Analyzer::with_defaults();
    let dir = TempDir::new();
    // a golden mintákon kívül PHP és JS/Vue (props, konstansok, importok)
    let extra = [
        dir.write(
            "a.php",
            "<?php\nnamespace App;\nuse Foo\\Bar;\nclass A extends Bar implements \\Countable\n{\n    const LIMIT = 5;\n    private ?int $n = null;\n    public function __construct(private readonly string $name) {}\n    /** doc */\n    public static function run(int $x): array { if ($x) { return [self::LIMIT, \"msg {$x}\"]; } $this->n = 1; return Bar::go($x); }\n}\n",
        ),
        dir.write(
            "b.js",
            "import { x } from './x.js';\nexport const LIMIT = 3;\nexport function load(id) { return x(id, 'needle'); }\n",
        ),
        dir.write(
            "c.vue",
            "<template><div @click=\"save\">{{ title }}</div></template>\n<script>\nexport default { props: { title: String }, methods: { save() { this.$emit('saved', 1); } } };\n</script>\n<style>.a { color: red; }</style>\n",
        ),
    ];
    let samples = Path::new(env!("CARGO_MANIFEST_DIR")).join("golden/samples");
    let mut files = FileWalker::for_registry(analyzer.registry()).collect(&[samples], &mut |_| {});
    files.extend(extra);
    assert!(files.len() > 10, "{files:?}");
    for file in files {
        let graph = analyzer.analyze_path(&file).unwrap().graph;
        let facts = FileFacts {
            snippets: vec![(1, "x".to_owned())],
            graph,
        };
        let stamp = FileStamp {
            mtime: 7,
            size: 9,
            ctime: 5,
        };
        let bytes = codec::encode_record(&key(&file), stamp, &facts).unwrap();
        let (path, stored, back) = codec::decode_record(&bytes).unwrap();
        assert_eq!((path, stored), (key(&file), stamp));
        assert_eq!(back, facts, "{}", file.display());
        // a JSON-kimenet is azonos
        assert_eq!(
            serde_json::to_string(&back.graph).unwrap(),
            serde_json::to_string(&facts.graph).unwrap()
        );
    }
}

#[test]
fn corrupt_bytes_decode_to_none_without_panicking() {
    let facts = FileFacts {
        graph: crate::model::FileGraphBuilder::new("/x.php").build(),
        snippets: Vec::new(),
    };
    let bytes = codec::encode_record("/x.php", FileStamp::default(), &facts).unwrap();
    for cut in [0, 3, 4, bytes.len() / 2, bytes.len() - 1] {
        assert!(codec::decode_record(&bytes[..cut]).is_none(), "{cut}");
    }
    let mut huge = bytes.clone();
    huge[..4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(codec::decode_record(&huge).is_none());
    let mut flipped = bytes;
    let last = flipped.len() - 1;
    flipped[last] ^= 0xff;
    let _ = codec::decode_record(&flipped);
}

#[test]
fn warm_open_needs_no_source_parsing() {
    let dir = TempDir::new();
    let file = dir.write("a.php", ORDER);
    let mut graph = ProjectGraph::open(dir.store());
    refresh_full(&mut graph, std::slice::from_ref(&file));
    graph
        .load_full(std::slice::from_ref(&file), &Analyzer::with_defaults())
        .unwrap();
    let first = graph.graph(&key(&file)).cloned().unwrap();
    for name in ["index.bin", "facts.bin", "graph.pack"] {
        assert!(dir.store().join(name).is_file(), "{name}");
    }
    // nyelvek nélküli elemző: bármilyen forrás-elemzés hibát adna — a meleg megnyitás mégis kiszolgál
    let none = Analyzer::new(LanguageRegistry::new());
    let mut warm = ProjectGraph::open(dir.store());
    warm.refresh(std::slice::from_ref(&file), true, &none)
        .unwrap();
    warm.ensure_callers(&none).unwrap();
    assert_eq!(callers_of(&warm, "Order::save"), vec!["first"]);
    // a hívó-indexhez a pack sem kell: a sortartomány és a hívás-sor a facts.bin-ből
    assert!(warm.graph(&key(&file)).is_none());
    assert_eq!(warm.stats().records_read, 0);
    assert_eq!(
        warm.method_range(&key(&file), Some("Caller"), "first"),
        Some((4, 4))
    );
    assert_eq!(
        warm.snippets(&key(&file)).unwrap(),
        &[(
            4,
            "class Caller { function first() { Order::save(); } }".to_owned()
        )]
    );
    warm.load_full(std::slice::from_ref(&file), &none).unwrap();
    assert_eq!(warm.graph(&key(&file)), Some(&first));
    assert_eq!(warm.stats().records_read, 1);
    assert_eq!(warm.stats().analyzed, 0);
}

/// A meleg, teljes hatókörű lekérdezés a projekt méretétől függetlenül ugyanannyi tároló-fájlt nyit (manifest +
/// facts.bin), és egyetlen teljes rekordot sem olvas; egy egyfájlos nézet egy pack-megnyitás, egy rekord.
#[test]
fn a_warm_query_opens_a_constant_number_of_files() {
    let none = Analyzer::new(LanguageRegistry::new());
    let mut seen = Vec::new();
    for count in [3, 40] {
        let dir = TempDir::new();
        let files: Vec<PathBuf> = (0..count)
            .map(|n| {
                dir.write(
                    &format!("f{n}.php"),
                    &format!(
                        "<?php\nnamespace App;\nclass C{n} {{ const K = {n}; function run() {{ Order::save(); return \"s{n}\"; }} }}\n"
                    ),
                )
            })
            .collect();
        let mut cold = ProjectGraph::open(dir.store());
        refresh_full(&mut cold, &files);
        // `--depth` / `callers`: teljes frissítés + hívó-index
        let mut deep = ProjectGraph::open(dir.store());
        deep.refresh(&files, true, &none).unwrap();
        deep.ensure_callers(&none).unwrap();
        assert_eq!(callers_of(&deep, "Order::save").len(), count);
        // `--grep` 1. szint / `--strings` / `--const` a teljes hatókörre: szűkített tények
        let mut wide = ProjectGraph::open(dir.store());
        wide.refresh(&files, false, &none).unwrap();
        wide.load_facts(&files, &none).unwrap();
        // (a rövid "s{n}" literált a PHP-elemző nem gyűjti a strings közé; a --strings egyezését a
        // tests/project_graph.rs összevetése ellenőrzi)
        assert!(files.iter().all(|file| {
            wide.slim_graph(&key(file))
                .is_some_and(|graph| graph.constants.len() == 1)
        }));
        // egyfájlos nézet: egy pack-megnyitás, egy rekord
        let mut single = ProjectGraph::open(dir.store());
        single.refresh(&files[..1], false, &none).unwrap();
        single.load_full(&files[..1], &none).unwrap();
        seen.push((deep.stats(), wide.stats(), single.stats()));
    }
    for (deep, wide, single) in &seen {
        assert_eq!(
            (deep.files_opened, deep.records_read, deep.analyzed),
            (2, 0, 0)
        );
        assert_eq!(
            (wide.files_opened, wide.records_read, wide.analyzed),
            (2, 0, 0)
        );
        assert_eq!(
            (single.files_opened, single.records_read, single.analyzed),
            (2, 1, 0)
        );
    }
}

#[test]
fn edited_file_is_reanalyzed_and_deleted_file_dropped() {
    let dir = TempDir::new();
    let file = dir.write("a.php", ORDER);
    let other = dir.write("b.php", "<?php\nfunction helper() {}\n");
    let mut graph = ProjectGraph::open(dir.store());
    refresh_full(&mut graph, &[file.clone(), other.clone()]);
    assert_eq!(callers_of(&graph, "Order::save"), vec!["first"]);
    fs::write(&file, ORDER.replace("first", "secondName")).unwrap();
    fs::remove_file(&other).unwrap();
    // új megnyitás (mint egy új CLI-hívás), és a memóriában maradt példány (mint az MCP) is frissül
    let mut reopened = ProjectGraph::open(dir.store());
    for graph in [&mut graph, &mut reopened] {
        refresh_full(graph, std::slice::from_ref(&file));
        assert_eq!(callers_of(graph, "Order::save"), vec!["secondName"]);
        assert!(graph.method_range(&key(&other), None, "helper").is_none());
    }
    // csak a változott fájl elemződött újra (hozzáfűzés a packhez), a teljes rekordja is az új
    let mut again = ProjectGraph::open(dir.store());
    let none = Analyzer::new(LanguageRegistry::new());
    again
        .refresh(std::slice::from_ref(&file), true, &none)
        .unwrap();
    again.load_full(std::slice::from_ref(&file), &none).unwrap();
    assert!(
        again
            .graph(&key(&file))
            .unwrap()
            .methods
            .iter()
            .any(|method| method.name == "secondName")
    );
}

#[test]
fn many_rewrites_compact_the_pack() {
    let dir = TempDir::new();
    // egy nagyobb fájl, sokszor átírva: a holt terület a pack felénél több lesz → tömörítés
    let body: String = (0..2000)
        .map(|n| format!("    function m{n}() {{ Order::save(); return \"{n}\"; }}\n"))
        .collect();
    let file = dir.write("big.php", &format!("<?php\nclass Big {{\n{body}}}\n"));
    let mut graph = ProjectGraph::open(dir.store());
    refresh_full(&mut graph, std::slice::from_ref(&file));
    let pack = dir.store().join("graph.pack");
    let single = fs::metadata(&pack).unwrap().len();
    for round in 0..12 {
        fs::write(
            &file,
            format!("<?php\nclass Big {{\n{body}    function r{round}() {{}}\n}}\n"),
        )
        .unwrap();
        refresh_full(&mut graph, std::slice::from_ref(&file));
    }
    assert!(
        fs::metadata(&pack).unwrap().len() < single * 2,
        "the pack was not compacted"
    );
    let mut warm = ProjectGraph::open(dir.store());
    let none = Analyzer::new(LanguageRegistry::new());
    warm.refresh(std::slice::from_ref(&file), true, &none)
        .unwrap();
    warm.load_full(std::slice::from_ref(&file), &none).unwrap();
    assert!(
        warm.graph(&key(&file))
            .unwrap()
            .methods
            .iter()
            .any(|method| method.name == "r11")
    );
}

#[test]
fn corrupt_or_old_storage_is_rebuilt_silently() {
    let dir = TempDir::new();
    let file = dir.write("a.php", ORDER);
    let analyzer = Analyzer::with_defaults();
    // a régi tárolók (JSONL gráf, JSON-cache, shardos változat) az első építéskor törlődnek; a jegyzetek maradnak
    for name in ["index.jsonl", "cache.jsonl", "callers.bin", "notes.jsonl"] {
        fs::write(dir.store().join(name), "{}\n").unwrap();
    }
    fs::create_dir_all(dir.store().join("graph")).unwrap();
    fs::write(dir.store().join("graph/0123456789abcdef.bin"), "x").unwrap();
    let mut graph = ProjectGraph::open(dir.store());
    refresh_full(&mut graph, std::slice::from_ref(&file));
    for gone in ["index.jsonl", "cache.jsonl", "callers.bin", "graph"] {
        assert!(!dir.store().join(gone).exists(), "{gone}");
    }
    assert!(dir.store().join("notes.jsonl").exists());
    // sérült manifest, facts.bin, pack — mindegyik után helyes válasz
    for broken in ["index.bin", "facts.bin", "graph.pack"] {
        fs::write(dir.store().join(broken), b"CGB1\x05\x00\x00\x00garbage").unwrap();
        let mut graph = ProjectGraph::open(dir.store());
        graph
            .refresh(std::slice::from_ref(&file), true, &analyzer)
            .unwrap();
        graph.ensure_callers(&analyzer).unwrap();
        graph
            .load_full(std::slice::from_ref(&file), &analyzer)
            .unwrap();
        assert_eq!(callers_of(&graph, "Order::save"), vec!["first"], "{broken}");
        assert!(graph.graph(&key(&file)).is_some(), "{broken}");
        graph.save().unwrap();
    }
}

#[test]
fn the_process_keeps_one_graph_per_project() {
    let dir = TempDir::new();
    let file = dir.write("a.php", ORDER);
    let first = shared(&dir.0).unwrap();
    refresh_full(&mut lock(&first), std::slice::from_ref(&file));
    // ugyanaz a gyökér (más alakban is) → ugyanaz a példány, lemezről nem töltődik újra
    let again = shared(&dir.0.join(".")).unwrap();
    assert!(std::sync::Arc::ptr_eq(&first, &again));
    assert_eq!(callers_of(&lock(&again), "Order::save"), vec!["first"]);
}

// --- előre-frissítés (`callgraph refresh`), zár, MCP-figyelő ---

fn analyzed(graph: &ProjectGraph) -> usize {
    graph.stats().analyzed
}

/// A megadott utak frissítése: a változott és az új fájl elemződik, a törölt kikerül, a nem kód / nem létező /
/// gyökéren kívüli út kimarad; utána egy új megnyitás semmit sem elemez, és a tények is naprakészek.
#[test]
fn refresh_paths_reflects_changed_new_and_deleted_files() {
    let dir = TempDir::new();
    let analyzer = Analyzer::with_defaults();
    let a = dir.write("a.php", ORDER);
    let b = dir.write("src/b.php", "<?php\nfunction helper() {}\n");
    let mut graph = ProjectGraph::open(dir.store());
    let first = refresh_all(&mut graph, &dir.0, &analyzer).unwrap();
    assert_eq!((first.updated, first.removed, first.unchanged), (2, 0, 0));
    graph.save().unwrap();

    fs::write(&a, ORDER.replace("first", "secondName")).unwrap();
    let c = dir.write("src/c.php", "<?php\nclass Fresh { function fresh() {} }\n");
    fs::remove_file(&b).unwrap();
    dir.write("notes.txt", "not code");
    let mut graph = ProjectGraph::open(dir.store());
    let paths = [
        PathBuf::from("a.php"),
        c.clone(),
        b.clone(),
        PathBuf::from("notes.txt"),
        PathBuf::from("missing.php"),
        std::env::temp_dir().join("callgraph-elsewhere/x.php"),
    ];
    let done = refresh_paths(&mut graph, &dir.0, &paths, &analyzer).unwrap();
    assert_eq!(
        done,
        Refreshed {
            updated: 2,
            removed: 1,
            unchanged: 0
        }
    );
    graph.save().unwrap();

    let mut warm = ProjectGraph::open(dir.store());
    assert!(warm.tracked_under(&b).is_empty());
    let again = refresh_paths(&mut warm, &dir.0, &paths, &analyzer).unwrap();
    assert_eq!((again.updated, again.removed, again.unchanged), (0, 0, 2));
    assert_eq!(analyzed(&warm), 0);
    assert!(
        warm.method_range(&key(&c), Some("Fresh"), "fresh")
            .is_some()
    );
    assert!(
        warm.method_range(&key(&a), Some("Caller"), "secondName")
            .is_some()
    );
}

/// Egy törölt mappa minden ismert fájlja kikerül; a teljes frissítés a törölt fájlt is észreveszi.
#[test]
fn refresh_handles_deleted_directories_and_full_refresh() {
    let dir = TempDir::new();
    let analyzer = Analyzer::with_defaults();
    let a = dir.write("a.php", ORDER);
    let nested = dir.write("lib/deep/n.php", "<?php\nfunction n() {}\n");
    dir.write("lib/m.php", "<?php\nfunction m() {}\n");
    let mut graph = ProjectGraph::open(dir.store());
    refresh_all(&mut graph, &dir.0, &analyzer).unwrap();
    graph.save().unwrap();

    fs::remove_dir_all(dir.0.join("lib")).unwrap();
    let done = refresh_paths(&mut graph, &dir.0, &[PathBuf::from("lib")], &analyzer).unwrap();
    assert_eq!((done.updated, done.removed), (0, 2));
    assert!(graph.tracked_under(&nested).is_empty());

    dir.write("d.php", "<?php\nfunction d() {}\n");
    fs::remove_file(&a).unwrap();
    let full = refresh_all(&mut graph, &dir.0, &analyzer).unwrap();
    assert_eq!((full.updated, full.removed, full.unchanged), (1, 1, 0));
    graph.save().unwrap();
    let warm = ProjectGraph::open(dir.store());
    assert!(warm.tracked_under(&a).is_empty());
    assert_eq!(warm.tracked_under(&dir.0).len(), 1);
}

/// A második frissítő a zárra vár, majd feladja; a zár eldobása után megszerezhető.
#[test]
fn a_second_refresh_lock_waits_then_gives_up() {
    let dir = TempDir::new();
    let held = RefreshLock::acquire(&dir.store(), Duration::ZERO).expect("first lock");
    let start = Instant::now();
    assert!(RefreshLock::acquire(&dir.store(), Duration::from_millis(100)).is_none());
    assert!(start.elapsed() >= Duration::from_millis(100));
    drop(held);
    assert!(RefreshLock::acquire(&dir.store(), Duration::ZERO).is_some());
}

/// Két egyszerre induló frissítő (szálanként saját példány, mint két folyamat) a zár mögött egymás után fut: a
/// tároló ép, és utána semmit sem kell újraelemezni.
#[test]
fn concurrent_refreshes_leave_a_consistent_store() {
    let dir = TempDir::new();
    for n in 0..8 {
        dir.write(
            &format!("f{n}.php"),
            &format!("<?php\nfunction f{n}() {{ g{n}(); }}\n"),
        );
    }
    let workers: Vec<_> = (0..2)
        .map(|_| {
            let root = dir.0.clone();
            std::thread::spawn(move || {
                let store = root.join(".callgraph");
                let _lock = RefreshLock::acquire(&store, Duration::from_secs(30)).expect("lock");
                let mut graph = ProjectGraph::open(store);
                refresh_all(&mut graph, &root, &Analyzer::with_defaults()).unwrap();
                graph.save().unwrap();
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
    let mut warm = ProjectGraph::open(dir.store());
    let done = refresh_all(&mut warm, &dir.0, &Analyzer::with_defaults()).unwrap();
    assert_eq!((done.updated, done.removed, done.unchanged), (0, 0, 8));
    assert_eq!(analyzed(&warm), 0);
}

#[test]
fn debouncer_waits_for_quiet_but_not_forever() {
    let quiet = Duration::from_millis(300);
    let mut pending = watch::Debouncer::new(quiet, Duration::from_secs(2));
    let t0 = Instant::now();
    assert_eq!(pending.wait(t0), None);
    pending.push(Vec::new(), t0);
    assert_eq!(pending.wait(t0), None);
    pending.push(vec![PathBuf::from("/p/b.php")], t0);
    assert_eq!(pending.wait(t0), Some(quiet));
    assert!(pending.take(t0 + Duration::from_millis(299)).is_none());
    // egy újabb esemény kitolja
    pending.push(
        vec![PathBuf::from("/p/a.php"), PathBuf::from("/p/b.php")],
        t0 + Duration::from_millis(200),
    );
    assert!(pending.take(t0 + Duration::from_millis(450)).is_none());
    assert_eq!(
        pending.take(t0 + Duration::from_millis(500)),
        Some(vec![PathBuf::from("/p/a.php"), PathBuf::from("/p/b.php")])
    );
    assert!(pending.take(t0 + Duration::from_secs(10)).is_none());
    // folyamatos írás mellett is legfeljebb 2 s késés
    let mut at = t0;
    let mut flushed = None;
    for step in 0..20 {
        pending.push(vec![PathBuf::from(format!("/p/{step}.php"))], at);
        at += Duration::from_millis(200);
        if let Some(batch) = pending.take(at) {
            flushed = Some((at - t0, batch.len()));
            break;
        }
    }
    assert_eq!(flushed, Some((Duration::from_secs(2), 10)));
}

#[test]
fn watcher_events_are_filtered_like_the_walker() {
    use notify::event::{AccessKind, AccessMode, CreateKind, DataChange, MetadataKind, ModifyKind};
    use notify::{Event, EventKind};
    let dir = TempDir::new();
    let root = &dir.0;
    let code = dir.write("a.php", ORDER);
    let text = dir.write("notes.txt", "x");
    let dep = dir.write("node_modules/x/i.js", "export const a = 1;\n");
    let stored = dir.write(".callgraph/x.php", "<?php\n");
    let src = root.join("src");
    fs::create_dir_all(&src).unwrap();
    let walker = FileWalker::for_registry(Analyzer::with_defaults().registry());
    let modify = EventKind::Modify(ModifyKind::Data(DataChange::Content));
    let paths = |kind: EventKind, path: &PathBuf| {
        watch::changed_paths(root, &Event::new(kind).add_path(path.clone()), &walker)
    };
    assert_eq!(paths(modify, &code), vec![code.clone()]);
    assert!(paths(modify, &text).is_empty());
    assert!(paths(modify, &dep).is_empty());
    assert!(paths(modify, &stored).is_empty());
    assert!(paths(modify, &std::env::temp_dir().join("elsewhere.php")).is_empty());
    assert!(paths(EventKind::Access(AccessKind::Read), &code).is_empty());
    assert_eq!(
        paths(
            EventKind::Access(AccessKind::Close(AccessMode::Write)),
            &code
        ),
        vec![code.clone()]
    );
    // mappa: csak létrehozás / törlés / átnevezés számít
    assert!(
        paths(
            EventKind::Modify(ModifyKind::Metadata(MetadataKind::Any)),
            &src
        )
        .is_empty()
    );
    assert_eq!(
        paths(EventKind::Create(CreateKind::Folder), &src),
        vec![src.clone()]
    );
    // út nélküli esemény (túlcsordulás): a teljes gyökér
    assert_eq!(
        watch::changed_paths(root, &Event::new(EventKind::Other), &walker),
        vec![root.clone()]
    );
}

/// A figyelő egy adagja frissíti a memóriában lévő gráfot és menti a tárolót (determinisztikus, szál nélkül).
#[test]
fn watcher_batch_updates_the_shared_graph_and_saves() {
    let dir = TempDir::new();
    let analyzer = Analyzer::with_defaults();
    let a = dir.write("a.php", ORDER);
    let graph = shared(&dir.0).unwrap();
    refresh_full(&mut lock(&graph), std::slice::from_ref(&a));
    let b = dir.write("b.php", "<?php\nclass Watched { function watched() {} }\n");
    watch::apply(&dir.0, &graph, std::slice::from_ref(&b), &analyzer);
    assert!(
        lock(&graph)
            .method_range(&key(&b), Some("Watched"), "watched")
            .is_some()
    );
    let mut warm = ProjectGraph::open(dir.store());
    refresh_paths(&mut warm, &dir.0, std::slice::from_ref(&b), &analyzer).unwrap();
    assert_eq!(analyzed(&warm), 0);
}

/// Valódi figyelő: egy fájl írása után a gráf egy időkorláton belül frissül. A fájlrendszer-események időzítése
/// CI-n bizonytalan, ezért alapból nem fut (`cargo test -- --ignored`).
#[test]
#[ignore = "real file-system events; timing is not reliable on CI"]
fn real_watcher_refreshes_a_written_file() {
    let dir = TempDir::new();
    dir.write("a.php", ORDER);
    let graph = shared(&dir.0).unwrap();
    assert!(watch::start(dir.0.clone(), std::sync::Arc::clone(&graph)));
    std::thread::sleep(Duration::from_millis(500));
    let b = dir.write("b.php", "<?php\nfunction watched() {}\n");
    let deadline = Instant::now() + Duration::from_secs(10);
    while lock(&graph).tracked_under(&b).is_empty() {
        assert!(
            Instant::now() < deadline,
            "the watcher did not refresh b.php"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Azonos méret + visszaállított mtime mellett is észrevehető a tartalomváltozás (unixon a ctime miatt).
#[cfg(unix)]
#[test]
fn restored_mtime_with_same_size_still_changes_stamp() {
    let dir = std::env::temp_dir().join(format!("callgraph-stamp-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let file = dir.join("a.php");
    fs::write(&file, "<?php // aaaa\n").unwrap();
    let modified = fs::metadata(&file).unwrap().modified().unwrap();
    let before = FileStamp::of(&file);
    std::thread::sleep(Duration::from_millis(20));
    fs::write(&file, "<?php // bbbb\n").unwrap();
    fs::File::options()
        .write(true)
        .open(&file)
        .unwrap()
        .set_modified(modified)
        .unwrap();
    let after = FileStamp::of(&file);
    assert_eq!((before.mtime, before.size), (after.mtime, after.size));
    assert_ne!(before, after);
    let _ = fs::remove_dir_all(dir);
}
