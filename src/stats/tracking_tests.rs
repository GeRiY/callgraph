//! A `locate`-követés, a CLI besorolása, a levágás és a karakterszám felismerése, és hogy a napi fájlba út
//! vagy név nem kerülhet (a típusok csak rögzített neveket hordoznak).

use std::path::PathBuf;
use std::time::Instant;

use super::Recorder;
use super::buckets::{InputBucket, RankBucket, SizeBucket};
use super::cli_probe::{Meter, classify};
use super::day::Day;
use super::event::{Client, CommandKind, ErrorKind, truncated};
use super::locate_hits::{LocateHits, WINDOW, ranked_files};
use super::mcp_probe::{McpStats, ToolCall};
use super::reuse::USE_BANDS;
use super::session::{GAP_BANDS, REP_LABELS, Rel};
use super::store::MonthFile;
use super::target::LANGUAGES;
use super::tests::temp_dir;

#[test]
fn locate_hits_follow_the_ranked_list() {
    let files = || vec!["src/A.php".to_owned(), "src/B.php".to_owned()];
    let mut hits = LocateHits::default();
    // nincs lista: nincs mit követni
    assert_eq!(hits.followed(None, Some("src/A.php")), None);

    assert_eq!(hits.listed(None, files()), None);
    assert_eq!(hits.followed(None, None), None);
    assert_eq!(
        hits.followed(None, Some("./src/B.php")),
        Some(RankBucket::Top5)
    );
    // lezárult: a további hívások nem számítanak
    assert_eq!(hits.followed(None, Some("src/A.php")), None);

    // abszolút út a kérésben, Windows-elválasztóval
    hits.listed(None, files());
    assert_eq!(
        hits.followed(None, Some("C:\\work\\shop\\src\\A.php")),
        Some(RankBucket::First)
    );

    // WINDOW hívás találat nélkül: „nem volt a listán”
    hits.listed(None, files());
    for _ in 1..WINDOW {
        assert_eq!(hits.followed(None, Some("src/Other.php")), None);
    }
    assert_eq!(
        hits.followed(None, Some("src/Other.php")),
        Some(RankBucket::NotListed)
    );

    // új locate a nyitott lista helyett: az előző „nem volt a listán”
    hits.listed(None, files());
    assert_eq!(
        hits.listed(None, vec!["src/C.php".into()]),
        Some(RankBucket::NotListed)
    );
    // más projektben ugyanaz az út nem találat
    assert_eq!(hits.followed(Some("other"), Some("src/C.php")), None);
    assert_eq!(
        hits.followed(None, Some("src/C.php")),
        Some(RankBucket::First)
    );

    // a 6. hely
    let many: Vec<String> = (1..=8).map(|i| format!("src/F{i}.php")).collect();
    hits.listed(Some("shop"), many);
    assert_eq!(
        hits.followed(Some("shop"), Some("src/F6.php")),
        Some(RankBucket::Beyond)
    );
    // üres lista nem nyit követést
    assert_eq!(hits.listed(None, Vec::new()), None);
    assert_eq!(hits.followed(None, Some("src/A.php")), None);
}

#[test]
fn reads_the_ranked_files_of_a_locate_output() {
    let output = "locate: 2 terms · 3 matches in 2 files · start here ↓\n\
                  1. src/RefundService.php  ★★★\n   class RefundService (exact class)\n\
                  2. src\\Order.php  ★\n   'payment_status' (string)\n\
                  no match: nothingLikeThis\n";
    assert_eq!(
        ranked_files(output),
        ["src/RefundService.php", "src\\Order.php"]
    );
    assert!(ranked_files("locate: 1 terms · no match in the project map\n").is_empty());
}

#[test]
fn classifies_cli_runs_and_skips_servers_and_background_children() {
    let args = |v: &[&str]| -> Vec<String> { v.iter().map(|a| (*a).to_owned()).collect() };
    let cli = |v: &[&str]| classify(&args(v), None);
    assert_eq!(
        cli(&["wrap", "app/Order.php"]),
        Some((CommandKind::Show, Client::Cli))
    );
    assert_eq!(
        cli(&["show", "--project", "/x"]),
        Some((CommandKind::Show, Client::Cli))
    );
    assert_eq!(
        cli(&["locate", "Order"]),
        Some((CommandKind::Locate, Client::Cli))
    );
    assert_eq!(
        cli(&["note", "list"]),
        Some((CommandKind::Notes, Client::Cli))
    );
    assert_eq!(
        cli(&["deps", "--project", "/x"]),
        Some((CommandKind::Deps, Client::Cli))
    );
    assert_eq!(
        cli(&["gen", "c:Foo", "--dry"]),
        Some((CommandKind::Generate, Client::Cli))
    );
    assert_eq!(cli(&["undo"]), Some((CommandKind::Undo, Client::Cli)));
    assert_eq!(
        cli(&["src/Order.php"]),
        Some((CommandKind::Other, Client::Cli))
    );
    assert_eq!(cli(&[]), Some((CommandKind::Other, Client::Cli)));
    for skipped in [
        &["mcp"][..],
        &["mcp", "--auto-update"],
        &["stats"],
        &["hook", "post-edit"],
    ] {
        assert_eq!(cli(skipped), None, "{skipped:?}");
    }
    assert_eq!(
        classify(&args(&["refresh", "--project", "/x"]), Some("hook")),
        Some((CommandKind::Refresh, Client::Hook))
    );
    assert_eq!(classify(&args(&["refresh"]), Some("background")), None);
}

#[test]
fn recognizes_truncated_output_and_meters_characters() {
    assert!(truncated(
        "⚠ --format outline output would be 812 lines — showing the method index instead.\n\nidx"
    ));
    assert!(truncated(
        "⚠ A --format outline kimenete 812 sor lenne (40 fájl) — helyette summary.\n"
    ));
    assert!(!truncated("FILE app/Order.php\n  cancel L10-20\n"));

    // a karakterszám (UTF-8) és a megőrzött eleje; a kimenet maga változatlanul továbbmegy
    let mut meter = Meter::new(Vec::new());
    std::io::Write::write_all(&mut meter, "árvíz ⚠\n".as_bytes()).unwrap();
    assert_eq!(meter.chars(), 8);
    std::io::Write::write_all(&mut meter, &[b'x'; 2000]).unwrap();
    assert_eq!(meter.chars(), 2008);
    assert_eq!(meter.head().len(), 512);
    assert!(meter.head().starts_with("árvíz".as_bytes()));
}

/// A napi fájl csak a rögzített szótárból áll: kulcsok és szöveges értékek a parancs-, kliens- és hiba-nevek,
/// a mezőnevek, a nyelvek, a sávok, a napok és az átmenet-élek kulcsai; út-elválasztó és a fixture-projekt egyetlen
/// neve sem kerülhet bele — akkor sem, ha az MCP-hívások utat, projekt-aliast és nevekkel teli kimenetet kapnak.
/// Ugyanez igaz a helyi eseménynaplóra; a munkamenet-állapotban csak hexa hash-ek vannak.
#[test]
fn the_day_file_holds_no_paths_or_names() {
    let dir = temp_dir("privacy");
    let stats = McpStats::new(Recorder::at(dir.clone()));
    let secrets = [
        "SecretShop",
        "RefundController",
        "processRefund",
        "payment_status",
        "Nem található",
        "app/Http",
    ];
    let locate_output = "locate: 2 terms · 2 matches in 2 files · start here ↓\n\
                         1. app/Http/RefundController.php  ★★★\n   processRefund (exact method)\n\
                         2. app/Models/SecretShop.php  ★\n   'payment_status' (string)\n";
    let call = |tool, path: Option<&'static str>| ToolCall {
        tool,
        started: Instant::now(),
        project: Some("SecretShop"),
        path,
        symbol: Some("processRefund"),
        invalid: false,
    };
    stats.tool_call(&call(CommandKind::Locate, None), &Ok(locate_output.into()));
    stats.tool_call(
        &call(CommandKind::Outline, Some("app/Http/RefundController.php")),
        &Ok("FILE app/Http/RefundController.php\n  processRefund L10-20\n".into()),
    );
    stats.tool_call(
        &call(
            CommandKind::MethodBody,
            Some("/home/me/SecretShop/app/Http/X.php"),
        ),
        &Err("Nem található: /home/me/SecretShop/app/Http/X.php::processRefund".into()),
    );
    stats.tool_call(
        &call(CommandKind::FindCallers, Some("app")),
        &Err("Unknown project alias: SecretShop".into()),
    );

    // a havi fájl (`YYYY-MM.json`); mellette csak a helyi munkamenet-állapot és az eseménynapló mappája
    let names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    let files: Vec<PathBuf> = names
        .iter()
        .filter(|n| {
            n.strip_suffix(".json")
                .is_some_and(|m| Day::parse_month(m).is_some())
        })
        .map(|n| dir.join(n))
        .collect();
    assert_eq!(files.len(), 1, "{names:?}");
    for name in &names {
        assert!(
            files.contains(&dir.join(name)) || name == "session.json" || name == "events",
            "unexpected file: {name}"
        );
    }
    let text = std::fs::read_to_string(&files[0]).unwrap();
    assert!(!text.contains('/') && !text.contains('\\'), "{text}");
    for secret in secrets {
        assert!(!text.contains(secret), "{secret}: {text}");
    }
    // csak a rögzített szótár: minden JSON-szöveg (kulcs vagy érték) ismert név, nap vagy él-kulcs
    let known = known_words();
    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
    let mut strings = Vec::new();
    collect_strings(&value, &mut strings);
    for s in &strings {
        assert!(
            known.contains(&s.as_str())
                || Day::parse(s).is_some()
                || is_edge_key(s)
                || is_use_key(s),
            "unexpected text in the day file: {s}"
        );
    }
    // az átmenet-élek is rögzültek (a négy hívás egy munkamenet)
    let edges: Vec<&String> = value["edges"]
        .as_object()
        .unwrap()
        .values()
        .flat_map(|day| day.as_object().unwrap().keys())
        .collect();
    assert_eq!(edges.len(), 4, "{edges:?}");
    // az előző kimenet felhasználása: a munkamenet első hívása nem számít, a többi csak sávnevekkel
    let uses: Vec<&String> = value["uses"]
        .as_object()
        .unwrap()
        .values()
        .flat_map(|day| day.as_object().unwrap().keys())
        .collect();
    assert_eq!(uses.len(), 3, "{uses:?}");
    for key in [
        "locate>outline|f:hit1|s:hit",
        "outline>method_body|f:miss|s:hit",
        "method_body!>find_callers|f:-|s:-",
    ] {
        assert!(uses.iter().any(|k| *k == key), "{key}: {uses:?}");
    }
    // a helyi eseménynapló: ugyanaz a szótár, út és név nélkül
    let events_dir = dir.join("events");
    let logs: Vec<PathBuf> = std::fs::read_dir(&events_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(logs.len(), 1, "{logs:?}");
    let log = std::fs::read_to_string(&logs[0]).unwrap();
    assert_eq!(log.lines().count(), 4, "{log}");
    for line in log.lines() {
        let value: serde_json::Value = serde_json::from_str(line).unwrap();
        let mut strings = Vec::new();
        collect_strings(&value, &mut strings);
        for s in &strings {
            assert!(
                known.contains(&s.as_str()) || (s.ends_with('Z') && s.contains('T')),
                "unexpected text in the event log: {s}"
            );
        }
        for secret in secrets {
            assert!(!line.contains(secret), "{secret}: {line}");
        }
    }
    // a munkamenet-állapot: csak hexa hash-ek (és a kulcs), név és út nélkül
    let session = std::fs::read_to_string(dir.join("session.json")).unwrap();
    assert!(
        !session.contains('/') && !session.contains('\\'),
        "{session}"
    );
    for secret in secrets {
        assert!(!session.contains(secret), "{secret}: {session}");
    }
    // a találat is rögzült: az outline a lista első fájlját nyitotta meg
    let file = MonthFile::load(&files[0]);
    let locate = &file.days.values().next().unwrap()[&CommandKind::Locate];
    assert_eq!(locate.hits, [1, 0, 0, 0]);
    let errors = &file.days.values().next().unwrap()[&CommandKind::MethodBody].errors;
    assert_eq!(errors.get(&ErrorKind::NotFound), Some(&1));
    let _ = std::fs::remove_dir_all(dir);
}

/// A napi fájl, az eseménynapló és a beküldés rögzített szótára.
pub(super) fn known_words() -> Vec<&'static str> {
    CommandKind::ALL
        .iter()
        .map(|c| c.name())
        .chain(Client::ALL.iter().map(|c| c.name()))
        .chain(["usage", "not_found", "io", "unsupported", "failed"])
        .chain([
            "v",
            "days",
            "calls",
            "clients",
            "errors",
            "latency",
            "size",
            "truncated",
            "hits",
            "next_shown",
            "next_used",
            "in_size",
            "lang",
            "edges",
            "sessions",
            "count",
            "len",
            "dur",
            "rep_max",
            "uses",
        ])
        // az eseménynapló mezői és sávjai
        .chain([
            "t0", "ms", "cmd", "via", "ok", "err", "in", "out", "rel", "rep", "sess", "seq",
            "use_file", "use_sym",
        ])
        .chain(USE_BANDS)
        .chain(InputBucket::LABELS)
        .chain(SizeBucket::LABELS)
        .chain(REP_LABELS)
        .chain(Rel::ALL.iter().map(|r| r.name()))
        .chain(LANGUAGES)
        .collect()
}

/// Egy átmenet-él kulcsa a rögzített szerkezetben: `{előző}[!]>{ez}|{rel}|{szünet}` vagy `other`.
pub(super) fn is_edge_key(key: &str) -> bool {
    if key == "other" {
        return true;
    }
    let mut parts = key.split('|');
    let (Some(pair), Some(rel), Some(gap), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    let Some((from, to)) = pair.split_once('>') else {
        return false;
    };
    let names: Vec<&str> = CommandKind::ALL.iter().map(|c| c.name()).collect();
    let from = from.strip_suffix('!').unwrap_or(from);
    (from == "start" || names.contains(&from))
        && (to == "end" || names.contains(&to))
        && Rel::ALL.iter().any(|r| r.name() == rel)
        && GAP_BANDS.contains(&gap)
}

/// Egy újrahasznosítás-kulcs a rögzített szerkezetben: `{előző}[!]>{ez}|f:{sáv}|s:{sáv}` vagy `other`.
pub(super) fn is_use_key(key: &str) -> bool {
    if key == "other" {
        return true;
    }
    let mut parts = key.split('|');
    let (Some(pair), Some(file), Some(sym), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    let Some((from, to)) = pair.split_once('>') else {
        return false;
    };
    let names: Vec<&str> = CommandKind::ALL.iter().map(|c| c.name()).collect();
    let from = from.strip_suffix('!').unwrap_or(from);
    let file_ok = file
        .strip_prefix("f:")
        .is_some_and(|b| USE_BANDS.contains(&b));
    let sym_ok = sym
        .strip_prefix("s:")
        .is_some_and(|b| ["hit", "miss", "-"].contains(&b));
    names.contains(&from) && names.contains(&to) && file_ok && sym_ok
}

fn collect_strings(value: &serde_json::Value, out: &mut Vec<String>) {
    match value {
        serde_json::Value::String(s) => out.push(s.clone()),
        serde_json::Value::Array(items) => items.iter().for_each(|v| collect_strings(v, out)),
        serde_json::Value::Object(map) => {
            for (key, v) in map {
                out.push(key.clone());
                collect_strings(v, out);
            }
        }
        _ => {}
    }
}

/// Minden felsorolás-változat neve a szótárból (kisbetű, `_`): szabad szöveg nem fér a típusokba.
#[test]
fn every_variant_serializes_to_a_fixed_name() {
    let fixed = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_lowercase() || b == b'_');
    for kind in CommandKind::ALL {
        let json = serde_json::to_string(&kind).unwrap();
        assert_eq!(json, format!("\"{}\"", kind.name()));
        assert!(fixed(kind.name()));
    }
    for client in Client::ALL {
        assert_eq!(
            serde_json::to_string(&client).unwrap(),
            format!("\"{}\"", client.name())
        );
    }
    for kind in ErrorKind::ALL {
        let json = serde_json::to_string(&kind).unwrap();
        assert!(fixed(json.trim_matches('"')), "{json}");
    }
}
