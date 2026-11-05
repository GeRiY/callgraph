//! Fogadó-típus következtetés, `?::` (feloldatlan fogadó), konstans-hivatkozások és const-stringek.

use super::RustBackend;
use super::tests::calls_of;
use crate::model::FileGraph;

const SRC: &str = r#"use crate::db::Repo;

pub const LIMIT: usize = 3;
static GREETING: &str = "hello there";
const RAW: &[u8] = b"raw bytes here";

pub struct Service {
    repo: Repo,
    cache: Box<Cache>,
}

pub struct Cache;

impl Cache {
    pub const TTL: u64 = 60;

    pub fn warm(&self) {}

    pub fn len(&self) -> usize {
        0
    }
}

impl Service {
    pub fn new(repo: Repo) -> Self {
        Self { repo, cache: Box::new(Cache) }
    }

    pub fn run(&self, other: &Cache, names: Vec<String>) -> u64 {
        self.repo.load(LIMIT);
        self.cache.warm();
        other.warm();
        let fresh = Cache {};
        fresh.warm();
        let total = other.len() + names.len();
        unknown().spin();
        for item in names {
            item.spin();
        }
        Cache::TTL + u64::MAX + total as u64
    }
}
"#;

fn graph() -> FileGraph {
    RustBackend::with_defaults()
        .analyze_source("/p/src/svc.rs", SRC.as_bytes())
        .graph
}

fn has(
    calls: &[(String, String, Option<String>)],
    target: &str,
    cat: &str,
    res: Option<&str>,
) -> bool {
    calls
        .iter()
        .any(|(t, c, r)| t == target && c == cat && r.as_deref() == res)
}

#[test]
fn receiver_types_resolve_method_calls() {
    let run = calls_of(&graph(), "run");
    // `self.repo` mező-típusa: a `crate::db::Repo`
    assert!(
        has(&run, "load", "module_use", Some("Repo::load")),
        "{run:?}"
    );
    // `Box<Cache>` mező, `&Cache` paraméter, `Cache {}` literál → a fájl saját típusa
    let warm = run
        .iter()
        .filter(|(t, c, r)| {
            t == "warm" && c == "internal_helper" && r.as_deref() == Some("Cache::warm")
        })
        .count();
    assert_eq!(warm, 3, "{run:?}");
    // std-szerű név: a saját típusnál a saját metódus, máskor marad `native_rust`
    assert!(
        has(&run, "len", "internal_helper", Some("Cache::len")),
        "{run:?}"
    );
    assert!(has(&run, "len", "native_rust", None), "{run:?}");
    // ismeretlen fogadó: `?::spin` (a `for item` kötés miatt az `item` sem kap típust)
    let spin = run
        .iter()
        .filter(|(t, c, r)| t == "spin" && c == "external_call" && r.as_deref() == Some("?::spin"))
        .count();
    assert_eq!(spin, 2, "{run:?}");
}

#[test]
fn const_references_are_const_fetch_calls() {
    let g = graph();
    let consts: Vec<(&str, &str, Option<&str>)> = g
        .calls
        .iter()
        .filter(|c| c.kind == "const_fetch")
        .map(|c| {
            (
                c.target.as_str(),
                c.category.as_str(),
                c.resolved.as_deref(),
            )
        })
        .collect();
    assert!(
        consts.contains(&("LIMIT", "internal_helper", Some("svc::LIMIT"))),
        "{consts:?}"
    );
    assert!(
        consts.contains(&("Cache::TTL", "internal_helper", None))
            || consts.contains(&("Cache::TTL", "internal_helper", Some("Cache::TTL"))),
        "{consts:?}"
    );
    assert!(
        consts
            .iter()
            .any(|(t, c, _)| *t == "u64::MAX" && *c == "native_rust"),
        "{consts:?}"
    );
    // a hívott út (`Box::new`, `Cache`) nem konstans
    assert_eq!(consts.len(), 3, "{consts:?}");
}

#[test]
fn const_and_static_initializer_strings() {
    let g = graph();
    let strings: Vec<(&str, &str, u32)> = g
        .strings
        .iter()
        .map(|s| (s.value.as_str(), s.caller.as_deref().unwrap_or(""), s.line))
        .collect();
    assert!(
        strings.contains(&("hello there", "__top__", 4)),
        "{strings:?}"
    );
    assert!(
        strings.contains(&("raw bytes here", "__top__", 5)),
        "{strings:?}"
    );
}
