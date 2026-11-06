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
        let root =
            std::env::temp_dir().join(format!("callgraph-rust-{id}-{}-{}", std::process::id(), {
                static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
                N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            }));
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
        // `State(state): State<AppState>` → a fogadó típusa ismert
        "AppState::next_id",
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

/// A projekt `callers` parancsa: a JSONL-sorok.
fn callers(sample: &Sample, pattern: &str) -> Vec<serde_json::Value> {
    let args = CliArgs::parse([
        "callers".to_owned(),
        pattern.to_owned(),
        "--project".to_owned(),
        sample.0.to_string_lossy().into_owned(),
    ])
    .unwrap();
    let command = build_command(args).unwrap();
    let (mut out, mut err) = (Vec::new(), Vec::new());
    assert_eq!(command.run(&mut out, &mut err).unwrap(), 0, "{err:?}");
    String::from_utf8(out)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn api_lists_pub_methods_of_impl_blocks() {
    let sample = Sample::new();
    let out = sample.show("src/limits.rs", &["--format", "api"]);
    for expected in [
        "struct Client  L15-17",
        "  public constants (1):\n    RETRIES = 3",
        "  public API (3):",
        "    new(state: AppState): Self  L23 [static]",
        "    allocate(&self): u64  L28",
        "      Allocates an order id (retried a few times).",
        "    within_limit(&self, quantity: u32): bool  L37",
        "    allocate_all(clients: &[Client]): Vec<u64>  L47",
        "    MAX_ITEMS = 50",
    ] {
        assert!(out.contains(expected), "missing {expected:?} in:\n{out}");
    }
    // privát: a `fn banner_len` és a `const GREETING`
    assert!(!out.contains("banner_len"), "{out}");
    assert!(!out.contains("GREETING"), "{out}");
    // trait-implementáció metódusa is publikus (a trait-en át hívható)
    let shapes = sample.show("src/shapes.rs", &["--format", "api"]);
    assert!(shapes.contains("    area(&self): f64  L36"), "{shapes}");
    assert!(
        shapes.contains("    fmt(&self, f: &mut fmt::Formatter<'_>): fmt::Result  L56"),
        "{shapes}"
    );
}

#[test]
fn const_search_finds_rust_definitions_and_references() {
    let sample = Sample::new();
    let out = sample.show("src", &["--const", "MAX_ITEMS"]);
    assert!(out.contains("limits.rs"), "{out}");
    assert!(
        out.contains("limits::MAX_ITEMS") && out.contains("[definition] = 50"),
        "{out}"
    );
    assert!(out.contains("L38"), "{out}");
    assert!(out.contains("Client::within_limit"), "{out}");
    let assoc = sample.show("src/limits.rs", &["--const", "RETRIES"]);
    assert!(assoc.contains("Client::allocate"), "{assoc}");
    assert!(assoc.contains("Client::RETRIES"), "{assoc}");
    assert!(assoc.contains("[definition] = 3"), "{assoc}");
    let framework = sample.show("src/handlers.rs", &["--const", "BAD_REQUEST"]);
    assert!(
        framework.contains("axum::http::StatusCode::BAD_REQUEST"),
        "{framework}"
    );
}

#[test]
fn strings_include_const_and_static_initializers() {
    let sample = Sample::new();
    let bytes = sample.show("src/limits.rs", &["--strings", "raw bytes"]);
    assert!(bytes.contains("L12"), "{bytes}");
    assert!(bytes.contains("\"Hello from the raw bytes\""), "{bytes}");
    let banner = sample.show("src/limits.rs", &["--strings", "open for business"]);
    assert!(banner.contains("L9"), "{banner}");
}

#[test]
fn method_callers_by_type_include_inferred_and_uncertain_receivers() {
    let sample = Sample::new();
    let out = sample.show("src", &["--grep", "AppState::next_id"]);
    for expected in [
        "create_order()",
        "allocate()",
        "report()",
        "allocate_all()",
        "?::next_id",
    ] {
        assert!(out.contains(expected), "missing {expected:?} in:\n{out}");
    }
    let rows = callers(&sample, "AppState::next_id");
    let has = |caller: &str, target: &str| {
        rows.iter()
            .any(|r| r["caller"] == caller && r["target"] == target)
    };
    assert!(has("create_order", "AppState::next_id"), "{rows:?}");
    assert!(has("allocate", "AppState::next_id"), "{rows:?}");
    assert!(has("allocate_all", "?::next_id"), "{rows:?}");
}

#[test]
fn body_is_compact_by_default_with_doc_and_raw_variants() {
    let sample = Sample::new();
    let target = "src/handlers.rs::create_order";
    let compact = sample.show(target, &["--body"]);
    assert!(
        compact.contains("    29  pub async fn create_order(\n"),
        "{compact}"
    );
    // a törzsbeli megjegyzés marad, a docblock kimarad — ezt a fejléc jelzi
    assert!(
        compact.contains("    35      }\n    36      // TODO: validate"),
        "{compact}"
    );
    assert!(!compact.contains("Creates an order"), "{compact}");
    assert!(
        compact.contains("create_order()  L29-40 (docblock L28 "),
        "{compact}"
    );
    assert!(compact.contains(" --doc)\n"), "{compact}");
    let doc = sample.show(target, &["--body", "--doc"]);
    assert!(
        doc.contains("    28  /// Creates an order and returns its id.\n    29  pub async fn"),
        "{doc}"
    );
    assert!(doc.contains("    36      // TODO: validate"), "{doc}");
    assert!(doc.contains("create_order()  L29-40\n"), "{doc}");
    assert!(!doc.contains("docblock L"), "{doc}");
    let raw = sample.show(target, &["--raw"]);
    assert!(raw.contains("    28  /// Creates an order"), "{raw}");
    assert!(raw.contains("    36      // TODO: validate"), "{raw}");
    assert!(raw.contains("create_order()  L29-40\n"), "{raw}");
}

#[test]
fn body_without_docblock_has_no_omission_note() {
    let sample = Sample::new();
    fs::write(
        sample.0.join("src/plain.rs"),
        "pub fn plain(a: u8) -> u8 {\n    // why\n\n    /* block */\n    a + 1\n}\n",
    )
    .unwrap();
    let out = sample.show("src/plain.rs::plain", &["--body"]);
    assert!(out.contains("plain()  L1-6\n"), "{out}");
    assert!(!out.contains("docblock"), "{out}");
    assert!(
        out.contains("     2      // why\n     4      /* block */\n     5      a + 1\n"),
        "{out}"
    );
}

#[test]
fn grep_with_body_prints_the_callers_source() {
    let sample = Sample::new();
    let out = sample.show("src", &["--grep", "AppState::next_id", "--body"]);
    assert!(out.contains("create_order()  L29-40"), "{out}");
    assert!(out.contains("let id = state.next_id();"), "{out}");
    assert!(out.contains("// TODO: validate"), "{out}");
}

#[test]
fn const_search_lists_re_exports() {
    let sample = Sample::new();
    fs::write(
        sample.0.join("src/lib.rs"),
        "pub mod state;\npub use limits::{MAX_ITEMS, Client};\npub(crate) use limits::MAX_ITEMS as LIMIT;\n",
    )
    .unwrap();
    let out = sample.show("src", &["--const", "MAX_ITEMS"]);
    assert!(out.contains("[definition] = 50"), "{out}");
    assert!(out.contains("[re-export] limits::MAX_ITEMS"), "{out}");
    let lib = sample.show("src/lib.rs", &["--const", "MAX_ITEMS"]);
    assert!(lib.contains("L2 "), "{lib}");
    assert!(lib.contains("LIMIT"), "{lib}");
    assert_eq!(lib.matches("[re-export]").count(), 2, "{lib}");
}
