//! Kotlin Compose Navigation: a `navigate(..)` hívás párosítása a másik fájlban deklarált `composable(..)` útvonallal.

use std::fs;
use std::path::PathBuf;

use callgraph::cli::{CliArgs, build_command};

/// Ideiglenes projekt-mappa, a teszt végén törlődik.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("callgraph-kt-nav-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("temp dir");
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A `callers <minta>` parancs JSONL-sorai a projekten.
fn callers(project: &TempDir, pattern: &str) -> Vec<serde_json::Value> {
    let args = CliArgs::parse([
        "callers".to_owned(),
        pattern.to_owned(),
        "--project".to_owned(),
        project.0.to_string_lossy().into_owned(),
    ])
    .expect("args");
    let command = build_command(args).expect("command");
    let (mut out, mut err) = (Vec::new(), Vec::new());
    assert_eq!(command.run(&mut out, &mut err).expect("run"), 0, "{err:?}");
    String::from_utf8(out)
        .expect("utf8")
        .lines()
        .map(|line| serde_json::from_str(line).expect("json"))
        .collect()
}

fn write(project: &TempDir, name: &str, text: &str) {
    fs::write(project.0.join(name), text).expect("write");
}

#[test]
fn navigate_literal_route_resolves_across_files() {
    let project = TempDir::new("literal");
    write(
        &project,
        "AppNavHost.kt",
        "@Composable\nfun AppNavHost(nav: NavHostController) {\n    NavHost(nav, startDestination = \"list\") {\n        composable(\"list\") { ListScreen() }\n        composable(\"detail/{id}\") { DetailScreen() }\n    }\n}\n",
    );
    write(
        &project,
        "ListScreen.kt",
        "@Composable\nfun ListScreen(nav: NavHostController) {\n    val id = 1\n    nav.navigate(\"detail/$id\")\n}\n",
    );
    write(
        &project,
        "DetailScreen.kt",
        "@Composable\nfun DetailScreen() {}\n",
    );
    let rows = callers(&project, "DetailScreen");
    assert!(rows.iter().any(|r| r["caller"] == "ListScreen"), "{rows:?}");
}

#[test]
fn swift_cross_file_chain_caller_shows_at_level_one() {
    let project = TempDir::new("swift-chain");
    write(
        &project,
        "Screen.swift",
        "final class Screen {\n    let store: Store\n    init(store: Store) { self.store = store }\n    func go() { self.store.session.user.load() }\n}\n",
    );
    write(
        &project,
        "Store.swift",
        "final class Store {\n    @Published var session: Session?\n}\n",
    );
    write(
        &project,
        "Session.swift",
        "final class Session {\n    var user: User!\n}\n",
    );
    write(
        &project,
        "User.swift",
        "final class User {\n    func load() {}\n}\n",
    );
    let rows = callers(&project, "User::load");
    assert!(rows.iter().any(|r| r["caller"] == "go"), "{rows:?}");
}

#[test]
fn navigate_type_safe_route_resolves_across_files() {
    let project = TempDir::new("typed");
    write(
        &project,
        "AppNavHost.kt",
        "@Composable\nfun AppNavHost(nav: NavHostController) {\n    NavHost(nav, startDestination = Home) {\n        composable<Detail> { DetailScreen() }\n    }\n}\n",
    );
    write(
        &project,
        "ListScreen.kt",
        "@Composable\nfun ListScreen(nav: NavHostController) {\n    nav.navigate(Detail(1))\n}\n",
    );
    write(
        &project,
        "DetailScreen.kt",
        "@Composable\nfun DetailScreen() {}\n",
    );
    let rows = callers(&project, "DetailScreen");
    assert!(rows.iter().any(|r| r["caller"] == "ListScreen"), "{rows:?}");
}
