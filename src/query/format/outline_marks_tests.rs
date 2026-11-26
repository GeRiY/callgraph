//! Az outline jelölései: `(ref)` hivatkozás-hívásnál, `[test]` / `[tests]`, `wire` / `inject` / `nav` kategória-oszlop.

use super::Formatter;
use super::outline::{Outline, grep_outline};
use crate::i18n::Lang;
use crate::model::{Call, Container, FileGraphBuilder, Method};
use crate::query::QueryResult;
use crate::query::snippet::SourceLines;

fn method(container: &str, name: &str, line: u32, kind: Option<&str>) -> Method {
    Method {
        container: Some(container.to_owned()),
        name: name.to_owned(),
        line,
        end_line: Some(line + 2),
        visibility: None,
        is_static: false,
        is_abstract: false,
        kind: kind.map(str::to_owned),
        params: Vec::new(),
        return_type: None,
        doc_types: None,
        doc: None,
        body: None,
    }
}

fn call(kind: &str, target: &str, category: &str, line: u32, handler: Option<bool>) -> Call {
    Call {
        container: Some("View".to_owned()),
        caller: Some("body".to_owned()),
        line,
        end_line: None,
        kind: kind.to_owned(),
        target: target.to_owned(),
        resolved: None,
        on: None,
        category: category.to_owned(),
        args: None,
        handler,
        from: None,
    }
}

fn container(kind: &str, name: &str, line: u32, end_line: u32) -> Container {
    Container {
        kind: kind.to_owned(),
        name: name.to_owned(),
        line,
        end_line,
        is_abstract: None,
        is_final: None,
        extends: None,
        implements: None,
        traits: None,
        doc: None,
    }
}

fn view(calls: Vec<Call>) -> QueryResult {
    let mut b = FileGraphBuilder::new("src/View.swift");
    b.container(container("class", "View", 1, 30));
    b.method(method("View", "body", 3, None));
    for c in calls {
        b.call(c);
    }
    QueryResult::new(b.build(), None)
}

fn plain(rows: &[QueryResult]) -> String {
    Outline.format(rows, Lang::En)
}

fn grep(rows: &[QueryResult]) -> String {
    grep_outline(rows, Lang::En, &SourceLines::default(), None)
}

#[test]
fn reference_calls_are_marked_in_outline_and_grep() {
    let rows = [view(vec![
        call("method_call", "vm.send", "internal_helper", 5, Some(true)),
        call("reference", "viewModel.refresh", "internal_helper", 6, None),
        call("method_call", "vm.load", "internal_helper", 7, None),
    ])];
    let text = plain(&rows);
    assert!(text.contains("vm.send (ref)"), "{text}");
    assert!(text.contains("viewModel.refresh (ref)"), "{text}");
    assert!(text.contains("vm.load,"), "{text}");
    let found = grep(&rows);
    assert!(found.contains("L5 (ref)"), "{found}");
    assert!(found.contains("L6 (ref)"), "{found}");
    assert!(found.contains("L7\n"), "{found}");
}

#[test]
fn wire_inject_nav_show_the_kind_as_the_column() {
    let rows = [view(vec![
        call(
            "wire",
            "RealtimeSocket::onFrame",
            "internal_helper",
            5,
            None,
        ),
        call("inject", "Store", "swiftui_api", 6, None),
        call("nav", "DetailView", "swiftui_view", 7, None),
    ])];
    let text = plain(&rows);
    assert!(
        text.contains("    wire RealtimeSocket::onFrame\n"),
        "{text}"
    );
    assert!(text.contains("    inject Store\n"), "{text}");
    assert!(text.contains("    nav DetailView\n"), "{text}");
}

#[test]
fn test_functions_and_test_modules_are_marked() {
    let mut b = FileGraphBuilder::new("src/lib.rs");
    b.container(container("module", "tests", 1, 40));
    for (i, name) in ["a", "b", "c"].iter().enumerate() {
        b.method(method("tests", name, 3 + 5 * i as u32, Some("test")));
    }
    let text = plain(&[QueryResult::new(b.build(), None)]);
    assert!(text.contains("module tests [tests]  L1-40\n"), "{text}");
    assert!(text.contains("  3 tests  L3-15\n"), "{text}");
    assert!(!text.contains("a()"), "{text}");

    // kevés teszt: a tagok maradnak, a `[test]` a sor végén
    let mut b = FileGraphBuilder::new("src/lib.rs");
    b.container(container("module", "tests", 1, 10));
    b.method(method("tests", "only", 3, Some("test")));
    b.method(method("tests", "helper", 6, None));
    let text = plain(&[QueryResult::new(b.build(), None)]);
    assert!(text.contains("  only()  L3-5 [test]\n"), "{text}");
    assert!(text.contains("  helper()  L6-8\n"), "{text}");
    assert!(text.contains("module tests  L1-10\n"), "{text}");
}

#[test]
fn go_resolved_targets_are_shown_short_in_the_outline() {
    let go_call = |target: &str, resolved: &str, category: &str, container: &str| {
        let mut c = call("method_call", target, category, 5, None);
        c.container = Some(container.to_owned());
        c.resolved = Some(resolved.to_owned());
        c
    };
    let mut b = FileGraphBuilder::new("src/command.go");
    b.namespace_if_unset(Some("cobra".to_owned()));
    b.container(container("class", "Command", 1, 30));
    b.method(method("Command", "body", 3, None));
    // the own receiver: just the method name; a typed local / param of another type: `Type::method`
    b.call(go_call(
        "c.Flags",
        "cobra\\Command::Flags",
        "internal_helper",
        "Command",
    ));
    b.call(go_call(
        "d.Name",
        "cobra\\Other::Name",
        "external_call",
        "Command",
    ));
    let text = plain(&[QueryResult::new(b.build(), None)]);
    assert!(text.contains("internal_helper Flags\n"), "{text}");
    assert!(text.contains("external_call Other::Name\n"), "{text}");
    assert!(!text.contains("cobra\\"), "{text}");
}
