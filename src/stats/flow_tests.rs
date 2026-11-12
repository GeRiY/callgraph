//! A parancs-sorrend (v0.9.5) tesztjei: a munkamenet-szabály (30 perc), a `sess` / `seq` számozás, a `rel` és a
//! `rep` jelzők, az átmenet-élek kulcsai (`!`, `start`, `end`, szünet-sávok), az 500 kulcsos korlát, a
//! munkamenet-sávok, az eseménynapló 14 napos törlése, a régi havi fájlok olvashatósága, a HMAC (RFC 4231) és a
//! CLI-cél felismerése.

use std::collections::BTreeMap;

use super::buckets::{InputBucket, LatencyBucket, SizeBucket};
use super::day::Day;
use super::event::{Client, CommandKind, ErrorKind, Event, Outcome};
use super::events::{self, EventLine, expired_days, rfc3339_ms};
use super::hmac::{from_hex, hmac_sha256, keyed_hex, to_hex};
use super::render_events;
use super::session::{
    Closed, MAX_EDGES, OTHER_EDGE, Rel, SESSION_GAP_MS, SessionState, Sessions, Visit, add_edge,
    gap_band, rep_band,
};
use super::store::{MonthFile, StatsStore};
use super::target::{Call, Keys, Target, cli_target};
use super::tests::temp_dir;
use super::tracking_tests::is_edge_key;

/// 2026-11-10 01:00 UTC, ms.
const T: u64 = (1_794_268_800 + 3600) * 1000;

fn keys(file: Option<&str>, dir: &str, project: &str) -> Keys {
    Keys {
        file: file.map(str::to_owned),
        dir: dir.to_owned(),
        project: project.to_owned(),
    }
}

/// Egy 10 ms-os hívás `start_ms`-kor.
fn visit(start_ms: u64, command: CommandKind, failed: bool, keys: Option<Keys>) -> Visit {
    Visit {
        start_ms,
        end_ms: start_ms + 10,
        day: Day::from_secs(start_ms / 1000),
        command,
        failed,
        keys,
    }
}

#[test]
fn a_session_ends_after_more_than_30_minutes_of_silence() {
    let mut state = SessionState::default();
    let first = state.advance(visit(T, CommandKind::Show, false, None));
    assert_eq!((first.sess, first.seq), (1, 1));
    assert_eq!(first.edge, "start>show|none|-");
    assert_eq!(first.closed, None);
    // pontosan 30 perc szünet: még ugyanaz a munkamenet
    let at = T + 10 + SESSION_GAP_MS;
    let second = state.advance(visit(at, CommandKind::Outline, false, None));
    assert_eq!((second.sess, second.seq), (1, 2));
    assert_eq!(second.edge, "show>outline|none|10-30m");
    // 30 perc + 1 ms: új munkamenet, az előző lezárul
    let third = state.advance(visit(
        at + 10 + SESSION_GAP_MS + 1,
        CommandKind::Locate,
        false,
        None,
    ));
    assert_eq!((third.sess, third.seq), (2, 1));
    assert_eq!(third.edge, "start>locate|none|-");
    let closed = third.closed.unwrap();
    assert_eq!(
        closed,
        Closed {
            day: Day::from_secs(T / 1000),
            last: CommandKind::Outline,
            len: 2,
            dur_ms: at + 10 - T,
            rep_max: 0,
        }
    );
    assert_eq!(closed.end_edge(), "outline>end|none|-");
    // a sorszám a munkamenetben tovább nő
    let fourth = state.advance(visit(
        at + 10 + SESSION_GAP_MS + 100,
        CommandKind::Show,
        false,
        None,
    ));
    assert_eq!((fourth.sess, fourth.seq), (2, 2));
    assert_eq!(fourth.closed, None);
}

#[test]
fn relation_to_the_previous_call() {
    let a = keys(Some("A"), "src", "p1");
    assert_eq!(Rel::between(Some(&a), Some(&a)), Rel::SameFile);
    assert_eq!(
        Rel::between(Some(&a), Some(&keys(Some("B"), "src", "p1"))),
        Rel::SameDir
    );
    assert_eq!(
        Rel::between(Some(&a), Some(&keys(Some("C"), "lib", "p1"))),
        Rel::SameProject
    );
    assert_eq!(
        Rel::between(Some(&a), Some(&keys(Some("D"), "src", "p2"))),
        Rel::SameDir,
        "the dir hash already includes the project"
    );
    assert_eq!(
        Rel::between(Some(&a), Some(&keys(Some("E"), "x", "p2"))),
        Rel::OtherProject
    );
    // mappa-cél: ugyanaz a mappa, de nem ugyanaz a fájl
    let dir = keys(None, "src", "p1");
    assert_eq!(Rel::between(Some(&dir), Some(&dir)), Rel::SameDir);
    assert_eq!(Rel::between(None, Some(&a)), Rel::None);
    assert_eq!(Rel::between(Some(&a), None), Rel::None);

    // a munkamenetben: az első hívás és a cél nélküli hívás utáni `none`
    let mut state = SessionState::default();
    let steps: Vec<Rel> = [
        Some(a.clone()),
        Some(a.clone()),
        Some(keys(Some("B"), "src", "p1")),
        None,
        Some(a.clone()),
        Some(keys(Some("Z"), "z", "p9")),
    ]
    .into_iter()
    .enumerate()
    .map(|(i, k)| {
        state
            .advance(visit(T + i as u64 * 1000, CommandKind::Show, false, k))
            .rel
    })
    .collect();
    assert_eq!(
        steps,
        [
            Rel::None,
            Rel::SameFile,
            Rel::SameDir,
            Rel::None,
            Rel::None,
            Rel::OtherProject
        ]
    );
}

#[test]
fn repeat_bands_count_touches_of_the_same_file_in_the_session() {
    assert_eq!(rep_band(1), "1");
    assert_eq!(rep_band(2), "2-3");
    assert_eq!(rep_band(3), "2-3");
    assert_eq!(rep_band(4), "4+");
    assert_eq!(rep_band(40), "4+");
    let mut state = SessionState::default();
    let a = || Some(keys(Some("A"), "src", "p"));
    let reps: Vec<Option<u32>> = (0..5u64)
        .map(|i| {
            let k = if i == 2 {
                Some(keys(None, "src", "p"))
            } else {
                a()
            };
            state
                .advance(visit(T + i * 1000, CommandKind::Outline, false, k))
                .rep
        })
        .collect();
    assert_eq!(reps, [Some(1), Some(2), None, Some(3), Some(4)]);
    // új munkamenetben újra 1-ről indul; a lezárt munkamenet legtöbbje 4
    let next = state.advance(visit(
        T + 5000 + SESSION_GAP_MS + 100,
        CommandKind::Outline,
        false,
        a(),
    ));
    assert_eq!(next.rep, Some(1));
    assert_eq!(next.closed.unwrap().rep_max, 4);
}

#[test]
fn edge_keys_mark_failures_and_gap_bands() {
    for (gap, band) in [
        (0, "<5s"),
        (4_999, "<5s"),
        (5_000, "5-60s"),
        (59_999, "5-60s"),
        (60_000, "1-10m"),
        (599_999, "1-10m"),
        (600_000, "10-30m"),
        (SESSION_GAP_MS, "10-30m"),
    ] {
        assert_eq!(gap_band(gap), band, "{gap} ms");
    }
    let mut state = SessionState::default();
    let a = || Some(keys(Some("A"), "src", "p"));
    let first = state.advance(visit(T, CommandKind::Search, true, a()));
    assert_eq!(first.edge, "start>search|none|-");
    // az előző hibás volt: `!`
    let second = state.advance(visit(T + 10 + 6_000, CommandKind::Show, false, a()));
    assert_eq!(second.edge, "search!>show|same_file|5-60s");
    let third = state.advance(visit(T + 7_000, CommandKind::MethodBody, false, None));
    assert_eq!(third.edge, "show>method_body|none|<5s");
    for edge in [&first.edge, &second.edge, &third.edge] {
        assert!(is_edge_key(edge), "{edge}");
    }
    assert!(is_edge_key("outline>end|none|-"));
    assert!(!is_edge_key("show>/home/x|none|-"));
}

#[test]
fn at_most_500_distinct_edge_keys_per_day() {
    let mut edges = BTreeMap::new();
    for n in 0..MAX_EDGES {
        add_edge(&mut edges, &format!("k{n}"));
    }
    assert_eq!(edges.len(), MAX_EDGES);
    add_edge(&mut edges, "k0");
    assert_eq!(edges["k0"], 2);
    add_edge(&mut edges, "new-1");
    add_edge(&mut edges, "new-2");
    assert!(!edges.contains_key("new-1"));
    assert_eq!(edges[OTHER_EDGE], 2);
    assert_eq!(edges.len(), MAX_EDGES + 1);
}

#[test]
fn closed_sessions_fill_length_duration_and_repeat_bands() {
    let closed = |len: u64, dur_ms: u64, rep_max: u32| Closed {
        day: Day::from_secs(0),
        last: CommandKind::Show,
        len,
        dur_ms,
        rep_max,
    };
    let mut sessions = Sessions::default();
    for c in [
        closed(1, 0, 0),
        closed(2, 59_999, 1),
        closed(5, 60_000, 2),
        closed(6, 599_999, 3),
        closed(20, 600_000, 4),
        closed(21, 3_599_999, 9),
        closed(50, 3_600_000, 1),
        closed(51, 99_000_000, 1),
    ] {
        sessions.add(&c);
    }
    assert_eq!(sessions.count, 8);
    assert_eq!(sessions.len, [1, 2, 2, 2, 1]);
    assert_eq!(sessions.dur, [2, 2, 2, 2]);
    // az egyetlen fájlt sem érintő munkamenet nem számít a rep_max-ba
    assert_eq!(sessions.rep_max, [3, 2, 2]);
}

#[test]
fn event_files_older_than_14_days_are_pruned() {
    let today = Day::parse("2026-11-20").unwrap();
    let names: Vec<String> = [
        "2026-11-06.jsonl",
        "2026-11-07.jsonl",
        "2026-11-20.jsonl",
        "2026-10-01.jsonl",
        "2026-11-06.json",
        "notes.txt",
    ]
    .iter()
    .map(|n| (*n).to_owned())
    .collect();
    assert_eq!(
        expired_days(&names, today),
        ["2026-11-06.jsonl", "2026-10-01.jsonl"]
    );

    // egy új nap első sora törli a lejártakat
    let dir = temp_dir("events-prune");
    let events_dir = dir.join(events::EVENTS_DIR);
    std::fs::create_dir_all(&events_dir).unwrap();
    std::fs::write(events_dir.join("2026-11-01.jsonl"), "{}\n").unwrap();
    std::fs::write(events_dir.join("2026-11-10.jsonl"), "{}\n").unwrap();
    std::fs::write(events_dir.join("keep.txt"), "x").unwrap();
    let line = EventLine {
        t0: rfc3339_ms(T),
        ms: 5,
        cmd: CommandKind::Show,
        via: Client::Cli,
        ok: true,
        err: None,
        lang: None,
        input: None,
        out: "<500".into(),
        rel: Rel::None,
        rep: None,
        sess: 1,
        seq: 1,
    };
    events::append(&dir, today, &line);
    assert!(!events_dir.join("2026-11-01.jsonl").exists());
    assert!(events_dir.join("2026-11-10.jsonl").is_file());
    assert!(events_dir.join("keep.txt").is_file());
    assert_eq!(events::read(&dir, today, today), vec![line]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn timestamps_are_rfc3339_utc_with_milliseconds() {
    assert_eq!(rfc3339_ms(0), "1970-01-01T00:00:00.000Z");
    assert_eq!(rfc3339_ms(T + 61_234), "2026-11-10T01:01:01.234Z");
}

#[test]
fn old_month_files_without_the_new_fields_still_load() {
    let old = r#"{"v":1,"days":{"2026-11-01":{"show":{"calls":2,"clients":{"cli":2},"latency":[2,0,0,0,0],"size":[2,0,0,0],"hits":[1,0,0,0]}}}}"#;
    let file: MonthFile = serde_json::from_str(old).unwrap();
    let day = Day::parse("2026-11-01").unwrap();
    let show = &file.days[&day][&CommandKind::Show];
    assert_eq!(show.calls, 2);
    assert_eq!(show.in_size, [0; InputBucket::COUNT]);
    assert!(show.lang.is_empty());
    assert!(file.edges.is_empty() && file.sessions.is_empty());
    // üresen nem kerülnek ki az új mezők
    let text = file.to_json();
    for field in ["edges", "sessions", "in_size", "lang"] {
        assert!(!text.contains(field), "{field}: {text}");
    }
}

#[test]
fn hmac_sha256_matches_rfc_4231() {
    // 1. teszteset: 20 × 0x0b kulcs, "Hi There"
    assert_eq!(
        to_hex(&hmac_sha256(&[0x0b; 20], b"Hi There")),
        "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
    );
    // 2. teszteset: "Jefe"
    assert_eq!(
        to_hex(&hmac_sha256(b"Jefe", b"what do ya want for nothing?")),
        "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
    );
    // 6. teszteset: a blokknál hosszabb (131 bájtos) kulcs
    assert_eq!(
        to_hex(&hmac_sha256(
            &[0xaa; 131],
            b"Test Using Larger Than Block-Size Key - Hash Key First"
        )),
        "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
    );
    assert_eq!(from_hex("00ff10"), Some(vec![0x00, 0xff, 0x10]));
    assert_eq!(from_hex("0g"), None);
    assert_eq!(from_hex("abc"), None);
    // más kulccsal más a hash (a helyi kulcs nélkül nem visszafejthető / összevethető)
    assert_ne!(keyed_hex(b"k1", "src/A.php"), keyed_hex(b"k2", "src/A.php"));
}

#[test]
fn the_session_key_is_created_once_and_kept() {
    let mut state = SessionState::default();
    let key = state.key_bytes();
    assert_eq!(key.len(), 32);
    assert_eq!(state.key.len(), 64);
    assert_eq!(state.key_bytes(), key);
    let reloaded: SessionState = serde_json::from_str(&state.to_json()).unwrap();
    assert_eq!(reloaded.key, state.key);
    assert_ne!(SessionState::default().key_bytes(), key);
}

fn call(command: CommandKind, end_ms: u64, ms: u64, target: Option<Target>, ok: bool) -> Call {
    Call {
        event: Event {
            command,
            client: Client::Mcp,
            outcome: if ok {
                Outcome::Ok
            } else {
                Outcome::Error(ErrorKind::NotFound)
            },
            latency: LatencyBucket::from_millis(u128::from(ms)),
            size: SizeBucket::from_chars(100),
            truncated: false,
        },
        end_ms,
        ms,
        target,
    }
}

/// Egy teljes rögzítés: számlálók a nyelvvel, élek, a lezárt munkamenet sávjai és vég-éle, eseménysorok.
#[test]
fn recorded_calls_build_edges_sessions_and_the_event_log() {
    let dir = temp_dir("flow");
    let store = StatsStore::new(dir.clone());
    let file = || Target::from_text(Some("shop"), "src/A.php");
    store.record_call(&call(CommandKind::Show, T + 100, 100, file(), true));
    store.record_call(&call(CommandKind::Outline, T + 2_100, 1_000, file(), false));
    let later = T + 2_100 + SESSION_GAP_MS + 60_000;
    store.record_call(&call(CommandKind::Locate, later, 50, None, true));

    let day = Day::from_secs(T / 1000);
    let month = MonthFile::load(&dir.join(format!("{}.json", day.month_key())));
    let edges = &month.edges[&day];
    for key in [
        "start>show|none|-",
        "show>outline|same_file|<5s",
        "outline>end|none|-",
        "start>locate|none|-",
    ] {
        assert_eq!(edges.get(key), Some(&1), "{key}: {edges:?}");
    }
    assert_eq!(edges.len(), 4, "{edges:?}");
    let sessions = &month.sessions[&day];
    assert_eq!(sessions.count, 1);
    assert_eq!(sessions.len, [0, 1, 0, 0, 0]);
    assert_eq!(sessions.dur, [1, 0, 0, 0]);
    assert_eq!(sessions.rep_max, [0, 1, 0]);
    let show = &month.days[&day][&CommandKind::Show];
    assert_eq!(show.lang.get("php"), Some(&1));
    // fájlrendszer nélküli célnál nincs méret
    assert_eq!(show.in_size, [0; InputBucket::COUNT]);
    let (flow_edges, flow_sessions) = store.day_flow(day);
    assert_eq!(&flow_edges, edges);
    assert_eq!(flow_sessions.as_ref(), Some(sessions));

    let lines = events::read(&dir, day, day);
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0].t0, "2026-11-10T01:00:00.000Z");
    assert_eq!(
        (lines[0].sess, lines[0].seq, lines[0].rel),
        (1, 1, Rel::None)
    );
    assert_eq!(lines[0].lang.as_deref(), Some("php"));
    assert_eq!(lines[1].rel, Rel::SameFile);
    assert_eq!(lines[1].rep.as_deref(), Some("2-3"));
    assert_eq!(
        (lines[1].ok, lines[1].err),
        (false, Some(ErrorKind::NotFound))
    );
    assert_eq!((lines[2].sess, lines[2].seq), (2, 1));
    assert_eq!(
        (lines[2].lang.as_deref(), lines[2].rep.as_deref()),
        (None, None)
    );

    let text = render_events(&store, day, 1);
    assert!(text.contains("s1#2"), "{text}");
    assert!(text.contains("show>outline|same_file|<5s"), "{text}");
    let lang = crate::i18n::lang();
    assert!(
        text.contains(lang.pick("top transitions", "leggyakoribb átmenetek")),
        "{text}"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_cli_target_is_the_first_existing_path_argument() {
    let dir = temp_dir("cli-target");
    std::fs::create_dir_all(dir.join(".git")).unwrap();
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/A.php"), vec![b'x'; 12_000]).unwrap();
    let root = dir.canonicalize().unwrap();
    let args = |v: &[&str]| -> Vec<String> { v.iter().map(|a| (*a).to_owned()).collect() };
    let root_text = root.display().to_string();

    let show = cli_target(
        CommandKind::Show,
        &args(&[
            "show",
            "--project",
            root_text.as_str(),
            "--format",
            "outline",
            "src/A.php::cancel",
        ]),
        std::path::Path::new("/"),
    )
    .unwrap();
    assert_eq!(show.lang, Some("php"));
    assert_eq!(show.input, Some(InputBucket::To50K));
    assert_eq!(show.project, root_text);
    assert!(show.file.is_some());

    // közvetlen `callgraph <fájl>`: a projekt a legközelebbi `.git`-es ős
    let direct = cli_target(CommandKind::Other, &args(&["src/A.php"]), &root).unwrap();
    assert_eq!(direct.project, root_text);
    assert_eq!(direct.file, show.file);

    let folder = cli_target(CommandKind::Other, &args(&["src"]), &root).unwrap();
    assert_eq!((folder.file, folder.lang, folder.input), (None, None, None));

    assert_eq!(
        cli_target(CommandKind::Locate, &args(&["locate", "src/A.php"]), &root),
        None
    );
    assert_eq!(
        cli_target(CommandKind::Show, &args(&["show", "missing.php"]), &root),
        None
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn text_targets_split_file_dir_and_project() {
    let file = Target::from_text(Some("shop"), "./src\\A.PHP").unwrap();
    let same = Target::from_text(Some("shop"), "src/A.PHP").unwrap();
    assert_eq!(file, same);
    assert_eq!(file.lang, Some("php"));
    let other_dir = Target::from_text(Some("shop"), "lib/B.php").unwrap();
    let key = [7u8; 32];
    let (a, b) = (file.keys(&key), other_dir.keys(&key));
    assert_eq!(Rel::between(Some(&a), Some(&b)), Rel::SameProject);
    let folder = Target::from_text(None, "app/").unwrap();
    assert_eq!((folder.file, folder.lang), (None, None));
    assert_eq!(Target::from_text(None, ""), None);
    // a hash-ekben nincs út
    assert!(!a.dir.contains("src") && a.dir.len() == 64);
}
