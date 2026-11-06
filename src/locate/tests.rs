//! A locate egységtesztjei: szavakra bontás, egyes / többes szám, kifejezés-előkészítés, egyezés-fajták, rangsor.

use super::callers::{CallerCounts, Callers};
use super::matcher::{Hit, Kind, Matcher, Symbol};
use super::rank::{FileMatch, rank, score};
use super::reasons::{group, pick};
use super::terms::{Term, same_word, split_member, words};
use crate::model::{Call, Container, FileGraph, Method, StringLit};

fn w(text: &str) -> Vec<String> {
    words(text)
}

#[test]
fn splits_camel_pascal_snake_kebab_and_dots() {
    assert_eq!(w("processRefund"), ["process", "refund"]);
    assert_eq!(w("RefundJob"), ["refund", "job"]);
    assert_eq!(w("refunds_table"), ["refunds", "table"]);
    assert_eq!(w("process-refund"), ["process", "refund"]);
    assert_eq!(w("orders.show"), ["orders", "show"]);
    assert_eq!(w("HTTPServer"), ["http", "server"]);
    assert_eq!(w("App\\Services\\RefundService"), ["app", "services", "refund", "service"]);
    assert_eq!(w("md5Hash"), ["md5", "hash"]);
    assert_eq!(w("PAYMENT_STATUS"), ["payment", "status"]);
    assert!(w("__").is_empty());
}

#[test]
fn plural_tolerance_is_symmetric_and_simple() {
    assert!(same_word("refund", "refunds"));
    assert!(same_word("refunds", "refund"));
    assert!(same_word("status", "statuses"));
    assert!(same_word("category", "categories"));
    assert!(same_word("box", "boxes"));
    assert!(!same_word("refund", "refunded"));
    assert!(!same_word("order", "border"));
    assert!(!same_word("a", "as"), "one-letter words have no plural");
}

#[test]
fn terms_are_trimmed_split_lowercased_and_deduplicated() {
    let terms = Term::parse_all(&["'RefundController'", "refund, Refund  payment_status", ""]);
    let raw: Vec<&str> = terms.iter().map(|t| t.raw.as_str()).collect();
    assert_eq!(raw, ["RefundController", "refund", "payment_status"]);
    assert_eq!(terms[0].lower, "refundcontroller");
    assert_eq!(terms[2].words, ["payment", "status"]);
    let member = Term::new("RefundService::processRefund").unwrap();
    assert_eq!(
        member.member,
        Some(("refundservice".into(), "processrefund".into()))
    );
    let dotted = Term::new("api.refund").unwrap();
    assert_eq!(dotted.member, Some(("api".into(), "refund".into())));
    assert!(Term::new("  \"\" ").is_none());
    assert_eq!(split_member("?::run"), (Some("?"), "run"));
    assert_eq!(split_member("$this->repo->find"), (Some("$this->repo"), "find"));
}

#[test]
fn name_part_needs_every_term_word_and_three_letters() {
    let term = Term::new("refund").unwrap();
    assert!(term.matches_words(&w("RefundJob")));
    assert!(term.matches_words(&w("refunds_table")));
    assert!(term.matches_words(&w("process-refund")));
    assert!(!term.matches_words(&w("Refunded")));
    let two = Term::new("paymentStatus").unwrap();
    assert!(two.matches_words(&w("payment_statuses")));
    assert!(!two.matches_words(&w("payment_method")));
    assert!(!Term::new("id").unwrap().matches_words(&w("user_id")));
}

fn container(name: &str, line: u32, end: u32) -> Container {
    Container {
        kind: "class".into(),
        name: name.into(),
        line,
        end_line: end,
        is_abstract: None,
        is_final: None,
        extends: None,
        implements: None,
        traits: None,
        doc: None,
    }
}

fn method(container: Option<&str>, name: &str, line: u32, end: u32) -> Method {
    Method {
        container: container.map(Into::into),
        name: name.into(),
        line,
        end_line: Some(end),
        visibility: None,
        is_static: false,
        is_abstract: false,
        kind: None,
        params: Vec::new(),
        return_type: None,
        doc: None,
        body: None,
    }
}

fn call(container: &str, caller: &str, line: u32, target: &str) -> Call {
    Call {
        container: Some(container.into()),
        caller: Some(caller.into()),
        line,
        end_line: None,
        kind: "method".into(),
        target: target.into(),
        resolved: None,
        on: None,
        category: "external_call".into(),
        args: None,
        handler: None,
    }
}

fn graph(file: &str) -> FileGraph {
    FileGraph {
        file: file.into(),
        module: None,
        framework: None,
        namespace: None,
        imports: Default::default(),
        containers: Vec::new(),
        properties: Vec::new(),
        constants: Vec::new(),
        methods: Vec::new(),
        calls: Vec::new(),
        comments: Vec::new(),
        control_flow: Vec::new(),
        strings: Vec::new(),
        writes: Vec::new(),
        sfc: None,
        binary: false,
    }
}

#[test]
fn matcher_finds_every_kind_of_match() {
    let mut service = graph("app/Services/RefundService.php");
    service.containers.push(container("RefundService", 5, 80));
    service.methods.push(method(Some("RefundService"), "processRefund", 22, 71));
    let mut controller = graph("app/Http/RefundController.php");
    controller.containers.push(container("RefundController", 12, 140));
    controller
        .calls
        .push(call("RefundController", "store", 44, "$this->service->processRefund"));
    let mut order = graph("app/Models/Order.php");
    order.strings.push(StringLit {
        container: Some("Order".into()),
        caller: None,
        line: 33,
        end_line: None,
        value: "payment_status".into(),
    });
    let terms = Term::parse_all(&["processRefund", "RefundService", "payment_status", "Order"]);
    let callers = CallerCounts::build([&service, &controller, &order]);
    let matcher = Matcher::new(&terms, &callers);

    let hits = matcher.file_hits("app/Services/RefundService.php", &service, true);
    let kinds = |hits: &[Hit], term: usize| -> Vec<Kind> {
        hits.iter().filter(|h| h.term == term).map(|h| h.kind).collect()
    };
    assert!(kinds(&hits, 0).contains(&Kind::Definition), "{hits:?}");
    assert!(kinds(&hits, 1).contains(&Kind::FileName), "{hits:?}");
    assert!(kinds(&hits, 1).contains(&Kind::Definition), "{hits:?}");
    let method_callers = hits.iter().find_map(|h| match &h.symbol {
        Symbol::Method { callers, .. } => Some(callers.clone()),
        _ => None,
    });
    assert_eq!(
        method_callers,
        Some(Callers {
            count: 1,
            sample: vec!["RefundController::store".into()]
        })
    );

    let hits = matcher.file_hits("app/Http/RefundController.php", &controller, true);
    assert_eq!(kinds(&hits, 0), [Kind::Call], "{hits:?}");

    let hits = matcher.file_hits("app/Models/Order.php", &order, true);
    assert_eq!(kinds(&hits, 2), [Kind::StringLit], "{hits:?}");
    assert_eq!(kinds(&hits, 3), [Kind::FileName], "{hits:?}");

    // dokumentum (md, json): csak a fájlneve számít
    let mut doc = graph("docs/refund.md");
    doc.methods.push(method(None, "processRefund", 1, 3));
    let hits = matcher.file_hits("docs/refund.md", &doc, false);
    assert!(hits.is_empty(), "{hits:?}");
}

fn hit(term: usize, kind: Kind) -> Hit {
    Hit {
        term,
        kind,
        symbol: Symbol::Str {
            value: format!("v{term}"),
            line: 1,
        },
    }
}

#[test]
fn distinct_terms_beat_repeated_hits_of_one_term() {
    let repeated: Vec<Hit> = (0..20).map(|_| hit(0, Kind::Definition)).collect();
    let two = vec![hit(0, Kind::Call), hit(1, Kind::Call)];
    assert!(score(&two) > score(&repeated));
}

#[test]
fn a_definition_beats_a_string_and_ties_are_ordered_by_path() {
    let files = vec![
        FileMatch::new("b/strings.php".into(), vec![hit(0, Kind::StringLit)]).unwrap(),
        FileMatch::new("c/def.php".into(), vec![hit(0, Kind::Definition)]).unwrap(),
        FileMatch::new("a/strings.php".into(), vec![hit(0, Kind::StringLit)]).unwrap(),
    ];
    let order: Vec<String> = rank(files).into_iter().map(|f| f.path).collect();
    assert_eq!(order, ["c/def.php", "a/strings.php", "b/strings.php"]);
    // a sok string-ismétlés sem éri utol a definíciót
    let many: Vec<Hit> = (0..50).map(|_| hit(0, Kind::StringLit)).collect();
    assert!(score(&[hit(0, Kind::Definition)]) > score(&many));
    assert!(FileMatch::new("x".into(), Vec::new()).is_none());
}

#[test]
fn reasons_merge_lines_and_prefer_new_terms() {
    let same = |line| Hit {
        term: 0,
        kind: Kind::StringLit,
        symbol: Symbol::Str {
            value: "payment_status".into(),
            line,
        },
    };
    let mut hits = vec![same(57), same(33)];
    hits.extend((0..4).map(|i| Hit {
        term: 0,
        kind: Kind::Call,
        symbol: Symbol::Call {
            caller: None,
            target: format!("t{i}"),
            line: 10 + i,
        },
    }));
    hits.push(hit(1, Kind::Call));
    let reasons = group(&hits);
    assert_eq!(reasons[0].lines, [33, 57]);
    let picked = pick(reasons, 3);
    assert_eq!(picked.len(), 3);
    assert!(picked.iter().any(|r| r.terms.contains(&1)), "{picked:?}");
}
