use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use callgraph::cli::{CliArgs, build_command};

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        // a macOS órája csak µs-os: párhuzamos tesztek ugyanazt a nevet kapnák (és egymás alól törölnék) — ezért számláló is
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("callgraph-index-{}-{n}-{id}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn run(project: &TempDir, depth: usize) -> Vec<serde_json::Value> {
    let args = CliArgs::parse([
        "callers".into(),
        "Order::saveTranslations".into(),
        "--depth".into(),
        depth.to_string(),
        "--project".into(),
        project.0.to_string_lossy().into_owned(),
    ])
    .unwrap();
    let command = build_command(args).unwrap();
    let mut out = Vec::new();
    let mut err = Vec::new();
    assert_eq!(command.run(&mut out, &mut err).unwrap(), 0, "{err:?}");
    String::from_utf8(out)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn creates_project_index_and_updates_changed_file() {
    let project = TempDir::new();
    let source = project.0.join("graph.php");
    fs::write(
        &source,
        "<?php\nnamespace App;\nclass Order { function saveTranslations() {} }\nclass Caller { function first() { Order::saveTranslations(); } }",
    )
    .unwrap();
    let first = run(&project, 1);
    assert_eq!(first.len(), 1);
    assert!(project.0.join(".callgraph/index.bin").exists());
    fs::write(
        &source,
        "<?php\nnamespace App;\nclass Order { function saveTranslations() {} }\nclass Caller { function second() { Order::saveTranslations(); } }",
    )
    .unwrap();
    let second = run(&project, 1);
    assert_eq!(second[0]["caller"], "second");
}

/// Durva mtime-felbontású fájlrendszeren (pl. Docker bind mount) két gyors írás azonos mtime-ot kaphat:
/// a méret-eltérésből akkor is észre kell venni a változást.
#[test]
fn reindexes_changed_file_with_identical_mtime() {
    let project = TempDir::new();
    let source = project.0.join("graph.php");
    fs::write(
        &source,
        "<?php\nnamespace App;\nclass Order { function saveTranslations() {} }\nclass Caller { function a() { Order::saveTranslations(); } }",
    )
    .unwrap();
    let mtime = fs::metadata(&source).unwrap().modified().unwrap();
    assert_eq!(run(&project, 1)[0]["caller"], "a");
    fs::write(
        &source,
        "<?php\nnamespace App;\nclass Order { function saveTranslations() {} }\nclass Caller { function longer() { Order::saveTranslations(); } }",
    )
    .unwrap();
    fs::File::options()
        .write(true)
        .open(&source)
        .unwrap()
        .set_modified(mtime)
        .unwrap();
    assert_eq!(fs::metadata(&source).unwrap().modified().unwrap(), mtime);
    assert_eq!(run(&project, 1)[0]["caller"], "longer");
}

#[test]
fn depth_follows_callers_inside_the_project() {
    let project = TempDir::new();
    fs::write(
        project.0.join("graph.php"),
        "<?php\nnamespace App;\nclass Order { function saveTranslations() {} }\nclass Caller { function inner() { Order::saveTranslations(); } function outer() { $this->inner(); } }",
    )
    .unwrap();
    let rows = run(&project, 2);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["level"], 1);
    assert_eq!(rows[0]["caller"], "inner");
    assert_eq!(rows[1]["level"], 2);
    assert_eq!(rows[1]["caller"], "outer");
}
