//! A projekt-gráf tárolása és frissítése: a `FileGraph` pontos visszaállása a bináris alakból, meleg megnyitás
//! elemzés nélkül, szerkesztés → frissül, törlés → kikerül, sérült / régi tároló → csendes újraépítés, és a
//! folyamaton belüli közös példány.

use std::fs;
use std::path::{Path, PathBuf};

use super::record::{FileFacts, FileStamp};
use super::{ProjectGraph, codec, lock, shared};
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

    fn shards(&self) -> Vec<PathBuf> {
        fs::read_dir(self.store().join("graph"))
            .map(|dir| dir.map(|entry| entry.unwrap().path()).collect())
            .unwrap_or_default()
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
        let stamp = FileStamp { mtime: 7, size: 9 };
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
        .load(std::slice::from_ref(&file), &Analyzer::with_defaults())
        .unwrap();
    let first = graph.graph(&key(&file)).cloned().unwrap();
    assert!(dir.store().join("index.bin").is_file());
    assert!(dir.store().join("callers.bin").is_file());
    assert_eq!(dir.shards().len(), 1);
    // nyelvek nélküli elemző: bármilyen forrás-elemzés hibát adna — a meleg megnyitás mégis kiszolgál
    let none = Analyzer::new(LanguageRegistry::new());
    let mut warm = ProjectGraph::open(dir.store());
    warm.refresh(std::slice::from_ref(&file), true, &none)
        .unwrap();
    warm.ensure_callers(&none).unwrap();
    assert_eq!(callers_of(&warm, "Order::save"), vec!["first"]);
    // a hívó-indexhez shard sem kell: a sortartomány és a hívás-sor a callers.bin-ből
    assert!(warm.graph(&key(&file)).is_none());
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
    warm.load(std::slice::from_ref(&file), &none).unwrap();
    assert_eq!(warm.graph(&key(&file)), Some(&first));
}

#[test]
fn edited_file_is_reanalyzed_and_deleted_file_dropped() {
    let dir = TempDir::new();
    let file = dir.write("a.php", ORDER);
    let other = dir.write("b.php", "<?php\nfunction helper() {}\n");
    let mut graph = ProjectGraph::open(dir.store());
    refresh_full(&mut graph, &[file.clone(), other.clone()]);
    assert_eq!(callers_of(&graph, "Order::save"), vec!["first"]);
    assert_eq!(dir.shards().len(), 2);
    fs::write(&file, ORDER.replace("first", "secondName")).unwrap();
    fs::remove_file(&other).unwrap();
    // új megnyitás (mint egy új CLI-hívás), és a memóriában maradt példány (mint az MCP) is frissül
    let mut reopened = ProjectGraph::open(dir.store());
    for graph in [&mut graph, &mut reopened] {
        refresh_full(graph, std::slice::from_ref(&file));
        assert_eq!(callers_of(graph, "Order::save"), vec!["secondName"]);
        assert!(graph.method_range(&key(&other), None, "helper").is_none());
    }
    assert_eq!(dir.shards().len(), 1);
}

#[test]
fn corrupt_or_old_storage_is_rebuilt_silently() {
    let dir = TempDir::new();
    let file = dir.write("a.php", ORDER);
    let analyzer = Analyzer::with_defaults();
    // a régi tárolók (JSONL gráf, JSON-cache) az első építéskor törlődnek; a jegyzetek maradnak
    for name in ["index.jsonl", "cache.jsonl", "notes.jsonl"] {
        fs::write(dir.store().join(name), "{}\n").unwrap();
    }
    let mut graph = ProjectGraph::open(dir.store());
    refresh_full(&mut graph, std::slice::from_ref(&file));
    assert!(!dir.store().join("index.jsonl").exists());
    assert!(!dir.store().join("cache.jsonl").exists());
    assert!(dir.store().join("notes.jsonl").exists());
    // sérült manifest, shard, callers.bin — mindegyik után helyes válasz
    for broken in ["index.bin", "callers.bin", "shard"] {
        let target = if broken == "shard" {
            dir.shards().remove(0)
        } else {
            dir.store().join(broken)
        };
        fs::write(&target, b"CGB1\x05\x00\x00\x00garbage").unwrap();
        let mut graph = ProjectGraph::open(dir.store());
        graph
            .refresh(std::slice::from_ref(&file), true, &analyzer)
            .unwrap();
        graph.ensure_callers(&analyzer).unwrap();
        graph.load(std::slice::from_ref(&file), &analyzer).unwrap();
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
