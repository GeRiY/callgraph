//! Az előző kimenet felhasználásának (v0.9.5, `reuse.rs`) tesztjei: a kimenet elemei (utak, azonosítók), az
//! út-utótag egyezés és a hely-sávok, a `-` esetek, a CLI- és MCP-szimbólumok, a napi `uses` kulcsai, és hogy a
//! régi `session.json` / havi fájl / eseménysor az új mezők nélkül is betölthető.

use std::collections::BTreeMap;
use std::time::Instant;

use super::Recorder;
use super::buckets::{LatencyBucket, SizeBucket};
use super::day::Day;
use super::event::{Client, CommandKind, ErrorKind, Event, Outcome};
use super::events::{self, EventLine};
use super::mcp_probe::{McpStats, ToolCall};
use super::render_events;
use super::reuse::{
    FileUse, MAX_FILE_ITEMS, OutputItems, SAMPLE_BYTES, SymUse, Uses, file_hash, file_items,
    file_use, single_identifier, step, symbol_hash, symbol_items, symbol_use, use_key,
};
use super::session::{MAX_EDGES, OTHER_EDGE, SessionState, add_edge};
use super::store::{MonthFile, StatsStore};
use super::target::{Call, Target, cli_symbols};
use super::tests::temp_dir;
use super::tracking_tests::is_use_key;

/// 2026-11-10 01:00 UTC, ms.
const T: u64 = (1_794_268_800 + 3600) * 1000;
const KEY: [u8; 32] = [7; 32];

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| (*x).to_owned()).collect()
}

/// Egy fájl-cél a fájlrendszer nélkül (abszolút út).
fn file_target(path: &str) -> Target {
    Target {
        file: Some(path.to_owned()),
        dir: String::new(),
        project: String::new(),
        lang: None,
        input: None,
    }
}

fn call(
    command: CommandKind,
    client: Client,
    ok: bool,
    target: Option<Target>,
    symbols: Vec<String>,
    output: Option<&str>,
) -> Call {
    Call {
        event: Event {
            command,
            client,
            outcome: if ok {
                Outcome::Ok
            } else {
                Outcome::Error(ErrorKind::NotFound)
            },
            latency: LatencyBucket::from_millis(10),
            size: SizeBucket::from_chars(100),
            truncated: false,
        },
        end_ms: T,
        ms: 10,
        target,
        symbols,
        output: output.and_then(OutputItems::from_text),
    }
}

#[test]
fn file_items_strip_lines_prefixes_and_keep_first_appearance_order() {
    let text = "FILE ./app/Http/Foo.php:L12\n  bar() app\\Models\\User.php:34, app/Http/Foo.php again\n\
                (src/x.rs) v0.9.5 https://callgraph.eu Order::cancel $this->form.submit 1. 2.5\n\
                /a/b/c/d/e/f/g/h/i/j.vue:3 C:\\work\\shop\\src\\A.php:12 src/x.rs::main\n";
    assert_eq!(
        file_items(text),
        s(&[
            "app/Http/Foo.php",
            "app/Models/User.php",
            "src/x.rs",
            "c/d/e/f/g/h/i/j.vue",
            "C:/work/shop/src/A.php",
        ])
    );
    // legfeljebb 200 különböző elem
    let many: String = (0..250).map(|n| format!("f{n}.rs ")).collect();
    let items = file_items(&many);
    assert_eq!(items.len(), MAX_FILE_ITEMS);
    assert_eq!(items[0], "f0.rs");
    assert_eq!(
        items[MAX_FILE_ITEMS - 1],
        format!("f{}.rs", MAX_FILE_ITEMS - 1)
    );
}

#[test]
fn symbol_items_split_class_method_and_member_access() {
    let text = "Order::cancel $this->items obj.prop a->b 12abc ab_ x cancel Order";
    assert_eq!(
        symbol_items(text, 100),
        s(&[
            "Order", "cancel", "this", "items", "obj", "prop", "abc", "ab_"
        ])
    );
    assert_eq!(symbol_items(text, 2), s(&["Order", "cancel"]));
    assert_eq!(
        single_identifier(" STATUS_ACTIVE "),
        Some("STATUS_ACTIVE".into())
    );
    assert_eq!(single_identifier("Order not found"), None);
    assert_eq!(single_identifier("Order.cancel"), None);
    assert_eq!(single_identifier("ab"), None);
}

#[test]
fn only_the_first_64_kib_of_an_output_is_scanned() {
    let mut text = "x".repeat(SAMPLE_BYTES - 1);
    text.push_str("é late/File.php");
    let items = OutputItems::from_text(&text).unwrap();
    assert!(items.files.is_empty(), "{:?}", items.files);
    assert_eq!(OutputItems::from_text("  \n"), None);
}

#[test]
fn the_target_file_matches_by_path_suffix_with_position_bands() {
    let output = OutputItems::from_text(
        "1. app/Http/Foo.php  ★★★\n2. b.php\n3. c.php\n4. d.php\n5. e.php\n6. f.php\n7. Bare.php\n",
    )
    .unwrap();
    let out: Vec<String> = output.files.iter().map(|f| file_hash(&KEY, f)).collect();
    let band = |target: Option<Target>| file_use(&KEY, &out, target.as_ref());
    assert_eq!(
        band(Some(file_target("/x/proj/app/Http/Foo.php"))),
        FileUse::Hit1
    );
    assert_eq!(
        band(Target::from_text(Some("shop"), "app/Http/Foo.php")),
        FileUse::Hit1
    );
    assert_eq!(
        band(Some(file_target("C:\\x\\proj\\app\\Http\\Foo.php"))),
        FileUse::Hit1
    );
    assert_eq!(band(Some(file_target("/x/proj/c.php"))), FileUse::Hit5);
    assert_eq!(band(Some(file_target("/x/proj/e.php"))), FileUse::Hit5);
    assert_eq!(band(Some(file_target("/x/proj/f.php"))), FileUse::Hit);
    // a csupasz fájlnév is egyezés
    assert_eq!(band(Some(file_target("/elsewhere/Bare.php"))), FileUse::Hit);
    // más mappában ugyanaz a név, de a kimenetben teljes út állt: nem egyezés
    assert_eq!(
        band(Some(file_target("/x/proj/lib/Http/Foo.php"))),
        FileUse::Miss
    );
    assert_eq!(
        band(Some(file_target("/x/proj/app/Http/Other.php"))),
        FileUse::Miss
    );
    // mappa-cél vagy cél nélkül: `-`
    assert_eq!(band(Target::from_text(None, "app/Http")), FileUse::None);
    assert_eq!(band(None), FileUse::None);
    assert_eq!(
        [
            FileUse::Hit1,
            FileUse::Hit5,
            FileUse::Hit,
            FileUse::Miss,
            FileUse::None
        ]
        .map(FileUse::name),
        ["hit1", "hit5", "hit", "miss", "-"]
    );
}

#[test]
fn symbols_hit_or_miss_against_the_previous_output() {
    let out: Vec<String> = ["Order", "cancel"]
        .iter()
        .map(|w| symbol_hash(&KEY, w))
        .collect();
    assert_eq!(symbol_use(&KEY, &out, &s(&["cancel"])), SymUse::Hit);
    assert_eq!(
        symbol_use(&KEY, &out, &s(&["refund", "Order"])),
        SymUse::Hit
    );
    assert_eq!(symbol_use(&KEY, &out, &s(&["refund"])), SymUse::Miss);
    assert_eq!(symbol_use(&KEY, &out, &[]), SymUse::None);
    // más kulccsal más a hash
    assert_ne!(symbol_hash(&KEY, "cancel"), symbol_hash(&[8; 32], "cancel"));
    assert_eq!(symbol_hash(&KEY, "cancel").len(), 16);
    assert_ne!(symbol_hash(&KEY, "a.php"), file_hash(&KEY, "a.php"));
}

#[test]
fn no_previous_output_gives_dash_bands() {
    let target = || Some(file_target("/p/app/Order.php"));
    let mut state = SessionState::default();
    // nincs előző hívás
    let first = call(
        CommandKind::Outline,
        Client::Mcp,
        true,
        target(),
        s(&["cancel"]),
        Some("FILE app/Order.php\n  cancel L1-5\n"),
    );
    assert_eq!(step(&mut state, &KEY, &first), Uses::NONE);
    assert!(state.has_out);
    assert_eq!(state.out_files.len(), 1);
    // a hook háttér-frissítése nem számít, és a tárolt elemekhez sem nyúl
    let hook = call(
        CommandKind::Refresh,
        Client::Hook,
        true,
        None,
        vec![],
        Some("ok\n"),
    );
    assert_eq!(step(&mut state, &KEY, &hook), Uses::NONE);
    assert_eq!(state.out_files.len(), 1);
    let second = call(
        CommandKind::MethodBody,
        Client::Mcp,
        false,
        target(),
        s(&["cancel"]),
        Some("Not found: app/Order.php::cancel"),
    );
    assert_eq!(
        step(&mut state, &KEY, &second),
        Uses {
            file: FileUse::Hit1,
            sym: SymUse::Hit
        }
    );
    // az előző hibás volt: `-`, és a hibás kimenet elemei nem tárolódnak
    assert!(!state.has_out && state.out_files.is_empty() && state.out_syms.is_empty());
    let third = call(
        CommandKind::Outline,
        Client::Mcp,
        true,
        target(),
        s(&["cancel"]),
        None,
    );
    assert_eq!(step(&mut state, &KEY, &third), Uses::NONE);
    // üres kimenet után is `-`
    let fourth = call(
        CommandKind::Outline,
        Client::Mcp,
        true,
        target(),
        vec![],
        None,
    );
    assert_eq!(step(&mut state, &KEY, &fourth), Uses::NONE);
    // a hash-ekben nincs út vagy név
    let json = state.to_json();
    assert!(!json.contains("Order") && !json.contains('/'), "{json}");
}

#[test]
fn cli_symbols_come_from_method_suffixes_patterns_and_single_identifiers() {
    let sym = |command, args: &[&str]| cli_symbols(command, &s(args));
    assert_eq!(
        sym(
            CommandKind::Show,
            &["show", "--format", "outline", "app/Order.php::cancel"]
        ),
        s(&["cancel"])
    );
    assert_eq!(
        sym(CommandKind::Show, &["show", "src/main.rs::Foo::bar"]),
        s(&["Foo", "bar"])
    );
    assert_eq!(
        sym(
            CommandKind::Show,
            &["show", "src", "--grep", "(^|::|\\.|->)refund$"]
        ),
        s(&["refund"])
    );
    assert_eq!(
        sym(CommandKind::Show, &["show", "--grep=submit", "src"]),
        s(&["submit"])
    );
    assert_eq!(
        sym(
            CommandKind::Callers,
            &["callers", "Order::cancel", "--depth", "2"]
        ),
        s(&["Order", "cancel"])
    );
    assert_eq!(
        sym(
            CommandKind::Show,
            &["show", "src", "--const", "STATUS_ACTIVE"]
        ),
        s(&["STATUS_ACTIVE"])
    );
    assert!(sym(CommandKind::Show, &["show", "src", "--const", "not one"]).is_empty());
    assert!(
        sym(
            CommandKind::Show,
            &["show", "src", "--strings", "Order not found"]
        )
        .is_empty()
    );
    assert!(sym(CommandKind::Show, &["show", "app/Order.php"]).is_empty());
    assert!(sym(CommandKind::Locate, &["locate", "Order::cancel"]).is_empty());
}

/// CLI-folyamat a tárolón át: `outline` után `show fájl::bar` → a fájl az első, a `bar` benne volt; utána egy
/// hiányzó metódus → `miss`. Az eseménysorban és a napi `uses`-ban csak sávok.
#[test]
fn a_cli_method_after_an_outline_listing_it_is_a_hit() {
    let dir = temp_dir("reuse-cli");
    let store = StatsStore::new(dir.clone());
    let target = || Target::from_text(None, "app/Order.php");
    let show = |method: &str| {
        let arg = format!("app/Order.php::{method}");
        cli_symbols(CommandKind::Show, &s(&["show", arg.as_str()]))
    };
    let first = call(
        CommandKind::Outline,
        Client::Cli,
        true,
        target(),
        vec![],
        Some("FILE app/Order.php\n  bar L10-20\n  baz L22-30\n"),
    );
    let mut second = call(
        CommandKind::Show,
        Client::Cli,
        true,
        target(),
        show("bar"),
        Some("app/Order.php::bar L10-20\n10 │ fn bar() {}\n"),
    );
    let mut third = call(
        CommandKind::Show,
        Client::Cli,
        true,
        target(),
        show("nothere"),
        None,
    );
    second.end_ms = T + 1_000;
    third.end_ms = T + 2_000;
    for c in [&first, &second, &third] {
        store.record_call(c);
    }
    let day = Day::from_secs(T / 1000);
    let lines = events::read(&dir, day, day);
    let bands: Vec<(&str, &str)> = lines
        .iter()
        .map(|l| (l.use_file.as_str(), l.use_sym.as_str()))
        .collect();
    assert_eq!(bands, [("-", "-"), ("hit1", "hit"), ("hit1", "miss")]);
    let month = MonthFile::load(&dir.join(format!("{}.json", day.month_key())));
    let uses = &month.uses[&day];
    assert_eq!(uses.get("outline>show|f:hit1|s:hit"), Some(&1), "{uses:?}");
    assert_eq!(uses.get("show>show|f:hit1|s:miss"), Some(&1), "{uses:?}");
    assert_eq!(uses.len(), 2, "{uses:?}");
    assert_eq!(&store.day_uses(day), uses);
    for key in uses.keys() {
        assert!(is_use_key(key), "{key}");
    }
    let text = render_events(&store, day, 1);
    assert!(text.contains("use f:hit1 s:hit"), "{text}");
    let lang = crate::i18n::lang();
    assert!(
        text.contains(lang.pick(
            "outline → show: 1 (file from previous output: hit1 1 · symbol: hit 1)",
            "outline → show: 1 (fájl az előző kimenetből: hit1 1 · szimbólum: hit 1)"
        )),
        "{text}"
    );
    let month_text =
        std::fs::read_to_string(dir.join(format!("{}.json", day.month_key()))).unwrap();
    for secret in ["Order", "bar", "nothere", "app/"] {
        assert!(!month_text.contains(secret), "{secret}: {month_text}");
    }
    let _ = std::fs::remove_dir_all(dir);
}

/// MCP: a `ToolCall::symbol` (metódusnév, hívó-minta) az előző eszköz-eredményhez mérve.
#[test]
fn mcp_tool_call_symbols_are_compared_with_the_previous_result() {
    let dir = temp_dir("reuse-mcp");
    let stats = McpStats::new(Recorder::at(dir.clone()));
    let call = |tool, path: Option<&'static str>, symbol: Option<&'static str>| ToolCall {
        tool,
        started: Instant::now(),
        project: Some("shop"),
        path,
        symbol,
        invalid: false,
    };
    stats.tool_call(
        &call(CommandKind::Outline, Some("app/Order.php"), None),
        &Ok("FILE app/Order.php\n  cancel L1-5\n".into()),
    );
    stats.tool_call(
        &call(
            CommandKind::MethodBody,
            Some("app/Order.php"),
            Some("cancel"),
        ),
        &Ok("app/Order.php::cancel L1-5\n1 │ fn cancel() {}\n".into()),
    );
    stats.tool_call(
        &call(
            CommandKind::FindCallers,
            Some("app"),
            Some("(^|::|\\.|->)refund$"),
        ),
        &Ok("root: .\n".into()),
    );
    let day = Day::today();
    let lines = events::read(&dir, day.minus(1), day);
    let bands: Vec<(&str, &str)> = lines
        .iter()
        .map(|l| (l.use_file.as_str(), l.use_sym.as_str()))
        .collect();
    assert_eq!(bands, [("-", "-"), ("hit1", "hit"), ("-", "miss")]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn use_keys_follow_the_edge_naming_and_cap() {
    let uses = Uses {
        file: FileUse::Hit5,
        sym: SymUse::Miss,
    };
    let key = use_key("show!", CommandKind::Outline, uses);
    assert_eq!(key, "show!>outline|f:hit5|s:miss");
    assert!(is_use_key(&key));
    assert!(is_use_key(&use_key(
        "locate",
        CommandKind::Show,
        Uses::NONE
    )));
    assert!(!is_use_key("start>show|f:-|s:-"));
    assert!(!is_use_key("show>outline|f:app/A.php|s:-"));
    let mut map = BTreeMap::new();
    for n in 0..=MAX_EDGES {
        add_edge(&mut map, &format!("show>outline|f:hit{n}|s:-"));
    }
    assert_eq!(map.len(), MAX_EDGES + 1);
    assert_eq!(map[OTHER_EDGE], 1);
}

#[test]
fn old_files_without_the_reuse_fields_still_load() {
    let old_session =
        r#"{"key":"","sess":1,"seq":2,"start_ms":1,"end_ms":2,"last_failed":false,"touches":{}}"#;
    let state: SessionState = serde_json::from_str(old_session).unwrap();
    assert!(!state.has_out && state.out_files.is_empty() && state.out_syms.is_empty());
    let text = SessionState::default().to_json();
    assert!(
        !text.contains("out_files") && !text.contains("out_syms"),
        "{text}"
    );

    let old_month = r#"{"v":1,"days":{},"edges":{"2026-11-01":{"start>show|none|-":1}}}"#;
    let month: MonthFile = serde_json::from_str(old_month).unwrap();
    assert!(month.uses.is_empty());
    assert!(!month.to_json().contains("uses"));

    let old_line = r#"{"t0":"2026-11-10T01:00:00.000Z","ms":5,"cmd":"show","via":"cli","ok":true,"err":null,"lang":null,"in":null,"out":"<500","rel":"none","rep":null,"sess":1,"seq":1}"#;
    let line: EventLine = serde_json::from_str(old_line).unwrap();
    assert_eq!((line.use_file.as_str(), line.use_sym.as_str()), ("-", "-"));
}
