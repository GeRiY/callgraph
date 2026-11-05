//! A projekt-gráf tárolása és frissítése: meleg betöltés elemzés nélkül, szerkesztés → frissül, törlés → kikerül,
//! sérült / régi formátumú fájl → csendes újraépítés, és a folyamaton belüli közös példány.

use std::fs;
use std::path::{Path, PathBuf};

use super::{ProjectGraph, lock, shared};
use crate::analysis::Analyzer;
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

    fn index(&self) -> PathBuf {
        self.0.join(".callgraph/index.jsonl")
    }

    fn write(&self, name: &str, text: &str) -> PathBuf {
        let path = self.0.join(name);
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

/// Teljes frissítés az alapértelmezett nyelvekkel, cache nélkül, majd mentés.
fn sync_full(graph: &mut ProjectGraph, files: &[PathBuf]) {
    graph
        .sync(files, true, &Analyzer::with_defaults(), &mut None)
        .unwrap();
    graph.save().unwrap();
}

fn callers_of(graph: &mut ProjectGraph, pattern: &str) -> Vec<String> {
    graph.ensure_callers();
    graph
        .caller_index()
        .callers(pattern, 1)
        .unwrap()
        .into_iter()
        .map(|row| row.caller)
        .collect()
}

#[test]
fn warm_load_needs_no_source_parsing() {
    let dir = TempDir::new();
    let file = dir.write("a.php", ORDER);
    let mut graph = ProjectGraph::load(dir.index());
    sync_full(&mut graph, std::slice::from_ref(&file));
    let first = graph.graph(&key(&file)).cloned().unwrap();
    let header = fs::read_to_string(dir.index()).unwrap();
    assert!(
        header.starts_with("{\"format\":\"callgraph-project-graph\",\"version\":1,"),
        "{header}"
    );
    // nyelvek nélküli elemző: bármilyen forrás-elemzés hibát adna — a meleg betöltés mégis kiszolgál
    let mut warm = ProjectGraph::load(dir.index());
    let none = Analyzer::new(LanguageRegistry::new());
    warm.sync(std::slice::from_ref(&file), true, &none, &mut None)
        .unwrap();
    assert_eq!(warm.graph(&key(&file)), Some(&first));
    assert_eq!(
        warm.snippets(&key(&file)).unwrap(),
        &[(
            4,
            "class Caller { function first() { Order::save(); } }".to_owned()
        )]
    );
    assert_eq!(callers_of(&mut warm, "Order::save"), vec!["first"]);
    assert_eq!(
        warm.method_range(&key(&file), Some("Caller"), "first"),
        Some((4, 4))
    );
}

#[test]
fn edited_file_is_reanalyzed_and_deleted_file_dropped() {
    let dir = TempDir::new();
    let file = dir.write("a.php", ORDER);
    let other = dir.write("b.php", "<?php\nfunction helper() {}\n");
    let mut graph = ProjectGraph::load(dir.index());
    sync_full(&mut graph, &[file.clone(), other.clone()]);
    assert_eq!(callers_of(&mut graph, "Order::save"), vec!["first"]);
    fs::write(&file, ORDER.replace("first", "secondName")).unwrap();
    fs::remove_file(&other).unwrap();
    // új betöltés (mint egy új CLI-hívás), és a memóriában maradt példány (mint az MCP) is frissül
    let mut reloaded = ProjectGraph::load(dir.index());
    for graph in [&mut graph, &mut reloaded] {
        sync_full(graph, std::slice::from_ref(&file));
        assert_eq!(callers_of(graph, "Order::save"), vec!["secondName"]);
        assert!(graph.graph(&key(&other)).is_none());
    }
    let stored = fs::read_to_string(dir.index()).unwrap();
    assert!(!stored.contains("b.php"), "{stored}");
}

#[test]
fn corrupt_or_old_format_file_is_rebuilt_silently() {
    let dir = TempDir::new();
    let file = dir.write("a.php", ORDER);
    let analyzer = Analyzer::with_defaults();
    // régi (fejléc nélküli) hívó-index, sérült sor, más build fejléce
    let old = format!(
        "{{\"path\":\"{}\",\"mtime\":1,\"size\":1,\"definitions\":[],\"graph\":{{}}}}\n",
        key(&file)
    );
    let foreign =
        "{\"format\":\"callgraph-project-graph\",\"version\":1,\"tool\":\"0.0.0-other\"}\n";
    for text in [old.as_str(), "nem json\n{{{", foreign, ""] {
        fs::write(dir.index(), text).unwrap();
        let mut graph = ProjectGraph::load(dir.index());
        graph
            .sync(std::slice::from_ref(&file), true, &analyzer, &mut None)
            .unwrap();
        assert_eq!(
            callers_of(&mut graph, "Order::save"),
            vec!["first"],
            "{text}"
        );
        graph.save().unwrap();
        let stored = fs::read_to_string(dir.index()).unwrap();
        assert!(stored.starts_with("{\"format\":"), "{stored}");
    }
    // érvényes fejléc, de sérült rekord-törzs: az a fájl újraelemződik
    let stored = fs::read_to_string(dir.index()).unwrap();
    let (header, _) = stored.split_once('\n').unwrap();
    let stamp = super::record::FileStamp::of(&file);
    fs::write(
        dir.index(),
        format!(
            "{header}\n{{\"path\":\"{}\",\"mtime\":{},\"size\":{},\"graph\":{{\"broken\":true}}}}\n",
            key(&file),
            stamp.mtime,
            stamp.size
        ),
    )
    .unwrap();
    let mut graph = ProjectGraph::load(dir.index());
    graph
        .sync(std::slice::from_ref(&file), true, &analyzer, &mut None)
        .unwrap();
    assert_eq!(callers_of(&mut graph, "Order::save"), vec!["first"]);
}

#[test]
fn the_process_keeps_one_graph_per_project() {
    let dir = TempDir::new();
    let file = dir.write("a.php", ORDER);
    let first = shared(&dir.0).unwrap();
    sync_full(&mut lock(&first), std::slice::from_ref(&file));
    // ugyanaz a gyökér (más alakban is) → ugyanaz a példány, lemezről nem töltődik újra
    let again = shared(&dir.0.join(".")).unwrap();
    assert!(std::sync::Arc::ptr_eq(&first, &again));
    assert!(lock(&again).graph(&key(&file)).is_some());
}
