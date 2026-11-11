//! A típus-hierarchia és a „via” illesztés tesztjei.

use super::*;
use crate::model::{Container, FileGraphBuilder, Method};

fn container(name: &str, extends: &[&str], implements: &[&str]) -> Container {
    Container {
        kind: "class".into(),
        name: name.into(),
        line: 1,
        end_line: 1,
        is_abstract: None,
        is_final: None,
        extends: Some(extends.iter().map(|s| (*s).to_owned()).collect()),
        implements: Some(implements.iter().map(|s| (*s).to_owned()).collect()),
        traits: None,
        doc: None,
    }
}

fn method(container: &str, name: &str) -> Method {
    Method {
        container: Some(container.into()),
        name: name.into(),
        line: 2,
        end_line: None,
        visibility: None,
        is_static: false,
        is_abstract: false,
        kind: None,
        params: Vec::new(),
        return_type: None,
        doc: None,
        doc_types: None,
        body: None,
    }
}

fn graph(ns: &str, containers: Vec<Container>, methods: Vec<Method>) -> FileGraph {
    let mut b = FileGraphBuilder::new("x.php");
    b.namespace_if_unset(Some(ns.to_owned()));
    for c in containers {
        b.container(c);
    }
    for m in methods {
        b.method(m);
    }
    b.build()
}

fn hierarchy() -> TypeHierarchy {
    let graphs = [
        graph(
            "App\\Contracts",
            vec![container("Resolver", &[], &[])],
            vec![method("Resolver", "getPlayables")],
        ),
        graph(
            "App\\Services",
            vec![
                container("PlaylistResolver", &["App\\Services\\BaseResolver"], &[]),
                container("BaseResolver", &[], &["App\\Contracts\\Resolver"]),
                container("SmartResolver", &["App\\Services\\PlaylistResolver"], &[]),
                container("Other", &[], &["App\\Contracts\\Resolver"]),
            ],
            vec![
                method("PlaylistResolver", "getPlayables"),
                method("Other", "getPlayables"),
            ],
        ),
    ];
    TypeHierarchy::from_graphs(graphs.iter())
}

#[test]
fn names_compare_on_segment_boundaries() {
    assert!(same_type("app::contracts::resolver", "resolver"));
    assert!(same_type("std::fmt::display", "fmt::display"));
    assert!(!same_type("app::myresolver", "resolver"));
    assert!(!same_type("a::resolver", "b::resolver"));
    assert_eq!(normalize("\\App\\X"), "app::x");
    assert_eq!(normalize("From<String>"), "from");
    assert_eq!(
        split_target("App\\X::run"),
        Some(("app::x".to_owned(), "run".to_owned()))
    );
}

#[test]
fn ancestors_are_transitive_and_keep_out_of_scope_parents() {
    let h = hierarchy();
    let names: Vec<String> = h
        .ancestors("app::services::playlistresolver")
        .into_iter()
        .map(|(_, label)| label)
        .collect();
    assert_eq!(names, ["BaseResolver", "Resolver"]);
}

#[test]
fn concrete_method_matches_calls_through_the_interface() {
    let h = hierarchy();
    let pattern = "PlaylistResolver::getPlayables$";
    let regex = regex::RegexBuilder::new(pattern)
        .case_insensitive(true)
        .build()
        .unwrap();
    let via = ViaMatcher::new(&h, pattern, &regex).unwrap();
    assert_eq!(
        via.via("App\\Contracts\\Resolver::getPlayables"),
        Some("Resolver")
    );
    assert_eq!(
        via.via("App\\Services\\BaseResolver::getPlayables"),
        Some("BaseResolver")
    );
    // a leszármazott, amely nem definiálja felül
    assert_eq!(
        via.via("App\\Services\\SmartResolver::getPlayables"),
        Some("SmartResolver")
    );
    // testvér-implementáció nem
    assert_eq!(via.via("App\\Services\\Other::getPlayables"), None);
    assert_eq!(via.via("App\\Contracts\\Resolver::other"), None);
}

#[test]
fn interface_method_matches_calls_on_implementations() {
    let h = hierarchy();
    let pattern = "Contracts\\\\Resolver::getPlayables";
    let regex = regex::RegexBuilder::new(pattern)
        .case_insensitive(true)
        .build()
        .unwrap();
    let via = ViaMatcher::new(&h, pattern, &regex).unwrap();
    // BaseResolver nem definiálja → a rá típusos hívás az interfész metódusát éri el
    assert_eq!(
        via.via("App\\Services\\BaseResolver::getPlayables"),
        Some("BaseResolver")
    );
    // PlaylistResolver és Other felüldefiniálja: az ő hívásuk a saját metódusuk
    assert_eq!(via.via("App\\Services\\Other::getPlayables"), None);
}

#[test]
fn unqualified_pattern_needs_no_via() {
    let h = hierarchy();
    let regex = Regex::new("getPlayables").unwrap();
    assert!(ViaMatcher::new(&h, "getPlayables", &regex).is_none());
}
