//! A helyi mérés egységtesztjei: sáv-határok, napok, összegzés, 90 napos törlés, bekapcsolás, riport. A
//! követés, a besorolás és az adatvédelem tesztjei: `tracking_tests.rs`.

use std::path::PathBuf;

use super::buckets::{LatencyBucket, RankBucket, SizeBucket};
use super::day::Day;
use super::event::{Client, CommandKind, ErrorKind, Event, Outcome};
use super::policy::StatsEnv;
use super::store::{StatsStore, expired_months};
use super::{Recorder, render};

pub(super) fn temp_dir(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    std::env::temp_dir().join(format!(
        "callgraph-stats-{name}-{}-{nanos}",
        std::process::id()
    ))
}

fn day(text: &str) -> Day {
    Day::parse(text).unwrap()
}

fn event(command: CommandKind, client: Client, outcome: Outcome, ms: u128) -> Event {
    Event {
        command,
        client,
        outcome,
        latency: LatencyBucket::from_millis(ms),
        size: SizeBucket::from_chars(100),
        truncated: false,
    }
}

#[test]
fn latency_and_size_bucket_boundaries() {
    use LatencyBucket::*;
    for (ms, bucket) in [
        (0, Under50Ms),
        (49, Under50Ms),
        (50, To200Ms),
        (199, To200Ms),
        (200, To1S),
        (999, To1S),
        (1000, To5S),
        (4999, To5S),
        (5000, Over5S),
        (600_000, Over5S),
    ] {
        assert_eq!(LatencyBucket::from_millis(ms), bucket, "{ms} ms");
    }
    use SizeBucket::*;
    // token ≈ karakter / 4
    for (chars, bucket) in [
        (0, Under500),
        (1999, Under500),
        (2000, To2K),
        (7999, To2K),
        (8000, To8K),
        (31_999, To8K),
        (32_000, Over8K),
    ] {
        assert_eq!(SizeBucket::from_chars(chars), bucket, "{chars} chars");
    }
    use RankBucket::*;
    for (rank, bucket) in [(1, First), (2, Top5), (5, Top5), (6, Beyond), (40, Beyond)] {
        assert_eq!(RankBucket::from_rank(rank), bucket, "rank {rank}");
    }
    assert_eq!(LatencyBucket::LABELS.len(), LatencyBucket::COUNT);
    assert_eq!(Over5S.index(), LatencyBucket::COUNT - 1);
    assert_eq!(Over8K.index(), SizeBucket::COUNT - 1);
    assert_eq!(NotListed.index(), RankBucket::COUNT - 1);
}

#[test]
fn days_round_trip_and_month_arithmetic() {
    assert_eq!(Day::from_secs(0).to_string(), "1970-01-01");
    // 2026-11-06T12:00:00Z
    assert_eq!(Day::from_secs(1_793_966_400).to_string(), "2026-11-06");
    for text in ["2024-02-29", "2026-12-31", "2027-01-01", "2000-03-01"] {
        assert_eq!(day(text).to_string(), text);
    }
    assert_eq!(Day::parse("2026-02-29"), None);
    assert_eq!(Day::parse("2026-13-01"), None);
    assert_eq!(Day::parse("garbage"), None);
    assert_eq!(day("2026-03-01").minus(1).to_string(), "2026-02-28");
    assert_eq!(day("2026-12-15").month_key(), "2026-12");
    assert_eq!(day("2026-12-15").month_start(), day("2026-12-01"));
    assert_eq!(day("2026-12-15").next_month_start(), day("2027-01-01"));
    assert_eq!(Day::parse_month("2026-07"), Some(day("2026-07-01")));
}

#[test]
fn records_and_sums_days_across_months() {
    let dir = temp_dir("sum");
    let store = StatsStore::new(dir.clone());
    let show = event(CommandKind::Show, Client::Cli, Outcome::Ok, 10);
    store.record(day("2026-10-31"), &show);
    store.record(day("2026-11-01"), &show);
    store.record(
        day("2026-11-01"),
        &event(
            CommandKind::Show,
            Client::Mcp,
            Outcome::Error(ErrorKind::NotFound),
            300,
        ),
    );
    store.record(
        day("2026-11-02"),
        &event(CommandKind::Locate, Client::Mcp, Outcome::Ok, 60),
    );
    store.record_hit(day("2026-11-02"), RankBucket::First);
    // az időszakon kívül
    store.record(day("2026-10-20"), &show);
    assert!(dir.join("2026-10.json").is_file());
    assert!(dir.join("2026-11.json").is_file());

    let totals = store.totals(day("2026-10-27"), day("2026-11-02"));
    assert_eq!(totals.calls(), 4);
    assert_eq!(totals.client_calls(Client::Cli), 2);
    assert_eq!(totals.client_calls(Client::Mcp), 2);
    let show = &totals.commands[&CommandKind::Show];
    assert_eq!(show.calls, 3);
    assert_eq!(show.error_count(), 1);
    assert_eq!(show.errors.get(&ErrorKind::NotFound), Some(&1));
    assert_eq!(show.latency, [2, 0, 1, 0, 0]);
    assert_eq!(show.size, [3, 0, 0, 0]);
    let locate = &totals.commands[&CommandKind::Locate];
    assert_eq!((locate.calls, locate.latency[1]), (1, 1));
    assert_eq!(locate.hits, [1, 0, 0, 0]);

    // egy nap egyedül; egy üres időszak
    assert_eq!(
        store.totals(day("2026-10-20"), day("2026-10-20")).calls(),
        1
    );
    assert_eq!(
        store.totals(day("2026-09-01"), day("2026-09-30")).calls(),
        0
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn prunes_month_files_older_than_90_days() {
    let names: Vec<String> = [
        "2025-12.json",
        "2026-07.json",
        "2026-08.json",
        "2026-11.json",
        "2026-13.json",
        "notes.txt",
        "2026-07.json.tmp-1",
    ]
    .iter()
    .map(|n| (*n).to_owned())
    .collect();
    // 2026-11-06 - 90 nap = 2026-08-08: a július vége régebbi, az augusztusi napok egy része még nem
    assert_eq!(
        expired_months(&names, day("2026-11-06")),
        ["2025-12.json", "2026-07.json"]
    );

    // az új hónap első írása törli a lejártakat
    let dir = temp_dir("prune");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("2026-07.json"), "{}").unwrap();
    std::fs::write(dir.join("2026-09.json"), "{}").unwrap();
    std::fs::write(dir.join("other.txt"), "keep").unwrap();
    let store = StatsStore::new(dir.clone());
    store.record(
        day("2026-11-06"),
        &event(CommandKind::Show, Client::Cli, Outcome::Ok, 1),
    );
    assert!(!dir.join("2026-07.json").exists());
    assert!(dir.join("2026-09.json").is_file());
    assert!(dir.join("other.txt").is_file());
    assert!(dir.join("2026-11.json").is_file());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn disabled_unless_in_a_skill_install_or_explicitly_pointed_to_a_dir() {
    let base = StatsEnv {
        setting: None,
        dir: None,
        ci: false,
        in_skill: true,
    };
    assert!(base.enabled());
    for off in ["0", "off", "FALSE", " no "] {
        let env = StatsEnv {
            setting: Some(off.into()),
            ..base.clone()
        };
        assert!(!env.enabled(), "{off}");
    }
    assert!(
        !StatsEnv {
            ci: true,
            ..base.clone()
        }
        .enabled()
    );
    let dev = StatsEnv {
        in_skill: false,
        ..base.clone()
    };
    assert!(!dev.enabled());
    // a CALLGRAPH_STATS_DIR kifejezetten bekapcsol (CI-ben és fejlesztői buildben is), a kikapcsolás erősebb
    let pointed = StatsEnv {
        dir: Some(PathBuf::from("/tmp/stats")),
        ci: true,
        ..dev
    };
    assert!(pointed.enabled());
    assert!(
        !StatsEnv {
            setting: Some("0".into()),
            ..pointed
        }
        .enabled()
    );
    assert!(!Recorder::disabled().enabled());
}

#[test]
fn report_is_short_and_shows_the_locate_hit_rate() {
    let dir = temp_dir("report");
    let store = StatsStore::new(dir.clone());
    let today = day("2026-11-06");
    for kind in CommandKind::ALL {
        store.record(today, &event(kind, Client::Mcp, Outcome::Ok, 20));
    }
    store.record(
        today.minus(2),
        &event(
            CommandKind::Show,
            Client::Cli,
            Outcome::Error(ErrorKind::Usage),
            20,
        ),
    );
    store.record_hit(today, RankBucket::First);
    store.record_hit(today, RankBucket::NotListed);
    let totals = store.totals(today.minus(6), today);
    let text = render(&totals, today, 7, &dir, true);
    assert!(text.lines().count() <= 22, "{text}");
    assert!(text.contains("2026-10-31"), "{text}");
    let lang = crate::i18n::lang();
    assert!(text.contains(lang.pick("1 of 2", "1 / 2")), "{text}");
    assert!(text.contains("CALLGRAPH_STATS=0"), "{text}");
    assert!(text.contains("mcp 16, cli 1"), "{text}");

    let empty = render(&Default::default(), today, 7, &dir, false);
    assert_eq!(empty.lines().count(), 3, "{empty}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn report_splits_user_and_internal_errors_and_shows_next_acceptance() {
    let dir = temp_dir("split");
    let store = StatsStore::new(dir.clone());
    let today = day("2026-11-06");
    for kind in [
        ErrorKind::Usage,
        ErrorKind::NotFound,
        ErrorKind::Unsupported,
    ] {
        store.record(
            today,
            &event(CommandKind::Show, Client::Cli, Outcome::Error(kind), 5),
        );
    }
    store.record(
        today,
        &event(
            CommandKind::Show,
            Client::Cli,
            Outcome::Error(ErrorKind::Failed),
            5,
        ),
    );
    store.record_next(today, CommandKind::Locate, false);
    store.record_next(today, CommandKind::Locate, false);
    store.record_next(today, CommandKind::Locate, true);
    let totals = store.totals(today, today);
    let show = &totals.commands[&CommandKind::Show];
    assert_eq!(
        (show.user_error_count(), show.internal_error_count()),
        (3, 1)
    );
    let text = render(&totals, today, 1, &dir, true);
    assert!(text.contains("errors.jsonl"), "{text}");
    assert!(text.contains("locate 1/2"), "{text}");
    assert!(text.contains("(50%)"), "{text}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn skill_failure_contexts_and_exit_code_2_map_to_user_errors() {
    assert_eq!(ErrorKind::of_context("usage"), ErrorKind::Usage);
    assert_eq!(ErrorKind::of_context("not_found"), ErrorKind::NotFound);
    assert_eq!(ErrorKind::of_context("unsupported"), ErrorKind::Unsupported);
    assert_eq!(ErrorKind::of_context("error"), ErrorKind::Failed);
    assert!(ErrorKind::Unsupported.is_user_error() && !ErrorKind::Io.is_user_error());
    assert_eq!(ErrorKind::of_message("Usage: bash …"), ErrorKind::Usage);
}

/// A helyreállító tippek (második `Did you mean:` sor) nem változtatják a hibaosztályt: az osztályozás az első
/// sor előtagjából jön, mindkét nyelven.
#[test]
fn recovery_hints_keep_the_error_class() {
    for (message, kind) in [
        (
            "Not found: /p/app/Model/User.php (relative to the --project p root)\nDid you mean: app/Models/User.php",
            ErrorKind::NotFound,
        ),
        (
            "Nem található: /p/app/Model/User.php (--project p gyökeréhez képest)\nEsetleg erre gondoltál: app/Models/User.php",
            ErrorKind::NotFound,
        ),
        (
            "No such method: /p/User.php::fnd (see --format outline for the file's full list)\nDid you mean: find",
            ErrorKind::NotFound,
        ),
        (
            "Nincs ilyen metódus: /p/User.php::fnd (lásd: --format outline a fájl teljes listájához)\nEsetleg erre gondoltál: find",
            ErrorKind::NotFound,
        ),
        (
            "Unknown category: modle_use (known: model_use, orm_relation)\nDid you mean: model_use",
            ErrorKind::Usage,
        ),
        (
            "Ismeretlen kategória: modle_use (ismertek: model_use)\nEsetleg erre gondoltál: model_use",
            ErrorKind::Usage,
        ),
    ] {
        assert_eq!(ErrorKind::of_message(message), kind, "{message}");
    }
}

#[test]
fn the_local_stats_notice_is_claimed_only_once() {
    let dir = temp_dir("notice");
    let recorder = Recorder::at(dir.clone());
    assert!(
        recorder
            .first_notice()
            .is_some_and(|text| text.contains("CALLGRAPH_STATS=0"))
    );
    assert!(recorder.first_notice().is_none());
    assert!(Recorder::disabled().first_notice().is_none());
    let _ = std::fs::remove_dir_all(dir);
}
