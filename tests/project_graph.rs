//! A perzisztens projekt-gráf (`.callgraph/`: `index.bin`, `graph/*.bin`, `callers.bin`): a gráfból kiszolgált lekérdezések kimenete bájtra azonos a
//! fájlonkénti (bejárásos) úttal (`CALLGRAPH_NO_GRAPH=1`) — hidegen és melegen, több nyelvű projekten; egy
//! szerkesztés a következő lekérdezésben látszik; a sérült / régi gráf-fájl csendben újraépül.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

struct Project(PathBuf);

/// PHP (osztályok, interfész nélkül), JS (modul-függvények), Rust (impl + szabad függvény) és Swift.
const FILES: [(&str, &str); 7] = [
    (
        "src/Repo.php",
        "<?php\nnamespace App;\n\nclass Repo\n{\n    const LIMIT = 5;\n\n    public function find(int $id): array\n    {\n        return [];\n    }\n}\n",
    ),
    (
        "src/Lookup.php",
        "<?php\nnamespace App;\n\nclass Lookup\n{\n    private Repo $repo;\n\n    public function run(int $id): array\n    {\n        $this->repo->find($id);\n        $this->repo->find(1);\n        $this->repo->find($id);\n        $this->repo->find(2);\n        return $this->repo->find(3);\n    }\n}\n",
    ),
    (
        "src/Admin.php",
        "<?php\nnamespace App;\n\nclass Admin\n{\n    private Lookup $lookup;\n\n    public function show(int $id): array\n    {\n        return $this->lookup->run($id);\n    }\n\n    public function other(): string\n    {\n        $limit = Repo::LIMIT;\n        return \"needle message {$limit}\";\n    }\n}\n",
    ),
    (
        "web/repo.js",
        "export function find(id) {\n  return 'needle js ' + id;\n}\n",
    ),
    (
        "web/app.js",
        "import { find } from './repo.js';\n\nexport function load(id) {\n  return find(id);\n}\n\nexport function page() {\n  return load(1);\n}\n",
    ),
    (
        "rs/lib.rs",
        "pub struct Store;\n\nconst LIMIT: u32 = 3;\n\nimpl Store {\n    pub fn find(&self) -> u32 {\n        LIMIT\n    }\n}\n\npub fn run(store: &Store) -> u32 {\n    store.find()\n}\n\npub fn top(store: &Store) -> u32 {\n    run(store)\n}\n",
    ),
    (
        "sw/App.swift",
        "class Store {\n    func find() -> Int { return 1 }\n}\n\nclass Screen {\n    let store = Store()\n    func show() -> Int {\n        return store.find()\n    }\n}\n",
    ),
];

/// A vizsgált lekérdezések (a `show` argumentumai, a `--project` nélkül).
const QUERIES: [&[&str]; 16] = [
    &["show", ".", "--grep", "find"],
    &["show", ".", "--grep", "find", "--depth", "3"],
    &["show", ".", "--grep", "find", "--depth", "3", "--guess"],
    &["show", ".", "--grep", "Repo::find", "--depth", "2"],
    &["show", "src", "--grep", "Repo::find", "--depth", "3"],
    &["show", "src/Lookup.php", "--grep", "find", "--depth", "2"],
    &["show", ".", "--grep", "find", "CALLGRAPH_OUT_LIMIT=8"],
    &[
        "show",
        ".",
        "--grep",
        "nothing_matches_this",
        "--depth",
        "2",
    ],
    &["show", ".", "--strings", "needle"],
    &["show", ".", "--const", "LIMIT"],
    &["show", "src", "--const", "LIMIT"],
    &["show", "src/Admin.php"],
    &["show", "src/Admin.php::other", "--body"],
    &["show", "web"],
    &["show", "src", "--format", "json"],
    &["show", "web", "--format", "json"],
];

impl Project {
    fn new() -> Self {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("callgraph-pgraph-{}-{n}-{id}", std::process::id()));
        for (path, text) in FILES {
            let path = root.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
        Self(root.canonicalize().unwrap())
    }

    /// A lekérdezés kimenete; `graph = false`: a gráf nélküli, fájlonkénti út.
    fn run(&self, args: &[&str], graph: bool) -> String {
        let mut command = Command::new(env!("CARGO_BIN_EXE_callgraph"));
        command
            .args(args)
            .arg("--project")
            .arg(&self.0)
            .env("CALLGRAPH_LANG", "en")
            .env_remove("CALLGRAPH_NO_GRAPH");
        if !graph {
            command.env("CALLGRAPH_NO_GRAPH", "1");
        }
        let out = command.output().expect("callgraph runs");
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }

    fn store(&self) -> PathBuf {
        self.0.join(".callgraph")
    }

    fn shard_count(&self) -> usize {
        fs::read_dir(self.store().join("graph"))
            .map(|dir| dir.count())
            .unwrap_or_default()
    }

    /// Minden lekérdezés: a gráfos kimenet azonos a gráf nélkülivel.
    fn assert_same(&self, when: &str) {
        for args in QUERIES {
            let walked = self.run(args, false);
            let graphed = self.run(args, true);
            assert_eq!(graphed, walked, "{when}: {args:?}");
        }
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn graph_backed_queries_match_the_file_walk_cold_and_warm() {
    let project = Project::new();
    // hideg gráf: minden lekérdezés előtt nincs gráf-fájl
    for args in QUERIES {
        let _ = fs::remove_dir_all(project.store());
        let graphed = project.run(args, true);
        assert_eq!(graphed, project.run(args, false), "cold: {args:?}");
    }
    project.assert_same("warm");
    let manifest = fs::read(project.store().join("index.bin")).unwrap();
    assert!(manifest.starts_with(b"CGB1"));
    assert!(project.store().join("callers.bin").is_file());
    assert_eq!(project.shard_count(), FILES.len());
    // a régi JSON-tárolók nem jönnek létre
    assert!(!project.store().join("index.jsonl").exists());
    assert!(!project.store().join("cache.jsonl").exists());
    // a kimenet tényleg tartalmaz gráfból jövő részeket (nem üres egyezés)
    let deep = project.run(QUERIES[1], true);
    assert!(deep.contains("Admin::show()"), "{deep}");
    assert!(
        deep.contains("L10, L12 │ $this->repo->find($id);"),
        "{deep}"
    );
}

#[test]
fn an_edited_file_shows_up_in_the_next_query() {
    let project = Project::new();
    let args: &[&str] = &["show", "src", "--grep", "Repo::find", "--depth", "3"];
    let before = project.run(args, true);
    assert!(!before.contains("Report::build()"), "{before}");
    fs::write(
        project.0.join("src/Report.php"),
        "<?php\nnamespace App;\n\nclass Report\n{\n    private Admin $admin;\n\n    public function build(): array\n    {\n        return $this->admin->show(1);\n    }\n}\n",
    )
    .unwrap();
    let after = project.run(args, true);
    assert!(after.contains("Report::build()"), "{after}");
    assert_eq!(project.shard_count(), FILES.len() + 1);
    assert_eq!(after, project.run(args, false));
    // törölt fájl: kikerül
    fs::remove_file(project.0.join("src/Report.php")).unwrap();
    let removed = project.run(args, true);
    assert_eq!(removed, before);
    assert_eq!(project.shard_count(), FILES.len());
}

#[test]
fn a_corrupt_or_old_graph_file_is_rebuilt_silently() {
    let project = Project::new();
    let args: &[&str] = &["show", ".", "--grep", "find", "--depth", "3"];
    let expected = project.run(args, false);
    // a régi tárolók (v0.2-es JSON-cache, a JSONL gráf) csendben eltűnnek
    fs::create_dir_all(project.store()).unwrap();
    for name in ["index.jsonl", "cache.jsonl"] {
        fs::write(project.store().join(name), "{\"path\":\"x\"}\n").unwrap();
    }
    assert_eq!(project.run(args, true), expected);
    assert!(!project.store().join("index.jsonl").exists());
    assert!(!project.store().join("cache.jsonl").exists());
    // sérült manifest, callers.bin és shard: mindegyik után azonos válasz
    for name in ["index.bin", "callers.bin", "shard"] {
        let target = if name == "shard" {
            fs::read_dir(project.store().join("graph"))
                .unwrap()
                .next()
                .unwrap()
                .unwrap()
                .path()
        } else {
            project.store().join(name)
        };
        fs::write(&target, b"garbage").unwrap();
        assert_eq!(project.run(args, true), expected, "{name}");
        assert_eq!(
            project.run(QUERIES[0], true),
            project.run(QUERIES[0], false),
            "{name}"
        );
    }
}
