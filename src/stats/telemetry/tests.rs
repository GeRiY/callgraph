//! A telemetria tesztjei: egy helyi HTTP-szerverrel (std `TcpListener` egy szálon) a beküldés feltételei
//! (nem döntött / ki / be, egy nap egyszer, nem 2xx, kemény tiltás, CI), a törzs adatvédelme, a kérdés, az
//! emlékeztető, az agent-tipp ritkítása és a `telemetry on` terminál-feltétele.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use super::{
    Choice, HttpSender, MIN_CALLS, REMIND_AFTER, SendOutcome, Telemetry, TelemetryEnv, Tty,
};
use crate::stats::buckets::{LatencyBucket, SizeBucket};
use crate::stats::day::Day;
use crate::stats::event::{Client, CommandKind, ErrorKind, Event, Outcome};
use crate::stats::policy::StatsEnv;
use crate::stats::store::StatsStore;
use crate::stats::target::{Call, Target};

/// 2026-11-10 01:00 UTC.
const NOW: u64 = 1_794_268_800 + 3600;
const TTY: Tty = Tty {
    stdin: true,
    stderr: true,
};
const NO_TTY: Tty = Tty {
    stdin: false,
    stderr: true,
};

fn temp_dir(name: &str) -> PathBuf {
    crate::stats::tests::temp_dir(&format!("telemetry-{name}"))
}

/// Egy helyi szerver, ami minden kérésre `status`-szal válaszol, és megjegyzi a törzseket.
fn server(status: u16) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!(
        "http://{}/callgraph/telemetry",
        listener.local_addr().unwrap()
    );
    let bodies = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&bodies);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut length = 0;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
                if let Some((name, value)) = line.split_once(':')
                    && name.eq_ignore_ascii_case("content-length")
                {
                    length = value.trim().parse().unwrap_or(0);
                }
            }
            let mut body = vec![0; length];
            let _ = reader.read_exact(&mut body);
            seen.lock()
                .unwrap()
                .push(String::from_utf8_lossy(&body).into_owned());
            let _ = write!(
                stream,
                "HTTP/1.1 {status} X\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            );
        }
    });
    (url, bodies)
}

fn env(dir: &std::path::Path, url: &str) -> TelemetryEnv {
    TelemetryEnv {
        stats: StatsEnv {
            dir: Some(dir.to_path_buf()),
            ..StatsEnv::default()
        },
        killed: false,
        url: url.to_owned(),
    }
}

fn event(command: CommandKind, outcome: Outcome) -> Event {
    Event {
        command,
        client: Client::Cli,
        outcome,
        latency: LatencyBucket::from_millis(20),
        size: SizeBucket::from_chars(900),
        truncated: false,
    }
}

/// Valószerű adat: ma, tegnap és tegnapelőtt; hibák, `locate`-találat, javaslat-számlálók.
fn populate(dir: &std::path::Path, calls_today: usize) {
    let store = StatsStore::new(dir.to_path_buf());
    let today = Day::from_secs(NOW);
    for n in 0..calls_today {
        let kind = if n % 2 == 0 {
            CommandKind::Show
        } else {
            CommandKind::Locate
        };
        store.record(today, &event(kind, Outcome::Ok));
    }
    for back in [1, 2] {
        let day = today.minus(back);
        store.record(day, &event(CommandKind::Locate, Outcome::Ok));
        store.record(
            day,
            &event(CommandKind::Show, Outcome::Error(ErrorKind::NotFound)),
        );
        store.record_hit(day, crate::stats::buckets::RankBucket::First);
        store.record_next(day, CommandKind::Locate, false);
    }
}

fn turned_on(dir: &std::path::Path, url: &str) -> Telemetry {
    let telemetry = Telemetry::new(env(dir, url), dir.to_path_buf());
    telemetry.decide(Choice::Yes, NOW - 10).unwrap();
    telemetry
}

#[test]
fn nothing_is_sent_unless_turned_on() {
    let (url, bodies) = server(200);
    let dir = temp_dir("undecided");
    populate(&dir, 3);
    let telemetry = Telemetry::new(env(&dir, &url), dir.clone());
    assert_eq!(telemetry.send_now(&HttpSender, NOW), SendOutcome::Skipped);
    telemetry.decide(Choice::Off, NOW).unwrap();
    assert_eq!(telemetry.send_now(&HttpSender, NOW), SendOutcome::Skipped);
    telemetry.decide(Choice::Later, NOW).unwrap();
    assert_eq!(telemetry.send_now(&HttpSender, NOW), SendOutcome::Skipped);
    assert!(bodies.lock().unwrap().is_empty());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn turned_on_sends_closed_days_once_a_day() {
    let (url, bodies) = server(204);
    let dir = temp_dir("on");
    populate(&dir, 3);
    let telemetry = turned_on(&dir, &url);
    assert_eq!(telemetry.send_now(&HttpSender, NOW), SendOutcome::Sent(2));
    assert_eq!(
        telemetry.send_now(&HttpSender, NOW + 60),
        SendOutcome::Skipped
    );
    let sent = bodies.lock().unwrap().clone();
    assert_eq!(sent.len(), 1, "{sent:?}");
    let body: serde_json::Value = serde_json::from_str(&sent[0]).unwrap();
    let days: Vec<&str> = body["days"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["day"].as_str().unwrap())
        .collect();
    assert_eq!(days, ["2026-11-08", "2026-11-09"]);
    assert_eq!(body["schema"], 2);
    let consent = telemetry.consent();
    assert_eq!(consent.last_sent_day.as_deref(), Some("2026-11-09"));
    // a következő nap: csak az új lezárt nap menne (ma nincs új adat: nincs mit küldeni)
    assert!(telemetry.pending(&consent, Day::from_secs(NOW)).is_empty());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_failed_send_leaves_the_days_pending() {
    let (url, bodies) = server(503);
    let dir = temp_dir("fail");
    populate(&dir, 1);
    let telemetry = turned_on(&dir, &url);
    assert!(matches!(
        telemetry.send_now(&HttpSender, NOW),
        SendOutcome::Failed(_)
    ));
    assert_eq!(bodies.lock().unwrap().len(), 1);
    let consent = telemetry.consent();
    assert_eq!(consent.last_sent_day, None);
    assert_eq!(telemetry.pending(&consent, Day::from_secs(NOW)).len(), 2);
    // a következő kísérlet legkorábban egy óra múlva
    assert_eq!(
        telemetry.send_now(&HttpSender, NOW + 60),
        SendOutcome::Skipped
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn kill_switch_stats_off_and_ci_block_sending_even_when_on() {
    let (url, bodies) = server(200);
    let dir = temp_dir("blocked");
    populate(&dir, 1);
    turned_on(&dir, &url);
    let mut killed = env(&dir, &url);
    killed.killed = true;
    let mut stats_off = env(&dir, &url);
    stats_off.stats.setting = Some("0".into());
    let mut ci = env(&dir, &url);
    ci.stats.ci = true;
    for blocked in [killed, stats_off, ci] {
        let telemetry = Telemetry::new(blocked, dir.clone());
        assert_eq!(telemetry.send_now(&HttpSender, NOW), SendOutcome::Skipped);
        assert!(telemetry.agent_hint(NOW).is_none());
    }
    assert!(bodies.lock().unwrap().is_empty());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_payload_has_no_paths_or_names() {
    let dir = temp_dir("payload");
    populate(&dir, 5);
    let telemetry = Telemetry::new(env(&dir, "http://127.0.0.1:1/"), dir.clone());
    let text = telemetry.preview(NOW);
    let dir_text = dir.display().to_string();
    assert!(!text.contains('/') && !text.contains('\\'), "{text}");
    for secret in [
        dir_text.as_str(),
        ".php",
        ".rs",
        "callgraph-stats",
        "telemetry-payload",
    ] {
        assert!(!text.contains(secret), "{secret}: {text}");
    }
    assert!(
        text.contains("\"batch\"") && text.contains("\"platform\""),
        "{text}"
    );
    assert!(!text.contains("2026-11-10"), "today is not closed: {text}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn asks_only_in_a_terminal_and_reminds_once_after_not_now() {
    let dir = temp_dir("ask");
    let telemetry = Telemetry::new(env(&dir, "http://127.0.0.1:1/"), dir.clone());
    let mut err = Vec::new();
    assert!(!telemetry.ask(NO_TTY, &mut "y\n".as_bytes(), &mut err, NOW));
    assert_eq!(telemetry.consent().choice, None);
    assert!(telemetry.ask(TTY, &mut "l\n".as_bytes(), &mut err, NOW));
    assert_eq!(telemetry.consent().choice, Some(Choice::Later));
    assert!(!telemetry.consent().enabled);
    // már döntött: nem kérdez újra
    assert!(!telemetry.ask(TTY, &mut "y\n".as_bytes(), &mut err, NOW + 1));
    assert!(telemetry.stats_hint(NOW + 60).is_none());
    assert!(telemetry.stats_hint(NOW + REMIND_AFTER).is_some());
    assert!(telemetry.stats_hint(NOW + 2 * REMIND_AFTER).is_none());
    let _ = std::fs::remove_dir_all(dir);

    let dir = temp_dir("never");
    let telemetry = Telemetry::new(env(&dir, "http://127.0.0.1:1/"), dir.clone());
    assert!(telemetry.ask(TTY, &mut "n\n".as_bytes(), &mut err, NOW));
    assert_eq!(telemetry.consent().choice, Some(Choice::Never));
    assert!(telemetry.stats_hint(NOW + REMIND_AFTER).is_none());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_agent_hint_is_rare_and_stops_after_never_or_on() {
    let dir = temp_dir("hint");
    let telemetry = Telemetry::new(env(&dir, "http://127.0.0.1:1/"), dir.clone());
    populate(&dir, MIN_CALLS as usize - 5);
    assert!(telemetry.agent_hint(NOW).is_none(), "too little usage");
    populate(&dir, 10);
    let hint = telemetry.agent_hint(NOW).unwrap();
    assert!(hint.contains("telemetry on"), "{hint}");
    assert!(telemetry.agent_hint(NOW + 60).is_none(), "once per 14 days");
    assert!(telemetry.agent_hint(NOW + REMIND_AFTER).is_some());
    assert!(telemetry.agent_hint(NOW + 2 * REMIND_AFTER).is_some());
    assert!(
        telemetry.agent_hint(NOW + 3 * REMIND_AFTER).is_none(),
        "at most 3"
    );
    let _ = std::fs::remove_dir_all(&dir);

    for choice in [Choice::Never, Choice::Yes, Choice::Off] {
        let dir = temp_dir("hint-decided");
        populate(&dir, MIN_CALLS as usize);
        let telemetry = Telemetry::new(env(&dir, "http://127.0.0.1:1/"), dir.clone());
        telemetry.decide(choice, NOW).unwrap();
        assert!(telemetry.agent_hint(NOW).is_none(), "{choice:?}");
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[test]
fn turning_on_needs_a_terminal_and_a_yes() {
    let dir = temp_dir("confirm");
    let telemetry = Telemetry::new(env(&dir, "http://127.0.0.1:1/"), dir.clone());
    let mut err = Vec::new();
    let refused = telemetry.confirm_on(NO_TTY, &mut "y\n".as_bytes(), &mut err, NOW);
    assert!(refused.is_err());
    assert!(!telemetry.consent().enabled);
    assert_eq!(
        telemetry.confirm_on(TTY, &mut "\n".as_bytes(), &mut err, NOW),
        Ok(false)
    );
    assert!(!telemetry.consent().enabled);
    assert_eq!(
        telemetry.confirm_on(TTY, &mut "y\n".as_bytes(), &mut err, NOW),
        Ok(true)
    );
    assert!(telemetry.consent().enabled);
    let _ = std::fs::remove_dir_all(dir);
}

/// A tegnapi napra két munkamenet (40 perc szünettel), fájl-céllal: élek és egy lezárt munkamenet.
fn populate_flow(dir: &std::path::Path) {
    let store = StatsStore::new(dir.to_path_buf());
    let yesterday_ms = (NOW - 86_400) * 1000;
    let call = |command, end_ms: u64| Call {
        event: event(command, Outcome::Ok),
        end_ms,
        ms: 20,
        target: Target::from_text(Some("SecretShop"), "app/Http/RefundController.php"),
    };
    store.record_call(&call(CommandKind::Locate, yesterday_ms));
    store.record_call(&call(CommandKind::Show, yesterday_ms + 3_000));
    store.record_call(&call(
        CommandKind::Show,
        yesterday_ms + 3_000 + 40 * 60 * 1000,
    ));
}

#[test]
fn payload_schema_2_adds_edges_and_sessions_without_names() {
    let dir = temp_dir("schema2");
    populate(&dir, 2);
    populate_flow(&dir);
    let telemetry = Telemetry::new(env(&dir, "http://127.0.0.1:1/"), dir.clone());
    let text = telemetry.preview(NOW);
    let body: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(body["schema"], 2);
    let days = body["days"].as_array().unwrap();
    let yesterday = days
        .iter()
        .find(|d| d["day"] == "2026-11-09")
        .expect("yesterday is a closed day");
    let edges = yesterday["edges"].as_object().unwrap();
    for key in [
        "start>locate|none|-",
        "locate>show|same_file|<5s",
        "show>end|none|-",
        "start>show|none|-",
    ] {
        assert_eq!(edges.get(key), Some(&serde_json::json!(1)), "{key}: {text}");
    }
    for key in edges.keys() {
        assert!(crate::stats::tracking_tests::is_edge_key(key), "{key}");
    }
    assert_eq!(yesterday["sessions"]["count"], 1);
    assert_eq!(
        yesterday["sessions"]["len"],
        serde_json::json!([0, 1, 0, 0, 0])
    );
    assert_eq!(yesterday["commands"]["show"]["lang"]["php"], 2);
    // a napbejegyzés mezői; az élek nélküli nap nem kap üres `edges`-t
    for day in days {
        for key in day.as_object().unwrap().keys() {
            assert!(
                ["day", "batch", "commands", "edges", "sessions"].contains(&key.as_str()),
                "{key}"
            );
        }
    }
    let before = days.iter().find(|d| d["day"] == "2026-11-08").unwrap();
    assert!(before.get("edges").is_none() && before.get("sessions").is_none());
    // út, név, hash nincs benne (a munkamenet-állapot és az eseménynapló nem megy el)
    assert!(!text.contains('/') && !text.contains('\\'), "{text}");
    for secret in ["SecretShop", "RefundController", "Http", ".php"] {
        assert!(!text.contains(secret), "{secret}: {text}");
    }
    let session = std::fs::read_to_string(dir.join("session.json")).unwrap();
    let key: serde_json::Value = serde_json::from_str(&session).unwrap();
    assert!(!text.contains(key["key"].as_str().unwrap()));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_command_order_notice_is_shown_once_and_only_when_already_on() {
    // már bekapcsolt beküldés: egyszer
    let dir = temp_dir("order-on");
    let telemetry = turned_on(&dir, "http://127.0.0.1:1/");
    let notice = telemetry.order_notice().unwrap();
    assert!(
        notice.contains("callgraph.sh telemetry preview"),
        "{notice}"
    );
    assert!(notice.contains("callgraph.sh telemetry off"), "{notice}");
    assert!(telemetry.order_notice().is_none());
    let _ = std::fs::remove_dir_all(dir);

    // nem döntött: semmi, és később bekapcsolva sem (az új leírást már látta)
    let dir = temp_dir("order-off");
    let telemetry = Telemetry::new(env(&dir, "http://127.0.0.1:1/"), dir.clone());
    assert!(telemetry.order_notice().is_none());
    telemetry.decide(Choice::Yes, NOW).unwrap();
    assert!(telemetry.order_notice().is_none());
    let _ = std::fs::remove_dir_all(dir);

    // tiltott környezetben a jelző sem jön létre
    let dir = temp_dir("order-killed");
    turned_on(&dir, "http://127.0.0.1:1/");
    let mut killed = env(&dir, "http://127.0.0.1:1/");
    killed.killed = true;
    assert!(Telemetry::new(killed, dir.clone()).order_notice().is_none());
    assert!(!dir.join(super::ORDER_NOTICE_MARKER).exists());
    assert!(
        Telemetry::new(env(&dir, "http://127.0.0.1:1/"), dir.clone())
            .order_notice()
            .is_some()
    );
    let _ = std::fs::remove_dir_all(dir);
}
