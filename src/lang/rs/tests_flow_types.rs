//! Elem-típus következtetés (ciklusváltozó, `if let` / `match` / `while let`, closure-paraméter), a `T: A + B`
//! fogadó, a teszt-függvények `kind`-ja és a zajos hívások (`Some`, `Box::new`, closure-hívás).

use super::RustBackend;
use super::tests::calls_of;
use crate::model::FileGraph;

const LIB: &str = r#"use std::collections::{HashMap, HashSet};

pub trait Alpha {
    fn ping(&self);
}

pub trait Beta {
    fn pong(&self);
}

pub struct Bar;
pub struct Item;

impl Bar {
    pub fn run(&self) {}
}

impl Item {
    pub fn run(&self) {}
}

pub struct Chart {
    bars: Vec<Bar>,
    by_name: HashMap<String, Bar>,
    cur: Option<Bar>,
}

impl Chart {
    pub fn grow(&mut self) {
        for b in self.bars.iter_mut() {
            b.run();
        }
    }

    pub fn pairs(&self) {
        for (k, v) in self.by_name.iter() {
            v.run();
            k.shout();
        }
    }

    pub fn only_values(&self) {
        for w in self.by_name.values() {
            w.run();
        }
    }

    pub fn borrowed(&self, items: &[Item]) {
        for x in items {
            x.run();
        }
    }

    pub fn by_ref(&self, items: Vec<Item>) {
        for y in &items {
            y.run();
        }
    }

    pub fn owned(&self, items: Vec<Item>, set: HashSet<Bar>) {
        for z in items.into_iter() {
            z.run();
        }
        for s in set {
            s.run();
        }
    }

    pub fn numbered(&self, items: Vec<Item>) {
        for (i, e) in items.iter().enumerate().skip(1) {
            e.run();
            i.shout();
        }
    }

    pub fn unknown_loop(&self, n: usize) {
        for q in 0..n {
            q.run();
        }
    }

    pub fn option_field(&self) {
        if let Some(x) = self.cur {
            x.run();
        }
    }

    pub fn option_match(&self) {
        match self.cur {
            Some(m) => m.run(),
            None => {}
        }
    }

    pub fn else_branch(&self) {
        let Some(y) = self.cur.as_ref() else {
            return;
        };
        y.run();
    }

    pub fn drain_stack(&self, stack: Vec<Item>, r: Result<Bar, String>) {
        let mut st = stack;
        while let Some(top) = st.pop() {
            top.run();
        }
        if let Ok(b) = r {
            b.run();
        }
    }

    pub fn shadowed(&self, items: Vec<Item>) {
        for w in items {
            w.run();
        }
        let w = Bar;
        w.run();
    }
}

pub fn typed_closure() {
    let go = |f: Bar| f.run();
    go(Bar);
}

pub fn adaptors(items: Vec<Item>, by_name: HashMap<String, Bar>) {
    let _a: Vec<()> = items.iter().map(|x| x.run()).collect();
    by_name.values().for_each(|v| v.run());
    let _b = items.iter().filter(|p| p.run()).count();
    let _c = items.iter().fold(0, |acc, e| e.run());
    let _d = items.iter().enumerate().map(|(i, t)| t.run());
    let _e = items.iter().map(|u| u.other());
}

pub fn both<T: Alpha + Beta>(t: &T) {
    t.ping();
    t.pong();
    t.other();
}

pub fn both_where<T>(t: T)
where
    T: Beta + Alpha + Clone,
{
    t.ping();
}

pub fn noise(f: impl Fn(i32) -> i32) -> Result<Option<Box<i32>>, String> {
    let c = |n: i32| n + 1;
    let a = c(1);
    let b = f(2);
    let d: Option<Box<i32>> = Some(Box::new(a));
    let _e = std::rc::Rc::new(1);
    let _g: i32 = Default::default();
    let n = String::new();
    if b > 0 {
        return Err(n);
    }
    Ok(d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds() {}

    #[tokio::test]
    async fn awaits() {}

    fn fixture() {}
}

#[test]
fn top_level_test() {}

pub fn normal() {}
"#;

fn graph() -> FileGraph {
    RustBackend::with_defaults()
        .analyze_source("/p/src/lib.rs", LIB.as_bytes())
        .graph
}

/// Az `owner::run` célú hívások száma a hívóban.
fn runs(caller: &str, resolved: &str) -> usize {
    calls_of(&graph(), caller)
        .iter()
        .filter(|(t, c, r)| t == "run" && c == "internal_helper" && r.as_deref() == Some(resolved))
        .count()
}

#[test]
fn loop_variable_gets_the_element_type() {
    assert_eq!(runs("grow", "Bar::run"), 1);
    assert_eq!(runs("only_values", "Bar::run"), 1);
    assert_eq!(runs("borrowed", "Item::run"), 1);
    assert_eq!(runs("by_ref", "Item::run"), 1);
    assert_eq!(runs("numbered", "Item::run"), 1);
}

#[test]
fn map_loop_binds_key_and_value() {
    assert_eq!(runs("pairs", "Bar::run"), 1);
    // a kulcs `String` → std-metódus-szerű hívás nem lesz `Bar`-hívás; ismeretlen név marad `?::`
    let pairs = calls_of(&graph(), "pairs");
    assert!(
        pairs.iter().any(|(t, c, r)| t == "shout"
            && c == "native_rust"
            && r.as_deref() == Some("String::shout")),
        "{pairs:?}"
    );
}

#[test]
fn owned_iteration_and_enumerate_index() {
    assert_eq!(runs("owned", "Item::run"), 1);
    assert_eq!(runs("owned", "Bar::run"), 1);
    // az `enumerate` indexe nem kap típust
    let numbered = calls_of(&graph(), "numbered");
    assert!(
        numbered
            .iter()
            .any(|(t, _, r)| t == "shout" && r.as_deref() == Some("?::shout")),
        "{numbered:?}"
    );
}

#[test]
fn unknown_iterables_and_shadowing_stay_untyped() {
    let unknown = calls_of(&graph(), "unknown_loop");
    assert!(
        unknown
            .iter()
            .any(|(t, _, r)| t == "run" && r.as_deref() == Some("?::run")),
        "{unknown:?}"
    );
    // `w` ciklusváltozó és `let w = Bar` is: a két kötés eltér → nincs típus
    let shadowed = calls_of(&graph(), "shadowed");
    assert_eq!(
        shadowed
            .iter()
            .filter(|(t, _, r)| t == "run" && r.as_deref() == Some("?::run"))
            .count(),
        2,
        "{shadowed:?}"
    );
}

#[test]
fn option_and_result_patterns_unwrap() {
    assert_eq!(runs("option_field", "Bar::run"), 1);
    assert_eq!(runs("option_match", "Bar::run"), 1);
    assert_eq!(runs("else_branch", "Bar::run"), 1);
    // `let mut st = stack` átveszi az alakot: `while let Some(top) = st.pop()` → Item; `Ok(b)` → Bar
    assert_eq!(runs("drain_stack", "Item::run"), 1);
    assert_eq!(runs("drain_stack", "Bar::run"), 1);
}

#[test]
fn typed_and_adaptor_closure_parameters() {
    assert_eq!(runs("typed_closure", "Bar::run"), 1);
    // x (map), p (filter), e (fold), t (enumerate + map) → Item; v (values + for_each) → Bar
    assert_eq!(runs("adaptors", "Item::run"), 4);
    assert_eq!(runs("adaptors", "Bar::run"), 1);
    let adaptors = calls_of(&graph(), "adaptors");
    assert!(
        adaptors
            .iter()
            .any(|(t, _, r)| t == "other" && r.as_deref() == Some("Item::other")),
        "{adaptors:?}"
    );
}

#[test]
fn two_own_trait_bounds_pick_the_declaring_trait() {
    let both = calls_of(&graph(), "both");
    let find = |t: &str| both.iter().find(|(x, _, _)| x == t).cloned();
    assert_eq!(
        find("ping"),
        Some((
            "ping".into(),
            "internal_helper".into(),
            Some("Alpha::ping".into())
        ))
    );
    assert_eq!(
        find("pong"),
        Some((
            "pong".into(),
            "internal_helper".into(),
            Some("Beta::pong".into())
        ))
    );
    // egyik trait sem deklarálja → marad ismeretlen
    assert_eq!(
        find("other"),
        Some((
            "other".into(),
            "external_call".into(),
            Some("?::other".into())
        ))
    );
    let where_bound = calls_of(&graph(), "both_where");
    assert!(
        where_bound
            .iter()
            .any(|(t, _, r)| t == "ping" && r.as_deref() == Some("Alpha::ping")),
        "{where_bound:?}"
    );
}

#[test]
fn test_functions_have_kind_test() {
    let g = graph();
    let kind = |n: &str| {
        g.methods
            .iter()
            .find(|m| m.name == n)
            .and_then(|m| m.kind.clone())
    };
    for name in ["adds", "awaits", "fixture", "top_level_test"] {
        assert_eq!(kind(name).as_deref(), Some("test"), "{name}");
    }
    assert_eq!(kind("normal").as_deref(), Some("fn"));
    assert_eq!(kind("grow").as_deref(), Some("method"));
}

#[test]
fn wrappers_are_not_calls_and_closure_calls_are_dynamic() {
    let g = graph();
    let noise = calls_of(&g, "noise");
    for gone in [
        "Some",
        "Ok",
        "Err",
        "Box::new",
        "std::rc::Rc::new",
        "Default::default",
    ] {
        assert!(
            !noise.iter().any(|(t, _, _)| t == gone),
            "{gone}: {noise:?}"
        );
    }
    // a hasznos std-konstruktor megmarad
    assert!(
        noise
            .iter()
            .any(|(t, c, _)| t == "String::new" && c == "native_rust"),
        "{noise:?}"
    );
    for name in ["c", "f"] {
        let call = g
            .calls
            .iter()
            .find(|c| c.caller.as_deref() == Some("noise") && c.target == name);
        assert!(
            call.is_some_and(|c| c.category == "dynamic_call" && c.kind == "dynamic"),
            "{name}: {noise:?}"
        );
    }
}
