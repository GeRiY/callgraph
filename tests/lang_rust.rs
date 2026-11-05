//! A Rust backend: regisztráció, és a `golden/samples/rust/` minták outline- és stats-kimenete.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use callgraph::cli::{CliArgs, build_command};
use callgraph::lang::{LanguageRegistry, LanguageRole};

/// A minta-projekt másolata egy ideiglenes mappában (a `show` cache-t ír a projekt mellé).
struct Sample(PathBuf);

impl Sample {
    fn new() -> Self {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("callgraph-rust-{id}"));
        let samples = Path::new(env!("CARGO_MANIFEST_DIR")).join("golden/samples/rust");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::copy(samples.join("Cargo.toml"), root.join("Cargo.toml")).unwrap();
        for entry in fs::read_dir(samples.join("src")).unwrap() {
            let path = entry.unwrap().path();
            fs::copy(&path, root.join("src").join(path.file_name().unwrap())).unwrap();
        }
        Self(root)
    }

    fn show(&self, file: &str, extra: &[&str]) -> String {
        let mut args = vec![
            "show".to_owned(),
            "--project".to_owned(),
            self.0.to_string_lossy().into_owned(),
        ];
        args.extend(extra.iter().map(|s| (*s).to_owned()));
        args.push(file.to_owned());
        let command = build_command(CliArgs::parse(args).unwrap()).unwrap();
        let (mut out, mut err) = (Vec::new(), Vec::new());
        assert_eq!(command.run(&mut out, &mut err).unwrap(), 0, "{err:?}");
        String::from_utf8(out).unwrap()
    }
}

impl Drop for Sample {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn registry_routes_rs_to_rust_backend() {
    let r = LanguageRegistry::with_defaults();
    let l = r.for_path(Path::new("src/main.rs")).unwrap();
    assert_eq!((l.id(), l.role()), ("rust", LanguageRole::Backend));
    assert!(r.extensions().contains(&"rs"));
    assert_eq!(r.extensions_for(&["rust"]), ["rs"]);
}

#[test]
fn outline_of_axum_handlers() {
    let sample = Sample::new();
    let out = sample.show("src/handlers.rs", &["--format", "outline"]);
    for expected in [
        "module handlers  L1-57",
        "struct CreateOrder  L15-18",
        "  router()  L21-26",
        "  create_order()  L29-40",
        "  show_order()  L43-52",
        "framework_use",
        "axum::routing::post",
        "macro_use",
        "tracing::info!",
        "external_call     next_id",
        "[TODO] TODO: validate the item against the catalogue",
    ] {
        assert!(out.contains(expected), "missing {expected:?} in:\n{out}");
    }
}

#[test]
fn outline_of_trait_and_impls() {
    let sample = Sample::new();
    let out = sample.show("src/shapes.rs", &["--format", "outline"]);
    for expected in [
        "trait Shape extends fmt::Debug  L7-15",
        "  area()  L9-9 [abstract]",
        "struct Circle implements Shape, fmt::Display  L18-20",
        "struct Rect implements Shape  L23-23",
        "  new()  L28-32 [static]",
        "UNIT = Circle { radius: 1.0 }",
        "internal_helper",
    ] {
        assert!(out.contains(expected), "missing {expected:?} in:\n{out}");
    }
}

#[test]
fn stats_and_flow_of_enum_state_machine() {
    let sample = Sample::new();
    let stats = sample.show("src/state.rs", &["--format", "stats"]);
    assert!(stats.contains("(module, enum, struct)"), "{stats}");
    assert!(stats.contains("native_rust"), "{stats}");
    let flow = sample.show("src/state.rs", &["--format", "flow"]);
    assert!(flow.contains("match (self)  OrderStatus::label"), "{flow}");
    assert!(
        flow.contains("(status, \"cancel\") if status != OrderStatus::Cancelled"),
        "{flow}"
    );
}

#[test]
fn json_has_imports_aliases_and_framework() {
    let sample = Sample::new();
    let out = sample.show("src/aliases.rs", &["--format", "json"]);
    let rows: Vec<serde_json::Value> = serde_json::from_str(&out).unwrap();
    let graph = &rows[0];
    assert_eq!(graph["framework"], "axum");
    let imports = &graph["imports"];
    assert_eq!(imports["Map"], "std::collections::BTreeMap");
    assert_eq!(imports["Round"], "crate::shapes::Circle");
    assert_eq!(imports["shapes"], "crate::shapes");
    assert_eq!(imports["crate::state::*"], "crate::state::*");
    let calls = graph["calls"].as_array().unwrap();
    let call = |target: &str| calls.iter().find(|c| c["target"] == target).unwrap();
    assert_eq!(call("Round::new")["category"], "module_use");
    assert_eq!(call("Round::new")["resolved"], "Circle::new");
    assert_eq!(call("json!")["category"], "macro_use");
    assert_eq!(call("index")["caller"], "count");
    assert_eq!(call("index")["resolved"], "aliases::index");
}

/// Egy név több metódusra is illeszkedhet (több `impl`): a `::metódus` kijelölés ettől nem eshet szét.
#[test]
fn method_selector_with_duplicate_names_does_not_panic() {
    let sample = Sample::new();
    fs::write(
        sample.0.join("src/dup.rs"),
        "struct A;\nstruct B;\nimpl A { fn new() -> Self { A } }\nimpl B { fn new() -> Self { B } }\n",
    )
    .unwrap();
    let flow = sample.show("src/dup.rs::new", &["--format", "flow"]);
    assert!(!flow.is_empty());
    let outline = sample.show("src/dup.rs::new", &[]);
    assert_eq!(outline.matches("new()").count(), 2, "{outline}");
}
