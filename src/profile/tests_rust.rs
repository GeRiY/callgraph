//! Rust crate / workspace: belépési pontok, `src/<modul>` rétegek, tests / benches / examples, fixture-mappák és a
//! CLI / könyvtár architektúra — fájlrendszer nélkül.

use super::classify::{Classifier, Reason};
use super::ecosystem::workspace_members;
use super::project::ProjectProfile;
use super::role::Role;
use super::tests_layers::entries;

fn role(rel: &str) -> Option<&'static str> {
    Classifier::default().role_of(rel).map(Role::name)
}

#[test]
fn entry_points_and_modules_follow_the_crate_layout() {
    assert_eq!(role("src/main.rs"), Some("entry"));
    assert_eq!(role("src/lib.rs"), Some("entry"));
    assert_eq!(role("src/bin/tool.rs"), Some("entry"));
    assert_eq!(role("crates/core/src/lib.rs"), Some("entry"));
    assert_eq!(role("src/lang/rs/visitor/calls.rs"), Some("module"));
    assert_eq!(role("crates/core/src/query/engine.rs"), Some("module"));
    assert_eq!(role("src/util.rs"), Some("module"));
    // a crate src/ mappáján kívül nincs szerep
    assert_eq!(role("build.rs"), None);
    assert_eq!(role("xtask/main.rs"), None);
    // a mappa- és névszabályok előbbre valók
    assert_eq!(role("src/handlers/users.rs"), Some("handler"));
    assert_eq!(role("src/db/mod.rs"), Some("repository"));
}

#[test]
fn a_module_is_anchored_at_its_top_level_folder() {
    let c = Classifier::default();
    let class = c.classify("src/lang/rs/visitor/calls.rs").unwrap();
    assert_eq!(class.anchor, "src/lang");
    assert_eq!(class.reason, Reason::Folder("lang".to_owned()));
    let bin = c.classify("src/bin/tool.rs").unwrap();
    assert_eq!(bin.anchor, "src/bin");
}

#[test]
fn tests_benches_examples_and_fixtures_are_support_roles() {
    assert_eq!(role("tests/api.rs"), Some("test"));
    assert_eq!(role("benches/speed.rs"), Some("bench"));
    assert_eq!(role("examples/demo.rs"), Some("example"));
    assert_eq!(role("tests/fixtures/shop/Cart.php"), Some("test"));
    assert_eq!(
        role("golden/samples/js/src/components/Card.vue"),
        Some("test")
    );
    assert_eq!(
        role("golden/samples/swift/ChatViewModel.swift"),
        Some("test")
    );
    assert_eq!(
        role("golden/samples/rust/src/handlers/users.rs"),
        Some("test")
    );
}

#[test]
fn a_rust_cli_with_fixture_samples_is_not_a_web_or_frontend_project() {
    let mut paths: Vec<String> = (0..12).map(|n| format!("src/lang/mod{n}.rs")).collect();
    paths.extend(
        [
            "src/main.rs",
            "src/lib.rs",
            "src/query/engine.rs",
            "tests/cli.rs",
            "golden/samples/js/src/components/Card.vue",
            "golden/samples/swift/ChatViewModel.swift",
            "golden/samples/swift/ChatView.swift",
            "golden/samples/rust/src/handlers/users.rs",
            "golden/samples/rust/src/routes.rs",
        ]
        .map(str::to_owned),
    );
    let refs: Vec<&str> = paths.iter().map(String::as_str).collect();
    let profile = ProjectProfile::aggregate(&entries(&refs), Vec::new(), None, false);
    let ids: Vec<&str> = profile.architectures.iter().map(|a| a.id).collect();
    assert_eq!(ids, ["rust-cli"], "{:?}", profile.architectures);
    let labels: Vec<&str> = profile.layers.iter().map(|l| l.label.as_str()).collect();
    assert!(labels.contains(&"module"), "{labels:?}");
    assert!(labels.contains(&"entry"), "{labels:?}");
    assert!(
        !labels.iter().any(|l| l.starts_with("component")
            || l.starts_with("viewmodel")
            || l.starts_with("handler")
            || l.starts_with("route")),
        "{labels:?}"
    );
    assert!(profile.unknown.is_empty(), "{:?}", profile.unknown);
}

#[test]
fn workspace_members_are_read_from_cargo_toml() {
    let toml = r#"[package]
name = "x"
members = ["not-this"]

[workspace]
resolver = "2"
members = [
    "crates/*", # all crates
    "tools/xtask",
]

[dependencies]
serde = "1"
"#;
    assert_eq!(workspace_members(toml), ["crates/*", "tools/xtask"]);
    assert_eq!(
        workspace_members("[workspace]\nmembers = [\"a\", \"b\"]\n"),
        ["a", "b"]
    );
    assert!(workspace_members("[package]\nname = \"x\"\n").is_empty());
}
