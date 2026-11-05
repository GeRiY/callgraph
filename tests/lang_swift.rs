//! A Swift frontend: regisztráció, bejárás, és a `show` outline / stats kimenete egy SwiftUI-mintán.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use callgraph::io::FileWalker;
use callgraph::lang::{LanguageRegistry, LanguageRole};

struct TempDir(PathBuf);

impl TempDir {
    /// Ideiglenes projekt a `golden/samples/swift/` másolatával.
    fn with_samples() -> Self {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("callgraph-swift-{id}"));
        fs::create_dir_all(&path).unwrap();
        let samples = Path::new(env!("CARGO_MANIFEST_DIR")).join("golden/samples/swift");
        for entry in fs::read_dir(samples).unwrap() {
            let entry = entry.unwrap();
            fs::copy(entry.path(), path.join(entry.file_name())).unwrap();
        }
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn show(file: &Path, format: &str) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_callgraph"))
        .arg("show")
        .arg(file)
        .args(["--format", format])
        .env("CALLGRAPH_LANG", "en")
        .output()
        .expect("callgraph runs");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn registry_routes_swift_to_frontend() {
    let r = LanguageRegistry::with_defaults();
    let l = r.for_path(Path::new("App/ContentView.swift")).unwrap();
    assert_eq!((l.id(), l.role()), ("swift", LanguageRole::Frontend));
    assert!(r.for_path(Path::new("App/ContentView.SWIFT")).is_none());
}

#[test]
fn walker_visits_swift_files() {
    let project = TempDir::with_samples();
    let r = LanguageRegistry::with_defaults();
    let files = FileWalker::for_registry(&r).collect(&[project.0.clone()], &mut |_| {});
    let names: Vec<_> = files
        .iter()
        .filter_map(|f| f.file_name()?.to_str().map(str::to_string))
        .collect();
    for name in ["ContentView.swift", "Todo.swift", "TodoStore.swift"] {
        assert!(names.iter().any(|n| n == name), "{name}: {names:?}");
    }
}

#[test]
fn outline_shows_swiftui_component() {
    let project = TempDir::with_samples();
    let text = show(&project.0.join("ContentView.swift"), "outline");
    for expected in [
        "component ContentView implements View  L4-40",
        "@State String $newTitle",
        "@EnvironmentObject AppSettings $settings",
        "  body()  L11-32",
        "    swiftui_view      Button, HStack, List, NavigationStack, TextField, VStack",
        "    component_use     TodoRow",
        "    internal_helper   addTodo",
        "💬 [TODO] TODO: validate the title length before adding",
    ] {
        assert!(text.contains(expected), "missing {expected:?} in:\n{text}");
    }
    assert!(
        text.contains(".padding") && text.contains(".onAppear"),
        "{text}"
    );
}

#[test]
fn outline_shows_types_extension_and_protocol() {
    let project = TempDir::with_samples();
    let todo = show(&project.0.join("Todo.swift"), "outline");
    assert!(
        todo.contains("enum Priority extends Int implements CaseIterable"),
        "{todo}"
    );
    assert!(todo.contains("low = 1 [enum case]"), "{todo}");
    assert!(
        todo.contains("extension Todo@L24 extends Todo  L24-35"),
        "{todo}"
    );
    let store = show(&project.0.join("TodoStore.swift"), "outline");
    assert!(store.contains("protocol TodoStore  L4-8"), "{store}");
    assert!(store.contains("fetch()  L6-6 [abstract]"), "{store}");
    assert!(store.contains("actor SyncService  L10-16"), "{store}");
    assert!(store.contains("module TodoStore.swift"), "{store}");
}

#[test]
fn stats_count_swift_categories() {
    let project = TempDir::with_samples();
    let text = show(&project.0.join("TodoViewModel.swift"), "stats");
    for expected in [
        "containers: 1 (class)",
        "properties: 3",
        "constants: 1",
        "methods: 6",
        "native_swift",
        "internal_helper",
    ] {
        assert!(text.contains(expected), "missing {expected:?} in:\n{text}");
    }
}
