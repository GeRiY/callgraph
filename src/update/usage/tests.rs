use super::*;
use std::sync::{Arc, Mutex as StdMutex};

/// Ideiglenes mappa a teszthez (a projekt többi tesztjének mintájára, külső crate nélkül).
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("callgraph-usage-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// 2026-11-12 (csütörtök) 10:00 UTC.
const NOW: u64 = 1_794_477_600;

fn day(text: &str) -> i64 {
    parse_day(text).expect(text)
}

#[test]
fn civil_round_trips_and_rejects_bad_days() {
    for text in [
        "1970-01-01",
        "2000-02-29",
        "2024-12-31",
        "2026-11-12",
        "2027-01-01",
    ] {
        assert_eq!(format_day(day(text)), text);
    }
    assert_eq!(day("1970-01-01"), 0);
    assert_eq!(format_day(day_number(NOW)), "2026-11-12");
    for bad in ["2026-02-30", "2026-13-01", "2026-00-10", "x", "", "2026-11"] {
        assert_eq!(parse_day(bad), None, "{bad}");
    }
}

#[test]
fn first_check_ever_sets_every_flag() {
    let flags = decide(None, day("2026-11-12")).unwrap();
    assert_eq!(
        flags,
        Flags {
            new: true,
            week: true,
            month: true
        }
    );
}

#[test]
fn same_day_sends_nothing() {
    assert_eq!(decide(Some(day("2026-11-12")), day("2026-11-12")), None);
}

#[test]
fn next_day_same_week_and_month() {
    let flags = decide(Some(day("2026-11-10")), day("2026-11-12")).unwrap();
    assert_eq!(
        flags,
        Flags {
            new: false,
            week: false,
            month: false
        }
    );
}

#[test]
fn week_boundary_is_monday() {
    // 2026-11-15 vasárnap, 2026-11-16 hétfő
    let sunday_to_monday = decide(Some(day("2026-11-15")), day("2026-11-16")).unwrap();
    assert!(sunday_to_monday.week && !sunday_to_monday.month);
    let monday_to_sunday = decide(Some(day("2026-11-16")), day("2026-11-22")).unwrap();
    assert!(!monday_to_sunday.week);
}

#[test]
fn month_and_year_boundaries() {
    let new_month = decide(Some(day("2026-10-31")), day("2026-11-01")).unwrap();
    assert!(new_month.month && !new_month.new);
    // 2026-12-31 csütörtök és 2027-01-01 péntek ugyanazon az ISO-héten: új hónap, nem új hét
    let new_year = decide(Some(day("2026-12-31")), day("2027-01-01")).unwrap();
    assert!(new_year.month && !new_year.week);
}

#[test]
fn clock_set_back_counts_as_a_new_day() {
    let flags = decide(Some(day("2026-11-20")), day("2026-11-12")).unwrap();
    assert!(!flags.new && flags.week);
}

#[test]
fn opt_out_switches() {
    let env = |pairs: &'static [(&'static str, &'static str)]| {
        move |k: &str| {
            pairs
                .iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| (*v).to_owned())
        }
    };
    assert!(!opted_out(env(&[])));
    assert!(!opted_out(env(&[
        ("CALLGRAPH_COUNT", "1"),
        ("DO_NOT_TRACK", "0"),
        ("CI", "")
    ])));
    for off in ["0", "off", "false", "no", " OFF "] {
        let pairs: &'static [(&str, &str)] = Box::leak(Box::new([("CALLGRAPH_COUNT", off)]));
        assert!(opted_out(env(pairs)), "{off}");
    }
    assert!(opted_out(env(&[("CALLGRAPH_UPDATE_CHECK", "0")])));
    assert!(opted_out(env(&[("CALLGRAPH_TELEMETRY", "off")])));
    assert!(!opted_out(env(&[
        ("CALLGRAPH_UPDATE_CHECK", "1"),
        ("CALLGRAPH_TELEMETRY", "1")
    ])));
    assert!(opted_out(env(&[("DO_NOT_TRACK", "1")])));
    assert!(opted_out(env(&[("DO_NOT_TRACK", "true")])));
    assert!(opted_out(env(&[("CI", "true")])));
}

#[test]
fn query_has_only_version_platform_and_flags() {
    let q = check_query(
        "0.9.4",
        "linux-x86_64",
        Via::Mcp,
        Flags {
            new: true,
            week: false,
            month: true,
        },
    );
    assert_eq!(q, "v=0.9.4&p=linux-x86_64&via=mcp&n=1&w=0&m=1");
}

#[derive(Clone, Default)]
struct Recorder(Arc<StdMutex<Vec<String>>>);

impl CheckSender for Recorder {
    fn send(&self, query: String) {
        self.0.lock().unwrap().push(query);
    }
}

fn check_in(dir: &Path) -> (UsageCheck, Recorder) {
    let rec = Recorder::default();
    let check = UsageCheck::new(
        Some(dir.join(STATE_FILE)),
        "0.9.4".into(),
        "darwin-arm64".into(),
        Box::new(rec.clone()),
    );
    (check, rec)
}

#[test]
fn once_per_day_across_cli_and_mcp() {
    let dir = TempDir::new();
    let (cli, rec) = check_in(dir.path());
    cli.after_use(NOW, Via::Cli);
    cli.after_use(NOW + 60, Via::Cli);
    // egy másik folyamat (pl. az MCP-szerver) ugyanazon a napon: a fájl szerint már volt
    let (mcp, mcp_rec) = check_in(dir.path());
    mcp.after_use(NOW + 120, Via::Mcp);
    assert_eq!(
        *rec.0.lock().unwrap(),
        vec!["v=0.9.4&p=darwin-arm64&via=cli&n=1&w=1&m=1".to_owned()]
    );
    assert!(mcp_rec.0.lock().unwrap().is_empty());
    // másnap az MCP küld (ugyanazon a héten, hónapban)
    mcp.after_use(NOW + 86_400, Via::Mcp);
    assert_eq!(
        *mcp_rec.0.lock().unwrap(),
        vec!["v=0.9.4&p=darwin-arm64&via=mcp&n=0&w=0&m=0".to_owned()]
    );
    assert_eq!(
        load_last(&dir.path().join(STATE_FILE)),
        Some(day_number(NOW + 86_400))
    );
}

#[test]
fn state_file_holds_only_the_day() {
    let dir = TempDir::new();
    let (check, _) = check_in(dir.path());
    check.after_use(NOW, Via::Cli);
    let text = std::fs::read_to_string(dir.path().join(STATE_FILE)).unwrap();
    assert_eq!(text, r#"{"last_day":"2026-11-12"}"#);
}

#[test]
fn corrupt_state_counts_as_new_install_and_is_repaired() {
    let dir = TempDir::new();
    std::fs::write(dir.path().join(STATE_FILE), "{broken").unwrap();
    let (check, rec) = check_in(dir.path());
    check.after_use(NOW, Via::Cli);
    assert_eq!(rec.0.lock().unwrap().len(), 1);
    assert_eq!(
        load_last(&dir.path().join(STATE_FILE)),
        Some(day_number(NOW))
    );
}

#[test]
fn unwritable_state_sends_nothing() {
    let dir = TempDir::new();
    let missing = dir.path().join("no-such-dir");
    let (check, rec) = check_in(&missing);
    check.after_use(NOW, Via::Cli);
    check.after_use(NOW + 10, Via::Cli);
    assert!(rec.0.lock().unwrap().is_empty());
}

#[test]
fn disabled_does_nothing() {
    let dir = TempDir::new();
    let check = UsageCheck::disabled();
    check.after_use(NOW, Via::Cli);
    assert!(!dir.path().join(STATE_FILE).exists());
}

#[test]
fn direct_download_maps_only_our_urls() {
    let ours = download_url("v0.9.4", "callgraph-linux-x86_64.zip", "0.9.3");
    assert_eq!(
        ours,
        "https://update.callgraph.eu/v1/download/v0.9.4/callgraph-linux-x86_64.zip?k=update&v=0.9.3"
    );
    assert_eq!(
        direct_download_url(&ours).as_deref(),
        Some(
            "https://github.com/GeRiY/callgraph/releases/download/v0.9.4/callgraph-linux-x86_64.zip"
        )
    );
    assert_eq!(
        direct_download_url(
            "https://update.callgraph.eu/v1/download/latest/callgraph-darwin-arm64.zip?k=install"
        )
        .as_deref(),
        Some(
            "https://github.com/GeRiY/callgraph/releases/latest/download/callgraph-darwin-arm64.zip"
        )
    );
    for other in [
        "https://github.com/GeRiY/callgraph/releases/download/v0.9.4/callgraph-linux-x86_64.zip",
        "https://update.callgraph.eu/v1/latest?v=1",
        "https://update.callgraph.eu/v1/download/v0.9.4/a/b",
        "https://update.callgraph.eu/v1/download//x.zip",
        "https://evil.example/v1/download/v0.9.4/x.zip",
    ] {
        assert_eq!(direct_download_url(other), None, "{other}");
    }
}

#[test]
fn latest_url_carries_version_and_platform() {
    assert_eq!(
        latest_url("0.9.3", "windows-x86_64"),
        "https://update.callgraph.eu/v1/latest?v=0.9.3&p=windows-x86_64"
    );
}
