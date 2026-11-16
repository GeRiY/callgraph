//! Függvény-hivatkozás értékként (`.map(Self::parse)`), generikus trait-korlát fogadó, függvény-
//! visszatérési típus fogadó, és a fájl saját almoduljai (`mod util;` → `util::f()`).

use super::RustBackend;
use super::tests::calls_of;
use crate::model::FileGraph;

const LIB: &str = r#"pub mod shape;
mod util;
use crate::util::double as twice;

pub trait Handler {
    fn handle(&self, x: i32) -> i32;
}

pub struct Bar;

impl Bar {
    pub fn add(&mut self, x: i32) {}
    pub fn parse(s: &str) -> Option<i32> { None }
}

pub struct Svc {
    cur: Bar,
}

impl Svc {
    fn p(s: &&str) -> Option<i32> { Bar::parse(s) }
    fn dbl(x: i32) -> i32 { x * 2 }
    fn make(&self) -> Result<Bar, String> { Ok(Bar) }
    fn open() -> Result<Self, String> { Err(String::new()) }

    pub fn run(&self, items: &[&str]) -> Vec<i32> {
        let z: Vec<i32> = items.iter().filter_map(Self::p).collect();
        let w: Vec<Option<i32>> = items.iter().map(Bar::parse).collect();
        let v: Vec<i32> = z.into_iter().map(util::double).map(twice).map(helper).collect();
        let a: Vec<i32> = v.iter().map(|x| Self::dbl(*x)).collect();
        a
    }

    pub fn typed(&self) {
        let m = self.make().unwrap();
        m.add(1);
        let s = Self::open().unwrap();
        s.dbl_all();
    }
}

fn helper(x: i32) -> i32 { x }

pub fn generic<T: Handler>(h: &T) -> i32 { h.handle(2) }
pub fn where_bound<T>(h: T) -> i32 where T: Handler + Clone { h.handle(2) }
pub fn std_bound<T: Clone>(h: T) -> T { h.clone() }
pub fn modules() -> i32 { util::double(1) + shape::area(1) }
"#;

fn graph() -> FileGraph {
    RustBackend::with_defaults()
        .analyze_source("/p/src/lib.rs", LIB.as_bytes())
        .graph
}

type Row = (String, String, Option<String>);

fn find<'a>(calls: &'a [Row], target: &str) -> Vec<&'a Row> {
    calls.iter().filter(|(t, _, _)| t == target).collect()
}

#[test]
fn method_and_function_references_are_calls() {
    let run = calls_of(&graph(), "run");
    let p = find(&run, "Self::p");
    assert_eq!(p.len(), 1, "{run:?}");
    assert_eq!(
        (p[0].1.as_str(), p[0].2.as_deref()),
        ("internal_helper", Some("Svc::p"))
    );
    let parse = find(&run, "Bar::parse");
    assert_eq!(parse.len(), 1, "{run:?}");
    assert_eq!(parse[0].1, "internal_helper");
    // szabad függvény értékként, importált alias nélkül is
    let helper = find(&run, "helper");
    assert_eq!(helper.len(), 1, "{run:?}");
    assert_eq!(helper[0].1, "internal_helper");
    // modul-út és `use ... as` alias
    let double = find(&run, "util::double");
    assert_eq!(double.len(), 1, "{run:?}");
    assert_eq!(double[0].1, "module_use");
    assert_eq!(double[0].2, None);
    let twice = find(&run, "twice");
    assert_eq!(twice.len(), 1, "{run:?}");
    assert_eq!(twice[0].2.as_deref(), Some("util::double"));
    // a closure-ön belüli hívás továbbra is hívás
    assert_eq!(find(&run, "Self::dbl").len(), 1, "{run:?}");
}

#[test]
fn references_do_not_double_count_direct_calls_or_locals() {
    let g = graph();
    // `helper(x)` hívás a `run`-ban nincs, a `std_bound`-ban a `h` paraméter nem függvény
    let std_bound = calls_of(&g, "std_bound");
    assert!(
        std_bound.iter().all(|(t, _, _)| t == "clone"),
        "{std_bound:?}"
    );
}

#[test]
fn generic_trait_bound_receivers_resolve_to_the_trait() {
    let g = graph();
    for caller in ["generic", "where_bound"] {
        let calls = calls_of(&g, caller);
        let handle = find(&calls, "handle");
        assert_eq!(handle.len(), 1, "{calls:?}");
        assert_eq!(
            (handle[0].1.as_str(), handle[0].2.as_deref()),
            ("internal_helper", Some("Handler::handle")),
            "{caller}: {calls:?}"
        );
    }
    // `T: Clone` std-korlát: nem ad saját típust
    let std_bound = calls_of(&g, "std_bound");
    assert!(
        std_bound.iter().all(|(_, _, r)| r.is_none()),
        "{std_bound:?}"
    );
}

#[test]
fn method_return_types_type_the_receiver() {
    let typed = calls_of(&graph(), "typed");
    // `self.make()` → `Result<Bar, _>`: `m.add()` a `Bar::add`
    let add = find(&typed, "add");
    assert_eq!(add.len(), 1, "{typed:?}");
    assert_eq!(add[0].2.as_deref(), Some("Bar::add"));
    // `Self::open()` → `Result<Self, _>`: `s.dbl_all()` a `Svc`-re kötve
    let dbl_all = find(&typed, "dbl_all");
    assert_eq!(dbl_all.len(), 1, "{typed:?}");
    assert_eq!(dbl_all[0].2.as_deref(), Some("Svc::dbl_all"));
}

#[test]
fn own_child_modules_are_crate_paths_not_packages() {
    let calls = calls_of(&graph(), "modules");
    assert!(
        calls.contains(&("util::double".into(), "module_use".into(), None)),
        "{calls:?}"
    );
    assert!(
        calls.contains(&("shape::area".into(), "module_use".into(), None)),
        "{calls:?}"
    );
}

#[test]
fn inline_module_items_resolve_to_the_inner_container() {
    let src = "mod inner { pub fn f() {} pub struct T; impl T { pub fn new() -> T { T } } }\n\
               pub fn go() { inner::f(); inner::T::new(); }\n";
    let g = RustBackend::with_defaults()
        .analyze_source("/p/src/lib.rs", src.as_bytes())
        .graph;
    let go = calls_of(&g, "go");
    assert!(
        go.contains(&("inner::f".into(), "internal_helper".into(), None)),
        "{go:?}"
    );
    assert!(
        go.iter()
            .any(|(t, c, _)| t == "inner::T::new" && c == "internal_helper"),
        "{go:?}"
    );
}
