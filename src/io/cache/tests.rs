use super::*;
use crate::error::CallgraphError;
use crate::io::CacheStore;
use crate::model::Imports;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

struct TempDir(PathBuf);
impl TempDir {
    fn new() -> Self {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!("callgraph-cache-{}-{n}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

// Minden OS-en abszolút cache-kulcs: Windowson a `/a.php` nem abszolút, ezért `C:` előtagot kap.
fn abs(p: &str) -> PathBuf {
    PathBuf::from(if cfg!(windows) {
        format!("C:{p}")
    } else {
        p.to_owned()
    })
}

fn graph(name: &str) -> crate::model::FileGraph {
    crate::model::FileGraph {
        file: name.to_owned(),
        framework: None,
        namespace: Some("App".into()),
        imports: Imports::default(),
        containers: vec![],
        properties: vec![],
        constants: vec![],
        methods: vec![],
        calls: vec![],
        comments: vec![],
        control_flow: vec![],
        strings: vec![],
        writes: vec![],
        module: None,
        sfc: None,
        binary: false,
        parse_errors: Vec::new(),
    }
}

#[test]
fn roundtrip_and_mtime_miss() {
    let temp = TempDir::new();
    let file = temp.path("cache.jsonl");
    let mut store = JsonlCacheStore::new(file.clone());
    store.put(&abs("/a.php"), 10, &graph("a")).unwrap();
    assert_eq!(store.get(&abs("/a.php"), 10), Some(graph("a")));
    assert!(store.get(&abs("/a.php"), 11).is_none());
    assert_eq!(JsonlCacheStore::new(file).len(), 1);
}

#[test]
fn language_id_is_part_of_cache_version() {
    let temp = TempDir::new();
    let file = temp.path("cache.jsonl");
    let mut store = JsonlCacheStore::new(file.clone());
    store
        .put_for_language(&abs("/a.php"), 1, &graph("php"), "php")
        .unwrap();
    store
        .put_for_language(&abs("/a.js"), 1, &graph("js"), "js")
        .unwrap();
    assert_eq!(
        store.get_for_language(&abs("/a.php"), 1, "php"),
        Some(graph("php"))
    );
    assert!(store.get_for_language(&abs("/a.php"), 1, "js").is_none());
    let text = fs::read_to_string(file).unwrap();
    assert!(text.contains("+lang=php") && text.contains("+lang=js"));
}

#[test]
fn compact_is_sorted_and_deduplicated() {
    let temp = TempDir::new();
    let file = temp.path("cache.jsonl");
    let mut store = JsonlCacheStore::new(file.clone());
    store.put(&abs("/b.php"), 1, &graph("b1")).unwrap();
    store.put(&abs("/a.php"), 1, &graph("a")).unwrap();
    store.put(&abs("/b.php"), 2, &graph("b2")).unwrap();
    assert_eq!(
        store.compact().unwrap(),
        CompactStats {
            kept: 2,
            dropped: 1
        }
    );
    let lines: Vec<_> = fs::read_to_string(file)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect();
    assert!(lines[0].contains("/a.php"));
    assert!(lines[1].contains("/b.php") && lines[1].contains("b2"));
}

fn lines(file: &Path) -> usize {
    fs::read_to_string(file).unwrap().lines().count()
}

#[test]
fn miss_on_empty_and_missing_file() {
    let t = TempDir::new();
    let s = JsonlCacheStore::new(t.path("c.jsonl"));
    assert!(s.is_empty());
    assert!(s.get(&abs("/a.php"), 1).is_none());
}

#[test]
fn put_then_get_roundtrip_and_persistence() {
    let t = TempDir::new();
    let f = t.path("c.jsonl");
    let mut s = JsonlCacheStore::new(f.clone());
    s.put(&abs("/a.php"), 10, &graph("a")).unwrap();
    assert_eq!(s.get(&abs("/a.php"), 10), Some(graph("a")));
    let s2 = JsonlCacheStore::new(f);
    assert_eq!(s2.get(&abs("/a.php"), 10), Some(graph("a")));
    assert_eq!(s2.len(), 1);
}

#[test]
fn mtime_mismatch_is_a_miss() {
    let t = TempDir::new();
    let mut s = JsonlCacheStore::new(t.path("c.jsonl"));
    s.put(&abs("/a.php"), 10, &graph("a")).unwrap();
    assert!(s.get(&abs("/a.php"), 11).is_none());
    assert!(s.get(&abs("/b.php"), 10).is_none());
}

#[test]
fn version_mismatch_is_a_miss() {
    let t = TempDir::new();
    let f = t.path("c.jsonl");
    let mut old = JsonlCacheStore::with_version(f.clone(), "0.0.1+s1".into());
    old.put(&abs("/a.php"), 10, &graph("a")).unwrap();
    let new = JsonlCacheStore::with_version(f.clone(), "0.0.1+s2".into());
    assert!(new.get(&abs("/a.php"), 10).is_none());
    let old2 = JsonlCacheStore::with_version(f, "0.0.1+s1".into());
    assert!(old2.get(&abs("/a.php"), 10).is_some());
}

#[test]
fn latest_valid_row_wins_on_load() {
    let t = TempDir::new();
    let f = t.path("c.jsonl");
    let mut s = JsonlCacheStore::new(f.clone());
    s.put(&abs("/a.php"), 1, &graph("v1")).unwrap();
    s.put(&abs("/a.php"), 2, &graph("v2")).unwrap();
    assert_eq!(lines(&f), 2);
    let s2 = JsonlCacheStore::new(f);
    assert!(s2.get(&abs("/a.php"), 1).is_none());
    assert_eq!(s2.get(&abs("/a.php"), 2), Some(graph("v2")));
}

#[test]
fn newer_row_with_other_version_does_not_shadow_valid_one() {
    let t = TempDir::new();
    let f = t.path("c.jsonl");
    let mut a = JsonlCacheStore::with_version(f.clone(), "A".into());
    a.put(&abs("/a.php"), 1, &graph("a")).unwrap();
    let mut b = JsonlCacheStore::with_version(f.clone(), "B".into());
    b.put(&abs("/a.php"), 1, &graph("b")).unwrap();
    let a2 = JsonlCacheStore::with_version(f, "A".into());
    assert_eq!(a2.get(&abs("/a.php"), 1), Some(graph("a")));
}

#[test]
fn relative_path_is_rejected() {
    let t = TempDir::new();
    let mut s = JsonlCacheStore::new(t.path("c.jsonl"));
    assert!(matches!(
        s.put(Path::new("rel/a.php"), 1, &graph("a")),
        Err(CallgraphError::Usage(_))
    ));
}

#[test]
fn corrupt_and_truncated_lines_are_skipped() {
    let t = TempDir::new();
    let f = t.path("c.jsonl");
    let mut s = JsonlCacheStore::new(f.clone());
    s.put(&abs("/a.php"), 1, &graph("a")).unwrap();
    let mut fh = OpenOptions::new().append(true).open(&f).unwrap();
    fh.write_all(b"nem json\n\xff\xfe\n{\"path\":\"/b.php\",\"mtime\":1,\"ver")
        .unwrap();
    drop(fh);
    let mut s2 = JsonlCacheStore::new(f.clone());
    assert_eq!(s2.get(&abs("/a.php"), 1), Some(graph("a")));
    assert!(s2.get(&abs("/b.php"), 1).is_none());
    s2.put(&abs("/c.php"), 3, &graph("c")).unwrap();
    let s3 = JsonlCacheStore::new(f);
    assert_eq!(s3.get(&abs("/c.php"), 3), Some(graph("c")));
    assert_eq!(s3.get(&abs("/a.php"), 1), Some(graph("a")));
}

#[test]
fn compact_drops_duplicates_stale_and_corrupt() {
    let t = TempDir::new();
    let f = t.path("c.jsonl");
    JsonlCacheStore::with_version(f.clone(), "OLD".into())
        .put(&abs("/old.php"), 1, &graph("old"))
        .unwrap();
    let mut s = JsonlCacheStore::new(f.clone());
    s.put(&abs("/a.php"), 1, &graph("a1")).unwrap();
    s.put(&abs("/a.php"), 2, &graph("a2")).unwrap();
    s.put(&abs("/b.php"), 5, &graph("b")).unwrap();
    OpenOptions::new()
        .append(true)
        .open(&f)
        .unwrap()
        .write_all(b"szemet\n")
        .unwrap();
    assert_eq!(
        s.compact().unwrap(),
        CompactStats {
            kept: 2,
            dropped: 3
        }
    );
    assert_eq!(lines(&f), 2);
    assert_eq!(s.get(&abs("/a.php"), 2), Some(graph("a2")));
    assert_eq!(s.get(&abs("/b.php"), 5), Some(graph("b")));
    assert_eq!(
        s.compact().unwrap(),
        CompactStats {
            kept: 2,
            dropped: 0
        }
    );
    let s2 = JsonlCacheStore::new(f);
    assert_eq!(s2.len(), 2);
    assert_eq!(s2.get(&abs("/a.php"), 2), Some(graph("a2")));
}

#[test]
fn compact_on_missing_file_is_noop() {
    let t = TempDir::new();
    let mut s = JsonlCacheStore::new(t.path("nincs.jsonl"));
    assert_eq!(s.compact().unwrap(), CompactStats::default());
}

#[test]
fn prune_removes_deleted_sources_only() {
    let t = TempDir::new();
    let alive = t.path("alive.php");
    let dead = t.path("dead.php");
    fs::write(&alive, "<?php").unwrap();
    fs::write(&dead, "<?php").unwrap();
    let f = t.path("c.jsonl");
    let mut s = JsonlCacheStore::new(f.clone());
    s.put(&alive, 1, &graph("alive")).unwrap();
    s.put(&dead, 1, &graph("dead")).unwrap();
    fs::remove_file(&dead).unwrap();
    assert_eq!(
        s.prune().unwrap(),
        CompactStats {
            kept: 1,
            dropped: 1
        }
    );
    assert_eq!(s.get(&alive, 1), Some(graph("alive")));
    assert!(s.get(&dead, 1).is_none());
    assert_eq!(lines(&f), 1);
}

#[test]
fn put_after_compact_appends_to_new_file() {
    let t = TempDir::new();
    let f = t.path("c.jsonl");
    let mut s = JsonlCacheStore::new(f.clone());
    s.put(&abs("/a.php"), 1, &graph("a")).unwrap();
    s.compact().unwrap();
    s.put(&abs("/b.php"), 2, &graph("b")).unwrap();
    let s2 = JsonlCacheStore::new(f);
    assert_eq!(s2.len(), 2);
}

#[test]
fn creates_missing_parent_directory() {
    let t = TempDir::new();
    let f = t.path("sub/dir/c.jsonl");
    let mut s = JsonlCacheStore::new(f.clone());
    s.put(&abs("/a.php"), 1, &graph("a")).unwrap();
    assert!(f.exists());
}

#[test]
fn reload_sees_other_writers() {
    let t = TempDir::new();
    let f = t.path("c.jsonl");
    let mut a = JsonlCacheStore::new(f.clone());
    let mut b = JsonlCacheStore::new(f);
    a.put(&abs("/a.php"), 1, &graph("a")).unwrap();
    assert!(b.get(&abs("/a.php"), 1).is_none());
    b.reload().unwrap();
    assert_eq!(b.get(&abs("/a.php"), 1), Some(graph("a")));
}

#[test]
fn concurrent_writers_and_compactor_lose_nothing() {
    let t = TempDir::new();
    let f = t.path("c.jsonl");
    let threads = 6;
    let per = 40;
    let handles: Vec<_> = (0..threads)
        .map(|th| {
            let f = f.clone();
            std::thread::spawn(move || {
                let mut s = JsonlCacheStore::new(f);
                for i in 0..per {
                    let p = abs(&format!("/t{th}/f{i}.php"));
                    s.put(&p, i as u64, &graph(&format!("{th}-{i}"))).unwrap();
                    if th == 0 && i % 10 == 0 {
                        s.compact().unwrap();
                    }
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    let s = JsonlCacheStore::new(f.clone());
    assert_eq!(s.len(), threads * per);
    for th in 0..threads {
        for i in 0..per {
            let p = abs(&format!("/t{th}/f{i}.php"));
            assert_eq!(s.get(&p, i as u64), Some(graph(&format!("{th}-{i}"))));
        }
    }
    for line in fs::read_to_string(&f).unwrap().lines() {
        serde_json::from_str::<RowIn>(line).unwrap();
    }
}

#[test]
fn tool_version_contains_crate_version_and_schema() {
    let v = tool_version();
    assert!(v.starts_with(env!("CARGO_PKG_VERSION")));
    assert!(v.contains(&format!("+s{SCHEMA}+")));
    assert!(v.ends_with(env!("CALLGRAPH_SRC_HASH")));
}
