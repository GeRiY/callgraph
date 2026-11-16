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
