//! Locator- és store-tesztek ideiglenes könyvtárakban.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Egyedi ideiglenes könyvtár (kanonizált), drop-kor törlődik.
struct Tmp(PathBuf);

impl Tmp {
    fn new() -> Self {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("callgraph-project-{}-{n}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        Tmp(dir.canonicalize().unwrap())
    }
    fn mkdir(&self, rel: &str) -> PathBuf {
        let p = self.0.join(rel);
        fs::create_dir_all(&p).unwrap();
        p
    }
    fn touch(&self, rel: &str) -> PathBuf {
        let p = self.0.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, "x").unwrap();
        p
    }
}

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn root_of(p: &Project) -> &Path {
    p.root()
}

#[test]
fn finds_nearest_marker_upwards() {
    let t = Tmp::new();
    t.touch("app/composer.json");
    let f = t.touch("app/src/Deep/A.php");
    let p = ProjectLocator::default().locate(&f);
    assert_eq!(root_of(&p), t.0.join("app"));
    assert_eq!(p.id(), "app");
}

#[test]
fn nested_project_wins_over_outer() {
    let t = Tmp::new();
    t.touch("outer/composer.json");
    t.touch("outer/packages/inner/package.json");
    let inner = t.touch("outer/packages/inner/src/a.js");
    let outer = t.touch("outer/src/b.php");
    let loc = ProjectLocator::default();
    assert_eq!(
        root_of(&loc.locate(&inner)),
        t.0.join("outer/packages/inner")
    );
    assert_eq!(root_of(&loc.locate(&outer)), t.0.join("outer"));
}

#[test]
fn git_dir_is_a_marker() {
    let t = Tmp::new();
    t.mkdir("repo/.git");
    let f = t.touch("repo/x/y.php");
    assert_eq!(
        root_of(&ProjectLocator::default().locate(&f)),
        t.0.join("repo")
    );
}

#[test]
fn no_marker_falls_back_to_input_root() {
    let t = Tmp::new();
    let dir = t.mkdir("plain/sub");
    let f = t.touch("plain/sub/a.php");
    let loc = ProjectLocator::default();
    // mappa-bemenet: maga a mappa; fájl-bemenet: a szülő mappája
    assert_eq!(root_of(&loc.locate(&dir)), dir);
    assert_eq!(root_of(&loc.locate(&f)), dir);
}

#[test]
fn groups_files_by_project_in_first_seen_order() {
    let t = Tmp::new();
    t.touch("a/composer.json");
    t.touch("b/package.json");
    let a1 = t.touch("a/1.php");
    let b1 = t.touch("b/1.js");
    let a2 = t.touch("a/sub/2.php");
    let groups = ProjectLocator::default().group([&a1, &b1, &a2]);
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].project.id(), "a");
    assert_eq!(groups[0].files, vec![a1, a2]);
    assert_eq!(groups[1].project.id(), "b");
    assert_eq!(groups[1].files, vec![b1]);
}

#[cfg(unix)]
#[test]
fn symlink_resolves_to_real_project() {
    let t = Tmp::new();
    t.touch("real/composer.json");
    let real = t.touch("real/src/a.php");
    t.mkdir("elsewhere");
    let link = t.0.join("elsewhere/link.php");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let p = ProjectLocator::default().locate(&link);
    assert_eq!(root_of(&p), t.0.join("real"));
}

#[test]
fn alias_comes_from_projects_json() {
    let t = Tmp::new();
    t.touch("proj/composer.json");
    let f = t.touch("proj/a.php");
    let json = format!(r#"{{"gw": {{"path": "{}"}}}}"#, t.0.join("proj").display());
    let loc = ProjectLocator::new(Box::new(ProjectsJsonAliases::parse(&json)));
    let p = loc.locate(&f);
    assert_eq!(p.alias(), Some("gw"));
    assert_eq!(p.id(), "proj");
}

#[test]
fn store_paths_and_ensure_write_only_inside_root() {
    let t = Tmp::new();
    let root = t.mkdir("proj");
    let store = ProjectStore::new(&Project::new(&root));
    assert_eq!(store.dir(), root.join(".callgraph"));
    assert_eq!(store.cache_path(), root.join(".callgraph/cache.jsonl"));
    assert_eq!(store.index_path(), root.join(".callgraph/index.jsonl"));
    assert_eq!(store.config_path(), root.join(".callgraph/config.json"));
    assert!(!store.dir().exists(), "az út-lekérdezés nem ír");

    store.ensure().unwrap();
    assert_eq!(
        fs::read_to_string(store.dir().join(".gitignore")).unwrap(),
        "*\n"
    );
    store.ensure().unwrap(); // idempotens
}

#[test]
fn ensure_keeps_existing_gitignore() {
    let t = Tmp::new();
    let root = t.mkdir("proj");
    let store = ProjectStore::at_root(&root);
    store.ensure().unwrap();
    fs::write(store.dir().join(".gitignore"), "*\n!keep\n").unwrap();
    store.ensure().unwrap();
    assert_eq!(
        fs::read_to_string(store.dir().join(".gitignore")).unwrap(),
        "*\n!keep\n"
    );
}

#[cfg(unix)]
#[test]
fn ensure_refuses_store_dir_symlinked_outside_root() {
    let t = Tmp::new();
    let root = t.mkdir("proj");
    let outside = t.mkdir("outside");
    std::os::unix::fs::symlink(&outside, root.join(".callgraph")).unwrap();
    assert!(ProjectStore::at_root(&root).ensure().is_err());
    assert!(!outside.join(".gitignore").exists());
}
