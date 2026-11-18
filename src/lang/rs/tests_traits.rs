//! Trait ↔ implementáció a hívó-indexen át: a trait-metódus hívói és az implementáció közvetlen hívói
//! kölcsönösen látják egymást `via` jelöléssel.

use super::RustBackend;
use crate::index::CallerIndex;
use crate::model::FileGraph;

fn graph_of(path: &str, text: &str) -> FileGraph {
    RustBackend::with_defaults()
        .analyze_source(path, text.as_bytes())
        .graph
}

const TRAITS: &str = r#"pub trait Handler {
    fn handle(&self, x: i32) -> i32;
}

pub struct Foo;
pub struct Bar;

impl Handler for Foo {
    fn handle(&self, x: i32) -> i32 { x }
}

impl Handler for Bar {
    fn handle(&self, x: i32) -> i32 { x + 1 }
}
"#;

const USERS: &str = r#"pub fn generic<T: Handler>(h: &T) -> i32 { h.handle(2) }

pub fn direct(f: &Foo) -> i32 { f.handle(3) }
"#;

fn via_of(index: &CallerIndex, pattern: &str) -> Vec<(String, Option<String>)> {
    let mut found: Vec<(String, Option<String>)> = index
        .callers(pattern, 1)
        .unwrap()
        .into_iter()
        .map(|r| (r.caller, r.via))
        .collect();
    found.sort();
    found
}

#[test]
fn trait_and_implementation_callers_see_each_other_with_a_via_mark() {
    let traits = graph_of("/p/src/traits.rs", TRAITS);
    let users = graph_of("/p/src/users.rs", USERS);
    let index = CallerIndex::build([("/p/src/traits.rs", &traits), ("/p/src/users.rs", &users)]);
    // az implementáció keresése: a generikus (trait-en át hívó) és a közvetlen hívó is megvan
    assert_eq!(
        via_of(&index, "Foo::handle"),
        vec![
            ("direct".to_owned(), None),
            ("generic".to_owned(), Some("Handler".to_owned())),
        ]
    );
    // a trait keresése: a közvetlen `Foo::handle` hívó is megvan, `via Foo`
    assert_eq!(
        via_of(&index, "Handler::handle"),
        vec![
            ("direct".to_owned(), Some("Foo".to_owned())),
            ("generic".to_owned(), None),
        ]
    );
    // a testvér-implementáció nem kapja a másik implementáció közvetlen hívóját
    assert_eq!(
        via_of(&index, "Bar::handle"),
        vec![("generic".to_owned(), Some("Handler".to_owned()))]
    );
}

#[test]
fn generic_with_two_traits_from_other_files_resolves_each_call_to_its_trait() {
    let a = graph_of("/p/src/a.rs", "pub trait A {\n    fn ping(&self);\n}\n");
    let b = graph_of("/p/src/b.rs", "pub trait B {\n    fn pong(&self);\n}\n");
    let run = graph_of(
        "/p/src/run.rs",
        "pub fn run<T: A + B>(t: &T) {\n    t.ping();\n    t.pong();\n}\n",
    );
    let index = CallerIndex::build([
        ("/p/src/a.rs", &a),
        ("/p/src/b.rs", &b),
        ("/p/src/run.rs", &run),
    ]);
    let callers = |pattern: &str| -> Vec<String> {
        index
            .callers(pattern, 1)
            .unwrap()
            .into_iter()
            .map(|r| r.caller)
            .collect()
    };
    assert_eq!(callers("A::ping"), vec!["run".to_owned()]);
    assert_eq!(callers("B::pong"), vec!["run".to_owned()]);
}

#[test]
fn field_chain_receiver_is_not_a_resolved_caller_of_the_own_type_at_depth() {
    // `self.inner.overrides.matched(p)` has an unknown receiver type: the unique-name fallback must not turn it
    // into a resolved caller of `Ignore::matched` on the second level
    let ignore = graph_of(
        "/p/src/dir.rs",
        r#"pub struct Ignore {
    inner: Inner,
}

impl Ignore {
    pub fn matched(&self, p: &str) -> bool {
        self.matched_ignore(p)
    }

    pub fn matched_ignore(&self, p: &str) -> bool {
        self.inner.overrides.matched(p)
    }
}
"#,
    );
    let index = CallerIndex::build([("/p/src/dir.rs", &ignore)]);
    let first = index.callers("Ignore::matched_ignore", 1).unwrap();
    assert!(first.iter().any(|row| row.caller == "matched"));
    let (rows, _) = index.parents_of_with_skips(&first, 2, false);
    assert!(
        !rows
            .iter()
            .any(|row| row.level == 2 && row.caller == "matched_ignore"),
        "{rows:?}"
    );
}
