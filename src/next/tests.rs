//! A javaslat-szabályok (pozitív és „nincs sor” esetek), a tároló (növekvő id, 50-es korlát, lejárat, párhuzamos
//! írás), a `next` értelmezése, a napló visszalépés-jele és a kikapcsolt alapállapot.

use std::path::{Path, PathBuf};

use super::invocation::Invocation;
use super::log::{EventLog, LogEvent};
use super::rules::suggest;
use super::run::{line, parse_choices, parse_id};
use super::store::{CAPACITY, EXPIRY_SECS, ResultStore};
use super::suggestion::{Suggestion, SuggestionKind};

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| (*x).to_owned()).collect()
}

fn temp_dir(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let dir = std::env::temp_dir().join(format!(
        "callgraph-next-{name}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn no_layer(_: &str) -> Option<String> {
    None
}

fn run(args: &[&str], output: &str) -> Vec<Suggestion> {
    let invocation = Invocation::parse(&s(args)).unwrap();
    suggest(&invocation, Path::new("/p"), output, &no_layer)
}

fn kinds(found: &[Suggestion]) -> Vec<SuggestionKind> {
    found.iter().map(|s| s.kind).collect()
}

const LOCATE: &str = "root: /p
locate: 2 terms · 3 matches in 2 files · start here ↓
1. app/OrderService.php  ★★★
   refund()  L10-40  callers: 2 (A::b, C::d)  ← \"refund\" (exact method)
2. app/Other.php  ★
   x()  L3  calls refund  ← \"refund\" (call)
";

#[test]
fn locate_suggests_outline_body_and_callers_of_the_exact_method() {
    let found = run(&["locate", "refund"], LOCATE);
    assert_eq!(
        kinds(&found),
        [
            SuggestionKind::Outline,
            SuggestionKind::Body,
            SuggestionKind::Callers
        ]
    );
    assert_eq!(found[0].args.last().unwrap(), "/p/app/OrderService.php");
    assert_eq!(
        found[1].args.last().unwrap(),
        "/p/app/OrderService.php::refund"
    );
    assert!(found[1].args.contains(&"--body".to_owned()));
    assert!(
        found[1]
            .label
            .contains("app/OrderService.php::refund --body")
    );
    assert!(found[2].args.contains(&r"(^|::|\.|->)refund$".to_owned()));
    // csak név-részlet / hívás: csak az outline
    let partial = LOCATE.replace("(exact method)", "(name part)");
    assert_eq!(
        kinds(&run(&["locate", "refund"], &partial)),
        [SuggestionKind::Outline]
    );
    // nincs találat: nincs sor
    assert!(
        run(
            &["locate", "zzz"],
            "locate: 1 terms · no match in the project map\n"
        )
        .is_empty()
    );
}

const OUTLINE: &str = "FILE  /p/app/Order.php

class Order  L1-90
  short()  L5-8
    native_php        count
  longest()  L10-60
  mid()  L62-80
";

#[test]
fn outline_suggests_the_body_of_the_unique_longest_method() {
    let found = run(&["wrap", "app/Order.php"], OUTLINE);
    assert_eq!(kinds(&found), [SuggestionKind::Body]);
    assert_eq!(found[0].args.last().unwrap(), "/p/app/Order.php::longest");
    // kevesebb mint 3 metódus, holtverseny, metódus-szelektor, --body, gépi formátum: semmi
    let two = OUTLINE.replace("  mid()  L62-80\n", "");
    assert!(run(&["wrap", "app/Order.php"], &two).is_empty());
    let tie = OUTLINE.replace("L62-80", "L62-112");
    assert!(run(&["wrap", "app/Order.php"], &tie).is_empty());
    assert!(run(&["wrap", "app/Order.php::short"], OUTLINE).is_empty());
    assert!(run(&["wrap", "app/Order.php", "--body"], OUTLINE).is_empty());
    for format in ["json", "jsonl", "mermaid", "dot"] {
        assert!(run(&["wrap", "app/Order.php", "--format", format], OUTLINE).is_empty());
    }
}

#[test]
fn a_truncated_output_suggests_the_narrowing_from_the_note() {
    let index = format!(
        "⚠ --format outline output would be 900 lines — showing the method index instead. One method: <file>::<method> [--format outline|flow] or --body; full output: --full.\n\n{OUTLINE}"
    );
    let found = run(&["wrap", "app/Order.php"], &index);
    assert_eq!(kinds(&found), [SuggestionKind::Narrow]);
    assert!(
        found[0]
            .args
            .last()
            .unwrap()
            .ends_with("Order.php::longest")
    );

    let root = temp_dir("summary");
    std::fs::create_dir_all(root.join("app")).unwrap();
    std::fs::write(root.join("app/Big.php"), "<?php").unwrap();
    let summary = "⚠ --format outline output would be 900 lines (2 files) — showing the summary instead. Narrow it down: a subfolder, --grep <pattern>, or a single file; full output: --full.\n\nSmall.php  class Small  (2 methods) — x\nBig.php  class Big  (9 methods) — y\n";
    let invocation = Invocation::parse(&s(&["wrap", "app"])).unwrap();
    let found = suggest(&invocation, &root, summary, &no_layer);
    assert_eq!(kinds(&found), [SuggestionKind::Narrow]);
    assert_eq!(
        found[0].args.last().unwrap(),
        &root.join("app/Big.php").display().to_string()
    );
    // a nem létező fájlt nem javasolja
    let missing = summary.replace("Big.php", "Gone.php");
    assert!(suggest(&invocation, &root, &missing, &no_layer).is_empty());
    let _ = std::fs::remove_dir_all(root);
}

const CALLERS: &str = "root: /p
FILE  app/Http/OrderController.php

class OrderController  L1-50
  store()  L10-30
    L25 │ $this->service->refund($order);

---
FILE  app/Jobs/RefundJob.php

class RefundJob  L1-20
  handle()  L5-15
    L9 │ $s->refund($o);
";

#[test]
fn callers_suggest_depth_the_first_caller_and_by_layer_when_two_layers() {
    let args = ["wrap", "app", "--grep", r"(^|::|\.|->)refund$"];
    let found = run(&args, CALLERS);
    assert_eq!(kinds(&found), [SuggestionKind::Depth, SuggestionKind::Body]);
    assert!(found[0].args.windows(2).any(|w| w == ["--depth", "2"]));
    assert_eq!(
        found[1].args.last().unwrap(),
        "/p/app/Http/OrderController.php::store"
    );
    let layer = |rel: &str| Some(rel.split('/').nth(1).unwrap_or("").to_owned());
    let invocation = Invocation::parse(&s(&args)).unwrap();
    let grouped = suggest(&invocation, Path::new("/p"), CALLERS, &layer);
    assert_eq!(grouped.last().unwrap().kind, SuggestionKind::ByLayer);
    // nincs hívó, már --depth 3, --body: semmi
    assert!(run(&args, "root: /p\n(no match)\n").is_empty());
    let mut deep = args.to_vec();
    deep.extend(["--depth", "3"]);
    assert!(run(&deep, CALLERS).is_empty());
    let mut body = args.to_vec();
    body.push("--body");
    assert!(run(&body, CALLERS).is_empty());
}

const PROFILE: &str = "profile: Laravel layered MVC · confidence: high
layers (files):
  controller    app/Http/Controllers    12
  view (rust)   src/views               30
  model         app/Models               8
  … +3 more layers (profile --full)
patterns: Repository (3)
";

#[test]
fn profile_suggests_the_largest_layer_and_the_full_view() {
    let found = run(&["profile"], PROFILE);
    assert_eq!(
        kinds(&found),
        [SuggestionKind::LayerSummary, SuggestionKind::ProfileFull]
    );
    assert!(
        found[0]
            .args
            .windows(2)
            .any(|w| w == ["--layer", "view:rust"])
    );
    assert_eq!(found[1].args, s(&["profile", "--project", "/p", "--full"]));
    assert!(run(&["profile", "--full"], PROFILE).is_empty());
    let shown = PROFILE.replace("  … +3 more layers (profile --full)\n", "");
    assert_eq!(
        kinds(&run(&["profile"], &shown)),
        [SuggestionKind::LayerSummary]
    );
}

#[test]
fn other_commands_get_no_suggestions() {
    for args in [&["stats"][..], &["note", "list"], &["mcp"], &["refresh"]] {
        assert!(Invocation::parse(&s(args)).is_none(), "{args:?}");
    }
}

#[test]
fn the_line_is_one_neutral_line_with_numbered_choices() {
    let found = run(&["locate", "refund"], LOCATE);
    let text = line(7, &found, None);
    assert!(!text.contains('\n'));
    assert!(text.starts_with("↪ r7 "), "{text}");
    assert!(text.contains("[1] ") && text.contains("[3] "), "{text}");
    assert!(line(7, &found, Some("shop")).ends_with("(next r7 <n> --project shop)"));
}

#[test]
fn parses_ids_and_choices() {
    assert_eq!(parse_id("r7"), Some(7));
    for bad in ["7", "r", "r0", "rx", "r-1"] {
        assert_eq!(parse_id(bad), None, "{bad}");
    }
    assert_eq!(parse_choices("1"), Some(vec![1]));
    assert_eq!(parse_choices("1,3"), Some(vec![1, 3]));
    assert_eq!(parse_choices("3, 1,3"), Some(vec![3, 1]));
    for bad in ["", "0", "4", "a", "1,,2"] {
        assert_eq!(parse_choices(bad), None, "{bad}");
    }
}

fn sample() -> Vec<Suggestion> {
    vec![Suggestion {
        kind: SuggestionKind::Outline,
        args: s(&["show", "--project", "/p", "/p/a.php"]),
        label: "outline a.php".into(),
    }]
}

#[test]
fn store_ids_increase_expire_and_keep_the_last_50() {
    let dir = temp_dir("store");
    let store = ResultStore::at(dir.clone());
    let t = 1_000_000;
    assert_eq!(store.save("locate", None, &sample(), t), Some(1));
    assert_eq!(store.save("show", Some("shop"), &sample(), t), Some(2));
    let got = store.get(2, t + 10).unwrap();
    assert_eq!(
        (got.cmd.as_str(), got.project.as_deref()),
        ("show", Some("shop"))
    );
    assert_eq!(got.s, sample());
    // lejárat; az azonosító a lejárt rekordok után sem kezdődik újra
    assert!(store.get(1, t + EXPIRY_SECS).is_none());
    assert_eq!(
        store.save("show", None, &sample(), t + 2 * EXPIRY_SECS),
        Some(3)
    );
    for n in 0..60 {
        store.save("show", None, &sample(), t + 2 * EXPIRY_SECS + n);
    }
    let now = t + 2 * EXPIRY_SECS + 60;
    assert!(store.get(63, now).is_some());
    assert!(store.get(63 - CAPACITY as u64, now).is_none());
    assert!(store.get(64 - CAPACITY as u64, now).is_some());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn concurrent_saves_get_distinct_ids() {
    let dir = temp_dir("concurrent");
    let handles: Vec<_> = (0..8)
        .map(|_| {
            let store = ResultStore::at(dir.clone());
            std::thread::spawn(move || store.save("show", None, &sample(), 1_000_000))
        })
        .collect();
    let mut ids: Vec<u64> = handles
        .into_iter()
        .filter_map(|h| h.join().unwrap())
        .collect();
    ids.sort_unstable();
    assert_eq!(ids, (1..=8).collect::<Vec<u64>>());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_different_command_on_the_same_file_after_an_accepted_suggestion_is_a_backtrack() {
    let dir = temp_dir("log");
    let log = EventLog::new(&dir);
    let suggested = s(&[
        "show",
        "--project",
        "/p",
        "--body",
        "/p/app/Order.php::refund",
    ]);
    let invocation = Invocation::parse(&suggested).unwrap();
    log.append(&LogEvent::Accepted {
        t: 1,
        id: 7,
        choice: 1,
        kind: SuggestionKind::Body,
        subj: invocation
            .subjects()
            .iter()
            .map(|s| super::log::hash(s))
            .collect(),
        sig: super::log::hash_args(&suggested),
    });
    let other = s(&["wrap", "app/Order.php", "--lines", "1-40"]);
    let other_subjects = Invocation::parse(&other).unwrap().subjects();
    log.observe(&other_subjects, &other, 2);
    log.observe(&other_subjects, &other, 3);
    log.observe(&other_subjects, &other, 4); // az ablakon túl: nem számít
    let steps: Vec<bool> = log
        .tail()
        .into_iter()
        .filter_map(|e| match e {
            LogEvent::Step { backtrack, .. } => Some(backtrack),
            _ => None,
        })
        .collect();
    assert_eq!(steps, [true, true]);
    // a napló nem tartalmaz utat / nevet
    let text = std::fs::read_to_string(dir.join("log.jsonl")).unwrap();
    assert!(!text.contains("Order") && !text.contains('/'), "{text}");
    let _ = std::fs::remove_dir_all(dir);
}

/// Egy parancs, ami csak kiír valamit.
struct Echo;

impl crate::cli::Command for Echo {
    fn run(
        &self,
        out: &mut dyn std::io::Write,
        _err: &mut dyn std::io::Write,
    ) -> crate::error::Result<i32> {
        out.write_all(LOCATE.as_bytes())?;
        Ok(0)
    }
}

#[test]
fn off_by_default_the_command_runs_unchanged() {
    // a tesztek nem állítják a CALLGRAPH_NEXT-et: alapból nincs becsomagolás, nincs sor
    assert!(std::env::var(super::NEXT_ENV).is_err());
    let command = super::decorate(&s(&["locate", "refund"]), Box::new(Echo));
    let (mut out, mut err) = (Vec::new(), Vec::new());
    command.run(&mut out, &mut err).unwrap();
    assert_eq!(String::from_utf8(out).unwrap(), LOCATE);
    assert!(super::is_on("1") && super::is_on(" on ") && !super::is_on("0"));
    let mut args = s(&["--next", "locate", "x", "--", "--next"]);
    assert!(super::take_next(&mut args));
    assert_eq!(args, s(&["locate", "x", "--", "--next"]));
}
