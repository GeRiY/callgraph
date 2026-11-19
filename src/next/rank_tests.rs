//! A rangsoroló: kevés adatnál a szabály-sorrend, visszalépéses fajta lesüllyed, küszöb → nincs sor, kikapcsolt
//! tanulás → változatlan, a kiértékelés visszajátszása, a forgatott napló és a sérült sorok.

use std::path::PathBuf;

use super::log::{EventLog, LogEvent};
use super::rank::{Model, apply_with, evaluate, render};
use super::suggestion::{Suggestion, SuggestionKind};

use SuggestionKind::{Body, Callers, Outline};

fn dir(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let dir = std::env::temp_dir().join(format!(
        "callgraph-rank-{name}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn shown(id: u64, kinds: &[SuggestionKind]) -> LogEvent {
    LogEvent::Shown {
        t: id,
        id,
        cmd: "locate".into(),
        kinds: kinds.to_vec(),
    }
}

fn accepted(id: u64, choice: usize, kind: SuggestionKind) -> LogEvent {
    LogEvent::Accepted {
        t: id,
        id,
        choice,
        kind,
        subj: Vec::new(),
        sig: String::new(),
    }
}

fn step(backtrack: bool) -> LogEvent {
    LogEvent::Step { t: 0, backtrack }
}

fn suggestions(kinds: &[SuggestionKind]) -> Vec<Suggestion> {
    kinds
        .iter()
        .enumerate()
        .map(|(at, &kind)| Suggestion {
            kind,
            args: vec![format!("a{at}")],
            label: format!("l{at}"),
        })
        .collect()
}

fn kinds(found: &[Suggestion]) -> Vec<SuggestionKind> {
    found.iter().map(|s| s.kind).collect()
}

const RULE: [SuggestionKind; 3] = [Outline, Body, Callers];

/// `n` megjelenés; mindegyiknél a `pick` helyen lévőt fogadja el, `bad` esetén visszalépéssel.
fn history(n: u64, pick: usize, bad: bool) -> Vec<LogEvent> {
    let mut events = Vec::new();
    for id in 1..=n {
        events.push(shown(id, &RULE));
        events.push(accepted(id, pick, RULE[pick - 1]));
        events.push(step(bad));
    }
    events
}

#[test]
fn empty_and_tiny_logs_keep_the_rule_order() {
    let found = Model::from_events(&[]).rank("locate", suggestions(&RULE));
    assert_eq!(kinds(&found), RULE);
    // 9 megjelenés (< MIN_EVIDENCE): az adat még nem számít, hiába fogad el mindig a 3.-at
    let tiny = history(9, 3, false);
    let found = Model::from_events(&tiny).rank("locate", suggestions(&RULE));
    assert_eq!(kinds(&found), RULE);
}

#[test]
fn a_kind_that_is_accepted_without_backtrack_moves_up() {
    let model = Model::from_events(&history(40, 3, false));
    let found = model.rank("locate", suggestions(&RULE));
    assert_eq!(found[0].kind, Callers);
    // a címke / argumentum együtt mozog a fajtával
    assert_eq!(found[0].label, "l2");
}

#[test]
fn a_kind_with_many_backtracks_drops_below_another() {
    // az 1. hely (Outline) mindig visszalépéssel; a 2. (Body) sosem kell
    let model = Model::from_events(&history(40, 1, true));
    let found = model.rank("locate", suggestions(&RULE));
    assert_eq!(found[0].kind, Body);
}

#[test]
fn clearly_negative_kind_is_dropped_and_no_candidate_means_no_line() {
    let model = Model::from_events(&history(40, 1, true));
    let found = model.rank("locate", suggestions(&RULE));
    assert!(!kinds(&found).contains(&Outline), "{found:?}");
    // csak az eldobandó jelölt: üres
    let only = model.rank("locate", suggestions(&[Outline]));
    assert!(only.is_empty());
}

#[test]
fn learning_off_is_identical_to_the_rule_candidates() {
    let d = dir("off");
    let log = EventLog::new(&d);
    for e in history(40, 1, true) {
        log.append(&e);
    }
    let rule = suggestions(&RULE);
    assert_eq!(apply_with(false, &d, "locate", rule.clone()), rule);
    let learned = apply_with(true, &d, "locate", rule.clone());
    assert_ne!(learned, rule);
    // üres napló / hiányzó mappa: változatlan akkor is, ha be van kapcsolva
    let empty = dir("empty");
    assert_eq!(apply_with(true, &empty, "locate", rule.clone()), rule);
}

#[test]
fn rotated_log_is_read_before_the_current_one_and_bad_lines_are_skipped() {
    let d = dir("rot");
    let line = |e: &LogEvent| format!("{}\n", serde_json::to_string(e).unwrap());
    let old: String = history(20, 3, false).iter().map(line).collect();
    let mut new: String = history(20, 3, false)
        .into_iter()
        .map(|mut e| {
            if let LogEvent::Shown { id, .. } | LogEvent::Accepted { id, .. } = &mut e {
                *id += 100;
            }
            e
        })
        .map(|e| line(&e))
        .collect();
    new.push_str("not json\n{\"ev\":\"unknown\"}\n");
    std::fs::write(d.join("log.1.jsonl"), old).unwrap();
    std::fs::write(d.join("log.jsonl"), new).unwrap();
    let events = EventLog::new(&d).read_all();
    assert_eq!(events.len(), 120);
    // az első esemény a forgatott fájlból való
    assert!(matches!(events[0], LogEvent::Shown { id: 1, .. }));
    // a két fájl együtt 40 megjelenés: a 3. hely előre kerül
    let found = apply_with(true, &d, "locate", suggestions(&RULE));
    assert_eq!(found[0].kind, Callers);
}

#[test]
fn eval_replays_chronologically_with_only_earlier_events() {
    // 12 megjelenés, mindig a 3. hely elfogadva visszalépés nélkül. A tanuló a 10 megjelenés után kezd hatni:
    // az 1-10. megjelenésnél még a naplózott sorrend (3. hely), a 11-12.-nél a tanult (1. hely).
    let events = history(12, 3, false);
    let eval = evaluate(&events);
    assert_eq!(eval.events, 36);
    assert_eq!(eval.shown, 12);
    assert_eq!(eval.accepted, 12);
    assert_eq!(eval.hit_logged, 0);
    assert_eq!(eval.hit_learned, 2);
    assert_eq!(eval.backtracks, 0);
    let third = 1.0 / 3.0;
    assert!((eval.rr_logged - 12.0 * third).abs() < 1e-9);
    assert!((eval.rr_learned - (10.0 * third + 2.0)).abs() < 1e-9);
}

#[test]
fn eval_counts_backtracks_and_reports_little_data() {
    let mut events = history(3, 1, false);
    events.extend(history(3, 1, true).into_iter().map(|mut e| {
        if let LogEvent::Shown { id, .. } | LogEvent::Accepted { id, .. } = &mut e {
            *id += 10;
        }
        e
    }));
    let eval = evaluate(&events);
    assert_eq!(eval.accepted, 6);
    assert_eq!(eval.backtracks, 3);
    assert_eq!(eval.hit_logged, 6);
    let text = render(&eval);
    assert!(text.lines().count() <= 8, "{text}");
    assert!(text.contains("50.0%"), "{text}");
    assert!(text.contains("30"), "{text}");
    // elég adatnál nincs figyelmeztetés
    let long = render(&evaluate(&history(30, 1, false)));
    assert!(
        !long.contains("too little") && !long.contains("kevés az adat"),
        "{long}"
    );
    // üres napló: nem omlik össze
    assert!(render(&evaluate(&[])).contains("n/a"));
}
