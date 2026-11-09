//! A Rust-csővezeték szélső esetei: idegen típus `impl`-je, névütközés, szintaktikai hiba, felső szintű makrók.

use super::RustBackend;
use super::tests::calls_of;

#[test]
fn impl_for_foreign_type_and_name_collisions() {
    let src = "struct A;\nimpl A { fn f(&self) {} }\nimpl A { fn f(&self, x: u8) {} }\nimpl Clone for Vec2 { fn clone(&self) -> Self { todo!() } }\n";
    let g = RustBackend::with_defaults()
        .analyze_source("src/a.rs", src.as_bytes())
        .graph;
    let names: Vec<(&str, &str)> = g
        .methods
        .iter()
        .map(|m| (m.container.as_deref().unwrap(), m.name.as_str()))
        .collect();
    assert_eq!(names, [("A", "f"), ("A", "f@L3"), ("Vec2", "clone")]);
    let vec2 = g.containers.iter().find(|c| c.name == "Vec2").unwrap();
    assert_eq!(
        (vec2.kind.as_str(), vec2.implements.clone()),
        ("impl", Some(vec!["Clone".into()]))
    );
}

#[test]
fn syntax_error_is_a_warning_with_file_container() {
    let a = RustBackend::with_defaults().analyze_source("src/bad.rs", b"fn a( {\n");
    assert_eq!(a.warnings.len(), 1);
    assert!(a.warnings[0].contains("src/bad.rs"));
    assert_eq!(a.graph.containers.len(), 1);
    assert_eq!(a.graph.containers[0].name, "bad");
    assert_eq!(a.graph.parse_errors.len(), 1);
    assert_eq!(a.graph.parse_errors[0].line, 1);
}

#[test]
fn syntax_error_keeps_the_intact_prefix() {
    let src = "fn a() -> u32 { 1 }\nfn b() -> u32 { 2 }\nfn c( {\n";
    let a = RustBackend::with_defaults().analyze_source("src/r.rs", src.as_bytes());
    let names: Vec<&str> = a.graph.methods.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(names, ["a", "b"]);
    assert_eq!(a.graph.parse_errors.len(), 1);
    assert_eq!(a.graph.parse_errors[0].line, 3, "{:?}", a.graph.parse_errors);
    // a fájl-container a teljes fájlt fedi
    let module = &a.graph.containers[0];
    assert_eq!((module.line, module.end_line), (1, 3));
    assert_eq!(a.warnings.len(), 1);
}

#[test]
fn valid_file_has_no_parse_errors() {
    let a = RustBackend::with_defaults().analyze_source("src/ok.rs", b"fn a() {}\n");
    assert!(a.graph.parse_errors.is_empty());
}

#[test]
fn top_level_macros_and_macro_rules() {
    let src = "macro_rules! hi { () => {} }\nthread_local! { static X: u8 = make(); }\nfn make() -> u8 { 1 }\n";
    let g = RustBackend::with_defaults()
        .analyze_source("src/m.rs", src.as_bytes())
        .graph;
    assert!(
        g.methods
            .iter()
            .any(|m| m.name == "hi!" && m.kind.as_deref() == Some("macro_rules"))
    );
    let top = calls_of(&g, "__top__");
    assert!(top.contains(&("thread_local!".into(), "macro_use".into(), None)));
    assert!(top.contains(&(
        "make".into(),
        "internal_helper".into(),
        Some("m::make".into())
    )));
}
