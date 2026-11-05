use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "callgraph-project-cache-{id}-{}-{}",
            std::process::id(),
            {
                static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
                N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            }
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn run(args: &[&Path]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_callgraph"))
        .args(args)
        .output()
        .expect("callgraph runs")
}

#[test]
fn default_cache_is_created_inside_each_project_and_preserves_output() {
    let temp = TempDir::new();
    let first = temp.0.join("first");
    let second = temp.0.join("second");
    for project in [&first, &second] {
        fs::create_dir_all(project).unwrap();
        fs::write(project.join("composer.json"), "{}").unwrap();
        fs::write(
            project.join("source.php"),
            "<?php\nfunction hello(): void {}\n",
        )
        .unwrap();
    }

    let paths = [first.as_path(), second.as_path()];
    let uncached = run(&[Path::new("--no-cache"), paths[0], paths[1]]);
    assert!(uncached.status.success(), "{:?}", uncached.stderr);

    let cached = run(&paths);
    assert!(cached.status.success(), "{:?}", cached.stderr);
    assert_eq!(cached.stdout, uncached.stdout);

    let cached_again = run(&paths);
    assert!(cached_again.status.success(), "{:?}", cached_again.stderr);
    assert_eq!(cached_again.stdout, cached.stdout);

    for project in [&first, &second] {
        let store = project.join(".callgraph");
        assert_eq!(fs::read_to_string(store.join(".gitignore")).unwrap(), "*\n");
        assert!(store.join("cache.jsonl").is_file());
    }
    assert!(!temp.0.join(".callgraph").exists());
    assert!(!temp.0.join(".cache").exists());
}
