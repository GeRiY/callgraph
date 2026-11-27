use callgraph::query::{OutputLimit, SelectorSet};
use std::path::PathBuf;

#[test]
fn selector_rejects_mixed_arguments() {
    let args = vec![PathBuf::from("a.php::one"), PathBuf::from("b.php")];
    assert!(SelectorSet::parse(&args).is_err());
}

#[test]
fn selector_groups_methods_by_file() {
    let args = vec![PathBuf::from("a.php::one"), PathBuf::from("a.php::two")];
    let selectors = SelectorSet::parse(&args).unwrap();
    assert_eq!(
        selectors
            .methods_for(std::path::Path::new("a.php"))
            .unwrap(),
        ["one", "two"]
    );
}

#[test]
fn output_limit_can_be_overridden() {
    let limited = OutputLimit::from_env(false).with_value(Some(1));
    assert!(
        limited
            .method_index("a\nb".into(), "index".into())
            .starts_with('⚠')
    );
}

#[test]
fn bare_method_selector_finds_the_object_literal_method() {
    let dir = std::env::temp_dir().join(format!("callgraph-objsel-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("store.ts"),
        "export const store = {\n  fetch() {\n    return 1\n  },\n  save: () => 2,\n}\n",
    )
    .unwrap();
    let engine = callgraph::query::QueryEngine::new(Some(&dir), None);
    let pick = |sel: &str| {
        engine
            .query(&SelectorSet::parse(&[PathBuf::from(sel)]).unwrap())
            .map(|rows| rows[0].methods.clone().unwrap())
    };
    // a pont nélküli név az `obj.name` metódusra is illeszkedik, a teljes név továbbra is működik
    assert_eq!(pick("store.ts::fetch").unwrap(), ["store.fetch"]);
    assert_eq!(pick("store.ts::store.save").unwrap(), ["store.save"]);
    assert!(pick("store.ts::missing").is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn partly_missing_method_selection_keeps_the_found_ones_and_warns() {
    let dir = std::env::temp_dir().join(format!("callgraph-partsel-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("a.ts"),
        "export function alpha() {\n  return 1\n}\nexport function beta() {\n  return 2\n}\n",
    )
    .unwrap();
    let engine = callgraph::query::QueryEngine::new(Some(&dir), None);
    let parse = |sels: &[&str]| {
        SelectorSet::parse(&sels.iter().map(PathBuf::from).collect::<Vec<_>>()).unwrap()
    };
    // one of three is missing: the two existing ones come back and a single warning line names the missing one
    let rows = engine
        .query(&parse(&["a.ts::alpha", "a.ts::gamma", "a.ts::beta"]))
        .unwrap();
    assert_eq!(rows[0].methods.clone().unwrap(), ["alpha", "beta"]);
    let warnings = engine.warnings();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(
        warnings[0].starts_with("⚠ no such method: gamma (in a.ts)")
            || warnings[0].starts_with("⚠ nincs ilyen metódus: gamma"),
        "{warnings:?}"
    );
    // nothing found: still an error, no warning
    let engine = callgraph::query::QueryEngine::new(Some(&dir), None);
    assert!(
        engine
            .query(&parse(&["a.ts::gamma", "a.ts::delta"]))
            .is_err()
    );
    assert!(engine.warnings().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}
