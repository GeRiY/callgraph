//! A locate egységtesztjei: szavakra bontás, egyes / többes szám, kifejezés-előkészítés, egyezés-fajták, rangsor.

use super::callers::{CallerCounts, Callers};
use super::matcher::{Hit, Kind, Matcher, Symbol};
use super::rank::{FileMatch, rank, score};
use super::reasons::{group, pick};
use super::terms::{Term, TermKind, same_word, split_member, words};
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
    assert_eq!(
        w("App\\Services\\RefundService"),
        ["app", "services", "refund", "service"]
    );
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
    // szóköz nélkül vessző mentén bontva (régebben a szóköz is bontott: most egy szóközös elem egyetlen kifejezés)
    let terms = Term::parse_all(&["'RefundController'", "refund,Refund,payment_status", ""]);
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
    assert_eq!(
        split_member("$this->repo->find"),
        (Some("$this->repo"), "find")
    );
}

#[test]
fn a_quoted_phrase_and_a_route_name_are_single_terms() {
    let terms = Term::parse_all(&["Szervező  további\teseményei", "route::events.show"]);
    assert_eq!(terms.len(), 2);
    assert_eq!(terms[0].raw, "Szervező további eseményei");
    assert_eq!(terms[0].lower, "szervező további eseményei");
    assert_eq!(terms[0].kind, TermKind::Phrase);
    // nem bontva, név-részletként nem illeszt
    assert!(terms[0].words.is_empty() && terms[0].member.is_none());
    assert!(!terms[0].matches_words(&w("eseményeim")));
    assert_eq!(terms[1].kind, TermKind::Route("events.show".into()));
    assert!(terms[1].member.is_none());
    assert_eq!(Term::new("x").unwrap().kind, TermKind::Name);
    // a `route::` önmagában név
    assert_eq!(Term::new("route::").unwrap().kind, TermKind::Name);
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
        doc_types: None,
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
        parse_errors: Vec::new(),
    }
}

#[test]
fn matcher_finds_every_kind_of_match() {
    let mut service = graph("app/Services/RefundService.php");
    service.containers.push(container("RefundService", 5, 80));
    service
        .methods
        .push(method(Some("RefundService"), "processRefund", 22, 71));
    let mut controller = graph("app/Http/RefundController.php");
    controller
        .containers
        .push(container("RefundController", 12, 140));
    controller.calls.push(call(
        "RefundController",
        "store",
        44,
        "$this->service->processRefund",
    ));
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
        hits.iter()
            .filter(|h| h.term == term)
            .map(|h| h.kind)
            .collect()
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

fn json_key(name: &str, line: u32) -> (Kind, Symbol) {
    let symbol = Symbol::Key {
        name: name.into(),
        line,
    };
    (Kind::Definition, symbol)
}

const PACKAGE_JSON: &[u8] = br#"{"scripts": {"build": "vite build"},
"devDependencies": {"laravel-vite-plugin": "^1.0"}}
"#;

#[test]
fn json_keys_match_exactly_by_full_path_or_last_segment() {
    use crate::lang::Language;
    use crate::lang::json::JsonDocument;
    let file = crate::core::SourceFile::new("package.json", PACKAGE_JSON.to_vec(), None);
    let json = JsonDocument.analyze(&file).unwrap().graph;
    let terms = Term::parse_all(&["laravel-vite-plugin", "scripts.build", "vite", "laravel"]);
    let callers = CallerCounts::build([&json]);
    let matcher = Matcher::new(&terms, &callers);
    let hits = matcher.file_hits("package.json", &json, false);
    let found = |term: usize| -> Vec<(Kind, Symbol)> {
        hits.iter()
            .filter(|h| h.term == term)
            .map(|h| (h.kind, h.symbol.clone()))
            .collect()
    };
    assert_eq!(
        found(0),
        [json_key("devDependencies.laravel-vite-plugin", 2)]
    );
    assert_eq!(found(1), [json_key("scripts.build", 1)]);
    // csak pontos egyezés: a név-részlet és az érték nem illeszkedik
    assert!(found(2).is_empty(), "{hits:?}");
    assert!(found(3).is_empty(), "{hits:?}");
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
    // azonos fajtánál a két különböző kifejezés többet ér, mint egy kifejezés sok találata
    let repeated: Vec<Hit> = (0..20).map(|_| hit(0, Kind::NamePart)).collect();
    let two = vec![hit(0, Kind::NamePart), hit(1, Kind::NamePart)];
    assert!(score(&two) > score(&repeated));
}

#[test]
fn an_exact_definition_beats_weak_matches_of_common_terms_in_tests() {
    // a koel-eset: egy osztály pontos definíciója ne maradjon le a több, általános kifejezésre csak
    // név-részlettel illő tesztfájlok mögött (ritkaság + teszt-szorzó)
    let mut files = vec![
        FileMatch::new(
            "app/Http/Controllers/PlaylistController.php".into(),
            vec![hit(0, Kind::Definition), hit(2, Kind::NamePart)],
        )
        .unwrap(),
    ];
    for n in 0..6 {
        files.push(
            FileMatch::new(
                format!("tests/Feature/Playlist{n}Test.php"),
                vec![
                    hit(1, Kind::NamePart),
                    hit(2, Kind::NamePart),
                    hit(3, Kind::NamePart),
                ],
            )
            .unwrap(),
        );
    }
    for n in 0..20 {
        files.push(
            FileMatch::new(format!("app/Other{n}.php"), vec![hit(2, Kind::NamePart)]).unwrap(),
        );
    }
    let ranked = rank(files, super::rank::is_test_path);
    assert_eq!(
        ranked[0].path,
        "app/Http/Controllers/PlaylistController.php"
    );
}

#[test]
fn test_paths_are_recognised() {
    for path in [
        "tests/Feature/PlaylistSongTest.php",
        "src/__tests__/store.ts",
        "resources/js/components/Foo.spec.ts",
        "lib/util.test.js",
        "crates/core/src/lib_test.rs",
        "App/AppTests.swift",
        "internal/repo/repo_test.go",
        "packages\\a\\test\\x.php",
    ] {
        assert!(super::rank::is_test_path(path), "{path}");
    }
    for path in [
        "app/Services/TestingService.php",
        "src/contest.ts",
        "app/Http/Controllers/A.php",
        "internal/repo/repo.go",
    ] {
        assert!(!super::rank::is_test_path(path), "{path}");
    }
}

#[test]
fn a_definition_beats_a_string_and_ties_are_ordered_by_path() {
    let files = vec![
        FileMatch::new("b/strings.php".into(), vec![hit(0, Kind::StringLit)]).unwrap(),
        FileMatch::new("c/def.php".into(), vec![hit(0, Kind::Definition)]).unwrap(),
        FileMatch::new("a/strings.php".into(), vec![hit(0, Kind::StringLit)]).unwrap(),
    ];
    let order: Vec<String> = rank(files, super::rank::is_test_path)
        .into_iter()
        .map(|f| f.path)
        .collect();
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

fn string(line: u32, value: &str) -> StringLit {
    StringLit {
        container: None,
        caller: None,
        line,
        end_line: None,
        value: value.into(),
    }
}

#[test]
fn route_terms_match_equal_strings_and_the_definition_in_routes() {
    let mut routes = graph("routes/web.php");
    routes.strings.push(string(5, "/events/{e}"));
    routes.strings.push(string(5, "events.show"));
    routes.strings.push(string(6, "events.show.extra"));
    let mut controller = graph("app/Http/Controllers/EventController.php");
    controller.strings.push(string(8, "events.show"));
    let mut nav = graph("resources/js/nav.js");
    nav.calls
        .push(call("nav", "goEvent", 2, "route::events.show"));
    let mut other = graph("app/Show.php");
    other.containers.push(container("EventsShow", 1, 9));
    let terms = Term::parse_all(&["route::events.show"]);
    let callers = CallerCounts::build([&routes, &controller, &nav, &other]);
    let matcher = Matcher::new(&terms, &callers);
    let kinds = |rel: &str, graph: &FileGraph| -> Vec<Kind> {
        matcher
            .file_hits(rel, graph, true)
            .iter()
            .map(|h| h.kind)
            .collect()
    };
    assert_eq!(kinds("routes/web.php", &routes), [Kind::Definition]);
    assert_eq!(
        kinds("app/Http/Controllers/EventController.php", &controller),
        [Kind::StringLit]
    );
    assert_eq!(kinds("resources/js/nav.js", &nav), [Kind::Call]);
    // név-részlet / fájlnév nem illeszt
    assert!(kinds("app/Show.php", &other).is_empty());
}

#[test]
fn a_phrase_matches_strings_as_a_whole() {
    let mut lang = graph("lang/hu.php");
    lang.strings
        .push(string(3, "Szervező\n  további eseményei"));
    lang.strings.push(string(4, "Eseményeim"));
    let mut vue = graph("resources/js/Pages/Events.vue");
    vue.containers.push(container("Eseményei", 1, 9));
    let terms = Term::parse_all(&["szervező további eseményei", "eseményei"]);
    let callers = CallerCounts::build([&lang, &vue]);
    let matcher = Matcher::new(&terms, &callers);
    let hits = matcher.file_hits("lang/hu.php", &lang, true);
    let phrase: Vec<u32> = hits
        .iter()
        .filter(|h| h.term == 0)
        .filter_map(|h| match &h.symbol {
            Symbol::Str { line, .. } => Some(*line),
            _ => None,
        })
        .collect();
    assert_eq!(phrase, [3]);
    // a több szavas kifejezés definícióra nem illeszt
    let hits = matcher.file_hits("resources/js/Pages/Events.vue", &vue, true);
    assert!(hits.iter().all(|h| h.term == 1), "{hits:?}");
}

#[test]
fn exact_matches_rank_before_files_matching_more_terms_by_strings() {
    // a „Szervező további eseményei” + EventDetailContent eset: a sok kifejezésre stringgel illő fordítási /
    // build-fájlok se előzzék meg a komponenst
    let mut files = vec![
        FileMatch::new(
            "resources/js/Components/EventDetailContent.vue".into(),
            vec![hit(1, Kind::FileName), hit(1, Kind::Definition)],
        )
        .unwrap(),
    ];
    for n in 0..4 {
        files.push(
            FileMatch::new(
                format!("lang/l{n}.php"),
                vec![
                    hit(0, Kind::StringLit),
                    hit(1, Kind::StringLit),
                    hit(2, Kind::StringLit),
                    hit(3, Kind::StringLit),
                ],
            )
            .unwrap(),
        );
    }
    let ranked = rank(files, super::rank::is_test_path);
    assert_eq!(
        ranked[0].path,
        "resources/js/Components/EventDetailContent.vue"
    );
    // a többi pontszám szerint, út szerint holtversenyben
    let rest: Vec<&str> = ranked[1..].iter().map(|f| f.path.as_str()).collect();
    assert_eq!(
        rest,
        ["lang/l0.php", "lang/l1.php", "lang/l2.php", "lang/l3.php"]
    );
}

#[test]
fn an_identifier_joining_terms_counts_its_independent_terms() {
    let terms = Term::parse_all(&["favorite", "podcast", "episode", "toggle", "favorites"]);
    let callers = CallerCounts::build(std::iter::empty::<&FileGraph>());
    let matcher = Matcher::new(&terms, &callers);
    // `toggleFavorite`: toggle + favorite (a `favorites` ugyanaz a szó, nem számít külön)
    let mut service = graph("app/Services/FavoriteService.php");
    service.containers.push(container("FavoriteService", 3, 30));
    service
        .methods
        .push(method(Some("FavoriteService"), "toggleFavorite", 5, 20));
    let hits = matcher.file_hits("app/Services/FavoriteService.php", &service, true);
    assert_eq!(matcher.multi_terms(&hits), 2, "{hits:?}");
    // a fájlnév is egyesíthet: `PodcastEpisodeController`
    let controller = graph("app/Http/PodcastEpisodeController.php");
    let hits = matcher.file_hits("app/Http/PodcastEpisodeController.php", &controller, true);
    assert_eq!(matcher.multi_terms(&hits), 2, "{hits:?}");
    // külön azonosítókban, illetve stringben a kifejezések nem egyesülnek
    let mut model = graph("app/Models/Podcast.php");
    model.containers.push(container("Podcast", 3, 30));
    model
        .methods
        .push(method(Some("Podcast"), "episodes", 5, 8));
    model.strings.push(StringLit {
        container: Some("Podcast".into()),
        caller: None,
        line: 9,
        end_line: None,
        value: "favorite toggle".into(),
    });
    let hits = matcher.file_hits("app/Models/Podcast.php", &model, true);
    assert_eq!(matcher.multi_terms(&hits), 1, "{hits:?}");
    // a bővebb kifejezés lefedi a szűkebbet: `RefundController` + `refund` egy kifejezésnyi
    let terms = Term::parse_all(&["RefundController", "refund"]);
    let matcher = Matcher::new(&terms, &callers);
    let mut refund = graph("app/Http/RefundController.php");
    refund.containers.push(container("RefundController", 3, 30));
    let hits = matcher.file_hits("app/Http/RefundController.php", &refund, true);
    assert_eq!(matcher.multi_terms(&hits), 1, "{hits:?}");
}

fn symbol_hit(term: usize, kind: Kind, name: &str) -> Hit {
    Hit {
        term,
        kind,
        symbol: Symbol::Container {
            kind: "class".into(),
            name: name.into(),
            line: 1,
            end: 9,
        },
    }
}

#[test]
fn a_multi_term_identifier_ranks_before_declarations_and_single_exact_terms() {
    let declaration_hits = vec![
        symbol_hit(0, Kind::Definition, "Favorite"),
        symbol_hit(1, Kind::Definition, "Podcast"),
        symbol_hit(2, Kind::Definition, "Episode"),
    ];
    let mut files = vec![
        FileMatch::new("resources/js/types.d.ts".into(), declaration_hits).unwrap(),
        FileMatch::new(
            "app/Models/Podcast.php".into(),
            vec![
                symbol_hit(1, Kind::Definition, "Podcast"),
                hit(0, Kind::StringLit),
            ],
        )
        .unwrap(),
        FileMatch::new(
            "app/Services/FavoriteService.php".into(),
            vec![
                symbol_hit(0, Kind::NamePart, "toggleFavorite"),
                symbol_hit(3, Kind::NamePart, "toggleFavorite"),
            ],
        )
        .unwrap()
        .with_multi(2),
    ];
    for n in 0..10 {
        files.push(
            FileMatch::new(
                format!("app/Other{n}.php"),
                vec![hit(0, Kind::StringLit), hit(3, Kind::StringLit)],
            )
            .unwrap(),
        );
    }
    let ranked = rank(files, super::rank::is_test_path);
    let order: Vec<&str> = ranked.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(order[0], "app/Services/FavoriteService.php", "{order:?}");
    assert_eq!(order[1], "app/Models/Podcast.php", "{order:?}");
    // a deklarációs fájl definíciói nem emelik a pontos szintre
    let at = |path: &str| order.iter().position(|p| *p == path).unwrap();
    assert!(at("resources/js/types.d.ts") > 1, "{order:?}");
    // a több kifejezés többet ér: három kifejezés > kettő
    let two = FileMatch::new("a.php".into(), vec![hit(0, Kind::NamePart)])
        .unwrap()
        .with_multi(2);
    let three = FileMatch::new("b.php".into(), vec![hit(0, Kind::NamePart)])
        .unwrap()
        .with_multi(3);
    let ranked = rank(vec![two, three], super::rank::is_test_path);
    assert_eq!(ranked[0].path, "b.php");
}

#[test]
fn declaration_paths_are_recognised() {
    use super::rank::is_declaration_path;
    for path in [
        "resources/js/types.d.ts",
        "src\\Global.D.TS",
        "x.d.mts",
        "stubs/a.pyi",
    ] {
        assert!(is_declaration_path(path), "{path}");
    }
    for path in ["src/types.ts", "src/d.ts.vue", "app.py"] {
        assert!(!is_declaration_path(path), "{path}");
    }
}
