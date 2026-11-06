//! Az MCP-frissítés tesztjei ideiglenes skill-mappában, hamis frissítővel / lekérdezéssel — hálózat és valódi
//! folyamat-indítás nélkül.

use std::cell::Cell;
use std::sync::atomic::AtomicUsize;

use super::*;
use crate::mcp::params::OutlineParams;
use crate::update::check::STATE_FILE;

const NOW: u64 = 1_800_000_000;
const DAY: u64 = crate::update::check::INTERVAL_SECS;

fn v(text: &str) -> Version {
    Version::parse(text).unwrap()
}

fn temp_skill(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("callgraph-mcp-au-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn home(dir: &Path) -> SkillHome {
    SkillHome::new(
        dir.to_path_buf(),
        "/skill".into(),
        dir.join("projects.json"),
    )
}

fn on() -> CheckEnv {
    CheckEnv {
        setting: None,
        ci: false,
        in_skill: true,
    }
}

fn load(dir: &Path) -> CheckState {
    CheckState::load(&dir.join(STATE_FILE))
}

/// Hamis frissítő: megszámolja a hívásokat, és a megadott eredményt adja.
struct FakeUpdater {
    calls: Cell<u32>,
    result: Result<Option<(Version, PathBuf)>, String>,
}

impl FakeUpdater {
    fn new(result: Result<Option<(Version, PathBuf)>, String>) -> Self {
        Self {
            calls: Cell::new(0),
            result,
        }
    }
}

impl Updater for FakeUpdater {
    fn update(&self) -> Result<Option<(Version, PathBuf)>, String> {
        self.calls.set(self.calls.get() + 1);
        self.result.clone()
    }
}

fn auto(dir: &Path, env: CheckEnv, reexec: bool) -> AutoUpdate {
    AutoUpdate::new(&home(dir), env, reexec).with_current(v("0.1.0"))
}

#[test]
fn auto_update_runs_once_a_day_and_reports_the_install() {
    let dir = temp_skill("daily");
    let binary = dir.join("bin/linux-x86_64/callgraph");
    let updated = FakeUpdater::new(Ok(Some((v("0.2.0"), binary.clone()))));
    assert_eq!(
        auto(&dir, on(), false).run(NOW, &updated),
        Outcome::Updated {
            version: v("0.2.0"),
            binary
        }
    );
    assert_eq!(updated.calls.get(), 1);
    // a kísérlet ideje a frissítés előtt mentve: ugyanazon a napon nincs újabb próbálkozás
    assert_eq!(load(&dir).auto_update_at, Some(NOW));
    let again = FakeUpdater::new(Ok(None));
    assert_eq!(
        auto(&dir, on(), false).run(NOW + 60, &again),
        Outcome::Skipped
    );
    assert_eq!(again.calls.get(), 0);
    // egy nap múlva újra megnézi
    assert_eq!(
        auto(&dir, on(), false).run(NOW + DAY, &again),
        Outcome::UpToDate
    );
    assert_eq!(again.calls.get(), 1);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn reexec_marker_prevents_a_loop() {
    let dir = temp_skill("reexec");
    let updater = FakeUpdater::new(Ok(Some((v("0.2.0"), dir.join("x")))));
    assert_eq!(auto(&dir, on(), true).run(NOW, &updater), Outcome::Skipped);
    assert_eq!(updater.calls.get(), 0);
    assert!(!dir.join(STATE_FILE).exists());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn opt_out_ci_and_dev_builds_touch_nothing() {
    let dir = temp_skill("optout");
    for env in [
        CheckEnv {
            setting: Some("0".into()),
            ..on()
        },
        CheckEnv { ci: true, ..on() },
        CheckEnv {
            in_skill: false,
            ..on()
        },
    ] {
        let updater = FakeUpdater::new(Ok(Some((v("0.2.0"), dir.join("x")))));
        assert_eq!(auto(&dir, env, false).run(NOW, &updater), Outcome::Skipped);
        assert_eq!(updater.calls.get(), 0);
        assert!(!dir.join(STATE_FILE).exists());
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn failure_continues_and_is_not_retried_the_same_day() {
    let dir = temp_skill("fail");
    // pl. eltérő SHA-256 (a `self-update` tesztjei mutatják, hogy ilyenkor semmi nem települ)
    let failing = FakeUpdater::new(Err("SHA-256 mismatch\nnothing was installed".into()));
    let outcome = auto(&dir, on(), false).run(NOW, &failing);
    let Outcome::Failed(message) = &outcome else {
        panic!("{outcome:?}");
    };
    let line = failed_line(&v("0.1.0"), message);
    assert_eq!(line.lines().count(), 1, "{line}");
    assert!(line.contains("0.1.0") && line.contains("nothing was installed"));
    // offline / rate limit: holnapig nem próbálja újra (nem lassít minden indítást)
    assert_eq!(
        auto(&dir, on(), false).run(NOW + 60, &failing),
        Outcome::Skipped
    );
    assert_eq!(failing.calls.get(), 1);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn unwritable_state_means_no_attempt() {
    let dir = temp_skill("ro");
    // az állapotfájl helyén egy mappa: a mentés nem sikerül
    std::fs::create_dir_all(dir.join(STATE_FILE).join("x")).unwrap();
    let updater = FakeUpdater::new(Ok(None));
    let outcome = auto(&dir, on(), false).run(NOW, &updater);
    assert!(matches!(outcome, Outcome::Failed(_)), "{outcome:?}");
    assert_eq!(updater.calls.get(), 0);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn restart_keeps_the_arguments_and_sets_the_marker() {
    let command = restart_command(Path::new("/skill/bin/linux-x86_64/callgraph"));
    assert_eq!(
        command.get_program(),
        std::ffi::OsStr::new("/skill/bin/linux-x86_64/callgraph")
    );
    let marker = command
        .get_envs()
        .find(|(k, _)| *k == std::ffi::OsStr::new(REEXEC_ENV))
        .and_then(|(_, v)| v);
    assert_eq!(marker, Some(std::ffi::OsStr::new("1")));
    assert_eq!(
        command.get_args().count(),
        std::env::args_os().skip(1).count()
    );
}

// --- értesítés ---------------------------------------------------------------------------------------------------

/// Hamis végrehajtó: minden hívásra ugyanazt a szöveget adja.
struct Echo;

impl QueryRunner for Echo {
    fn run(&self, _query: &ShowQuery) -> Result<String, String> {
        Ok("outline\n".into())
    }

    fn projects(&self) -> Result<String, String> {
        Ok("app\t/srv/app\n".into())
    }
}

/// Egy lekérdezés, amelyet a hamis végrehajtó nem is néz.
fn query() -> ShowQuery {
    ShowQuery::outline(OutlineParams {
        path: "a.php".into(),
        ..OutlineParams::default()
    })
    .unwrap()
}

/// Lekérdezés, amelynek nem szabad lefutnia.
fn no_lookup() -> Lookup {
    Box::new(|| -> Result<Option<Version>, ()> { panic!("no lookup expected") })
}

#[test]
fn first_tool_result_carries_the_notice_and_later_ones_do_not() {
    let dir = temp_skill("notice");
    let fresh = CheckState {
        checked_at: now_secs(),
        latest: Some("0.2.0".into()),
        ..CheckState::default()
    };
    fresh.save(&dir.join(STATE_FILE)).unwrap();
    let runner = with_notice_from(Arc::new(Echo), &home(&dir), &on(), v("0.1.0"), no_lookup());

    let first = runner.run(&query()).unwrap();
    assert!(first.starts_with("outline\n"), "{first}");
    let notice = first.strip_prefix("outline\n").unwrap();
    assert_eq!(notice.lines().count(), 1, "{first}");
    assert!(
        notice.starts_with("ℹ callgraph 0.2.0") && notice.contains("0.1.0"),
        "{notice}"
    );
    assert!(
        notice.contains("bash /skill/callgraph.sh update") && notice.contains("--auto-update"),
        "{notice}"
    );
    // a többi eredmény változatlan
    assert_eq!(runner.run(&query()).unwrap(), "outline\n");
    assert_eq!(runner.projects().unwrap(), "app\t/srv/app\n");
    // az értesítés rögzítve (közös a callgraph.sh értesítésével: 24 óráig egy új munkamenet sem szól)
    assert_eq!(load(&dir).notified_version.as_deref(), Some("0.2.0"));
    let next = with_notice_from(Arc::new(Echo), &home(&dir), &on(), v("0.1.0"), no_lookup());
    assert_eq!(next.projects().unwrap(), "app\t/srv/app\n");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn no_notice_when_up_to_date_or_disabled() {
    let dir = temp_skill("nonotice");
    let state = CheckState {
        checked_at: now_secs(),
        latest: Some("0.1.0".into()),
        ..CheckState::default()
    };
    state.save(&dir.join(STATE_FILE)).unwrap();
    let runner = with_notice_from(Arc::new(Echo), &home(&dir), &on(), v("0.1.0"), no_lookup());
    assert_eq!(runner.run(&query()).unwrap(), "outline\n");

    let newer = CheckState {
        latest: Some("0.2.0".into()),
        ..state
    };
    newer.save(&dir.join(STATE_FILE)).unwrap();
    let off = CheckEnv {
        setting: Some("off".into()),
        ..on()
    };
    let runner = with_notice_from(Arc::new(Echo), &home(&dir), &off, v("0.1.0"), no_lookup());
    assert_eq!(runner.run(&query()).unwrap(), "outline\n");
    assert_eq!(CheckState::load(&dir.join(STATE_FILE)), newer);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn due_lookup_runs_in_the_background_once() {
    let dir = temp_skill("lookup");
    // valódi idő: a háttérszál a `record_latest`-tel a valódi időt írja be
    let now = now_secs();
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&calls);
    let lookup: Lookup = Box::new(move || -> Result<Option<Version>, ()> {
        counter.fetch_add(1, Ordering::SeqCst);
        Ok(Some(Version::parse("0.2.0").unwrap()))
    });
    let notice = UpdateNotice::start(&home(&dir), &on(), now, v("0.1.0"), lookup).unwrap();
    // a checked_at azonnal beírva (a párhuzamos indulások ne kérdezzenek le újra)
    assert!(load(&dir).checked_at >= now);
    notice.wait_for_lookup();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(load(&dir).latest.as_deref(), Some("0.2.0"));
    let line = notice.take(now + 1).unwrap();
    assert!(line.starts_with("ℹ callgraph 0.2.0"), "{line}");
    assert_eq!(notice.take(now + 2), None);

    // friss állapot: nincs újabb lekérdezés, és már szóltunk
    let again = UpdateNotice::start(&home(&dir), &on(), now + 3, v("0.1.0"), no_lookup()).unwrap();
    assert_eq!(again.take(now + 4), None);
    let _ = std::fs::remove_dir_all(dir);
}
