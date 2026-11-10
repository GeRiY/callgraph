//! Frissítés az MCP-szerverből, két módon (a stdout a protokollé: ide soha semmi nem kerül, csak a stderr-re).
//!
//! 1. `callgraph mcp --auto-update` (opt-in): a kézfogás ELŐTT, naponta legfeljebb egyszer (`update-check.json`,
//!    `decide_auto`) megnézi a legutóbbi kiadást; ha újabb, letölti, a `SHA256SUMS` szerint ellenőrzi (kötelező) és
//!    telepíti — pontosan mint a `self-update` (bináris + skill-fájlok, a felhasználói adat marad) —, majd az új
//!    binárissal indítja újra magát, ugyanazokkal az argumentumokkal. Az újraindított folyamat a
//!    `CALLGRAPH_MCP_REEXEC=1` jelzőt kapja, és kihagyja a lépést (nincs hurok). Unixon `exec` (ugyanaz a PID,
//!    ugyanaz a stdio); Windowson nincs `exec`: az új exe (a telepítő átnevezése után már az eredeti úton) gyerekként
//!    indul az örökölt stdio-val, a régi folyamat megvárja és a kilépési kódját továbbadja. Bármilyen hibánál
//!    (offline, rate limit, eltérő összeg, jogosultság) egy sor a stderr-re, és a szerver a jelenlegi verzióval indul.
//! 2. Kapcsoló nélkül: naponta legfeljebb egy háttér-lekérdezés (szál, rövid időkorláttal; a kérést nem várja meg
//!    semmi), és ha újabb verzió van, az első sikeres eszköz-eredmény végére egy egysoros értesítés kerül
//!    (munkamenetenként legfeljebb egyszer; verziónként 24 óránként, közösen a `callgraph.sh` értesítésével).
//!
//! Mindkettő kikapcsolva: `CALLGRAPH_UPDATE_CHECK=0`, `CI`, illetve skill-telepítésen kívül (`CheckEnv`).
//! Csak a skill-mappába ír (állapotfájl, telepítés), projekt-fájlokba soha.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use super::request::ShowQuery;
use super::runner::QueryRunner;
use crate::cli::commands::SelfUpdateCommand;
use crate::skill::SkillHome;
use crate::update::check::{CheckEnv, CheckState, decide, decide_auto, now_secs, record_latest};
use crate::update::{BACKGROUND_TIMEOUT, HttpFetcher, Platform, Version, default_source};

/// A kapcsoló az MCP-kliens konfigurációjában: `"args": ["mcp", "--auto-update"]`.
pub(crate) const AUTO_UPDATE_FLAG: &str = "--auto-update";

/// Az újraindított folyamat jelzője: ha be van állítva, nincs frissítési lépés (nem lehet hurok).
pub(crate) const REEXEC_ENV: &str = "CALLGRAPH_MCP_REEXEC";

// ---------------------------------------------------------------------------------------------------------------
// 1. `--auto-update`

/// A frissítés végrehajtója (DI: a teszt hálózat nélküli hamisítványt ad): `Ok(Some((verzió, új bináris)))`,
/// ha telepített; `Ok(None)`, ha nincs újabb; `Err(üzenet)` bármilyen hibánál (akkor semmi nem települt).
pub(crate) trait Updater {
    fn update(&self) -> Result<Option<(Version, PathBuf)>, String>;
}

/// Az éles végrehajtó: a `self-update` logikája rövid időkorlátokkal; csak hívásra jön létre.
struct SelfUpdater<'a>(&'a SkillHome);

impl Updater for SelfUpdater<'_> {
    fn update(&self) -> Result<Option<(Version, PathBuf)>, String> {
        SelfUpdateCommand::for_auto_update(self.0.clone())
            .update_if_newer()
            .map_err(|e| e.message)
    }
}

/// Az indításkori lépés eredménye.
#[derive(Debug, PartialEq)]
pub(crate) enum Outcome {
    /// Nem kellett próbálkozni (kikapcsolva, újraindított folyamat, ma már próbálta, friss gyorsítótár).
    Skipped,
    /// Megnézte: nincs újabb.
    UpToDate,
    /// Telepítette: újra kell indulni az új binárissal.
    Updated { version: Version, binary: PathBuf },
    /// Nem sikerült (semmi nem települt): a jelenlegi verzióval folytatjuk.
    Failed(String),
}

/// Az `--auto-update` döntése a függőségeivel (állapotfájl, környezet, újraindítás-jelző, saját verzió).
pub(crate) struct AutoUpdate {
    file: PathBuf,
    env: CheckEnv,
    reexec: bool,
    current: Version,
}

impl AutoUpdate {
    /// A valódi környezet: a skill-mappa állapotfájlja, a folyamat környezete és a `CALLGRAPH_MCP_REEXEC`.
    pub(crate) fn from_env(home: &SkillHome) -> Self {
        let reexec = std::env::var_os(REEXEC_ENV).is_some_and(|v| !v.is_empty());
        Self::new(home, CheckEnv::from_process(), reexec)
    }

    pub(crate) fn new(home: &SkillHome, env: CheckEnv, reexec: bool) -> Self {
        Self {
            file: home.update_check_file(),
            env,
            reexec,
            current: Version::current(),
        }
    }

    #[cfg(test)]
    fn with_current(mut self, current: Version) -> Self {
        self.current = current;
        self
    }

    /// A lépés: újraindított folyamatban és kikapcsolt ellenőrzésnél semmi (hálózat sem); különben naponta egyszer
    /// a frissítés. A kísérlet idejét a hálózat ELŐTT menti; ha nem tudja menteni, nem próbálkozik (különben minden
    /// indítás próbálkozna — és a telepítés sem sikerülne).
    pub(crate) fn run(&self, now: u64, updater: &dyn Updater) -> Outcome {
        if self.reexec || !self.env.enabled() {
            return Outcome::Skipped;
        }
        let state = CheckState::load(&self.file);
        let Some(next) = decide_auto(&state, now, &self.current) else {
            return Outcome::Skipped;
        };
        if let Err(e) = next.save(&self.file) {
            return Outcome::Failed(crate::tf!(
                "cannot write {}: {e}",
                "nem írható: {}: {e}",
                self.file.display()
            ));
        }
        match updater.update() {
            Ok(Some((version, binary))) => Outcome::Updated { version, binary },
            Ok(None) => Outcome::UpToDate,
            Err(message) => Outcome::Failed(message),
        }
    }
}

/// `callgraph mcp --auto-update`: a kézfogás előtti lépés. Sikeres frissítés után nem tér vissza (újraindul az új
/// binárissal); minden más esetben visszatér, és a hívó a jelenlegi verzióval szolgál ki. Csak a stderr-re ír.
pub(crate) fn auto_update(home: &SkillHome, err: &mut dyn Write) {
    let auto = AutoUpdate::from_env(home);
    let current = &auto.current;
    match auto.run(now_secs(), &SelfUpdater(home)) {
        Outcome::Updated { version, binary } => {
            let _ = writeln!(err, "{}", updated_line(current, &version));
            let e = restart(&binary);
            let _ = writeln!(err, "{}", restart_failed_line(&version, &e.to_string()));
        }
        Outcome::Failed(message) => {
            let _ = writeln!(err, "{}", failed_line(current, &message));
        }
        Outcome::Skipped | Outcome::UpToDate => {}
    }
}

/// Egysoros alak (a hibaüzenetek több sorosak is lehetnek; a kliensek soronként naplózzák a stderr-t).
fn one_line(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn updated_line(current: &Version, version: &Version) -> String {
    crate::tf!(
        "callgraph: auto-update {current} → {version} installed; restarting the MCP server",
        "callgraph: automatikus frissítés {current} → {version} telepítve; az MCP-szerver újraindul"
    )
}

fn failed_line(current: &Version, message: &str) -> String {
    crate::tf!(
        "callgraph: auto-update skipped, continuing with {current}: {}",
        "callgraph: az automatikus frissítés elmaradt, a(z) {current} fut tovább: {}",
        one_line(message)
    )
}

fn restart_failed_line(version: &Version, error: &str) -> String {
    crate::tf!(
        "callgraph: could not restart with {version} ({}); this session uses the previous version, the next one starts with {version}",
        "callgraph: nem sikerült újraindulni a(z) {version} verzióval ({}); ez a munkamenet az előző verziót használja, a következő már a(z) {version}-t",
        one_line(error)
    )
}

/// Az újraindítás parancsa: az új bináris (a Windows-os `\\?\` előtag nélkül), a saját argumentumainkkal és a
/// hurok elleni jelzővel.
fn restart_command(binary: &Path) -> Command {
    let program = binary.to_str().map_or_else(
        || binary.to_path_buf(),
        |s| PathBuf::from(crate::query::strip_verbatim(s)),
    );
    let mut command = Command::new(program);
    command
        .args(std::env::args_os().skip(1))
        .env(REEXEC_ENV, "1");
    command
}

/// Unix: `exec` — a folyamat helyben lesz az új bináris (PID, stdin/stdout/stderr marad); csak hibánál tér vissza.
#[cfg(unix)]
fn restart(binary: &Path) -> std::io::Error {
    use std::os::unix::process::CommandExt;
    restart_command(binary).exec()
}

/// Windows (nincs `exec`): az új exe gyerekként, örökölt stdio-val; megvárjuk, és a kilépési kódjával lépünk ki.
/// A szülő addig nem olvas a stdin-ből és nem ír a stdout-ra, így a kliens közvetlenül a gyerekkel beszél.
/// Csak akkor tér vissza, ha a gyerek el sem indult.
#[cfg(not(unix))]
fn restart(binary: &Path) -> std::io::Error {
    match restart_command(binary).status() {
        Ok(status) => std::process::exit(status.code().unwrap_or(1)),
        Err(e) => e,
    }
}

// ---------------------------------------------------------------------------------------------------------------
// 2. Értesítés kapcsoló nélkül

/// A háttér-lekérdezés: a legutóbbi stabil kiadás verziója (`None`: nincs kiadás; `Err`: hiba).
type Lookup = Box<dyn FnOnce() -> Result<Option<Version>, ()> + Send>;

/// Az éles háttér-lekérdezés (rövid időkorláttal, mint a `callgraph.sh` háttér-ellenőrzése).
fn background_lookup() -> Result<Option<Version>, ()> {
    let fetcher = HttpFetcher::new(BACKGROUND_TIMEOUT);
    default_source(&Platform::current())
        .latest(&fetcher)
        .map(|release| release.map(|r| r.version))
        .map_err(|_| ())
}

/// Az „újabb verzió érhető el” értesítés egy MCP-munkamenetre.
pub(crate) struct UpdateNotice {
    file: PathBuf,
    current: Version,
    update_cmd: String,
    /// Igaz, amíg ebben a munkamenetben még szólhatunk (legfeljebb egyszer).
    pending: Mutex<bool>,
    /// Befejeződött-e (vagy el sem indult) a háttér-lekérdezés; utána már nem várunk értesítésre.
    lookup_done: Arc<AtomicBool>,
}

impl UpdateNotice {
    /// Indítás: ha esedékes (naponta egyszer, `decide`), a lekérdezés egy háttérszálon fut, az eredményt az
    /// állapotfájlba írja. Kikapcsolt ellenőrzésnél `None` (semmi nem fut, semmi nem íródik).
    fn start(
        home: &SkillHome,
        env: &CheckEnv,
        now: u64,
        current: Version,
        lookup: Lookup,
    ) -> Option<Self> {
        if !env.enabled() {
            return None;
        }
        let file = home.update_check_file();
        let state = CheckState::load(&file);
        let lookup_done = Arc::new(AtomicBool::new(true));
        // a `checked_at` azonnal beírva, hogy a párhuzamos futások ne kérdezzenek le még egyszer
        let stamped = CheckState {
            checked_at: now,
            ..state.clone()
        };
        if decide(&state, now, &current, true).spawn && stamped.save(&file).is_ok() {
            lookup_done.store(false, Ordering::Release);
            let done = Arc::clone(&lookup_done);
            let home = home.clone();
            let spawned = std::thread::Builder::new()
                .name("callgraph-update-check".into())
                .spawn(move || {
                    record_latest(&home, now_secs(), lookup());
                    done.store(true, Ordering::Release);
                });
            if spawned.is_err() {
                lookup_done.store(true, Ordering::Release);
            }
        }
        Some(Self {
            file,
            current,
            update_cmd: home.script("callgraph.sh"),
            pending: Mutex::new(true),
            lookup_done,
        })
    }

    /// Az értesítő sor, ha most kell szólni (csak fájl-olvasás, hálózat soha). Munkamenetenként legfeljebb egyszer;
    /// ha nincs mit mondani és a háttér-lekérdezés már véget ért, többé nem is néz.
    pub(crate) fn take(&self, now: u64) -> Option<String> {
        let mut pending = self.pending.lock().ok()?;
        if !*pending {
            return None;
        }
        // előbb a jelző, aztán az állapot: egy közben befejeződő lekérdezés eredménye így nem vész el
        let finished = self.lookup_done.load(Ordering::Acquire);
        let state = CheckState::load(&self.file);
        let Some(latest) = decide(&state, now, &self.current, true).notice else {
            if finished {
                *pending = false;
            }
            return None;
        };
        *pending = false;
        let next = CheckState {
            notified_at: Some(now),
            notified_version: Some(latest.to_string()),
            ..state
        };
        let _ = next.save(&self.file);
        Some(notice_line(&latest, &self.current, &self.update_cmd))
    }

    #[cfg(test)]
    fn wait_for_lookup(&self) {
        for _ in 0..500 {
            if self.lookup_done.load(Ordering::Acquire) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("the background lookup did not finish");
    }
}

/// Az MCP-értesítés: `ℹ callgraph X is available (installed: Y) — update: <skill>/callgraph.sh update, or add
/// --auto-update to the MCP config`.
fn notice_line(latest: &Version, current: &Version, update_cmd: &str) -> String {
    crate::tf!(
        "ℹ callgraph {latest} is available (installed: {current}) — update: {update_cmd} update, or add --auto-update to the MCP config",
        "ℹ callgraph {latest} elérhető (telepítve: {current}) — frissítés: {update_cmd} update, vagy add hozzá a --auto-update-et az MCP-konfighoz"
    )
}

/// A lekérdezés-végrehajtó csomagolása: az első sikeres eredmény végére kerül az értesítés (ha van).
struct NoticeRunner {
    inner: Arc<dyn QueryRunner>,
    notice: UpdateNotice,
}

impl NoticeRunner {
    fn append(&self, mut text: String) -> String {
        if let Some(line) = self.notice.take(now_secs()) {
            if !text.is_empty() && !text.ends_with('\n') {
                text.push('\n');
            }
            text.push_str(&line);
            text.push('\n');
        }
        text
    }
}

impl QueryRunner for NoticeRunner {
    fn run(&self, query: &ShowQuery) -> Result<String, String> {
        self.inner.run(query).map(|text| self.append(text))
    }

    fn projects(&self) -> Result<String, String> {
        self.inner.projects().map(|text| self.append(text))
    }

    fn profile(&self, project: Option<&str>, full: bool) -> Result<String, String> {
        self.inner
            .profile(project, full)
            .map(|text| self.append(text))
    }

    fn startup_note(&self) -> String {
        self.inner.startup_note()
    }

    fn next(&self, project: Option<&str>, id: &str, choices: &[usize]) -> Result<String, String> {
        self.inner
            .next(project, id, choices)
            .map(|text| self.append(text))
    }
}

/// A szerver végrehajtója az értesítéssel (kapcsoló nélküli mód); kikapcsolt ellenőrzésnél változatlan.
pub(crate) fn with_notice(runner: Arc<dyn QueryRunner>, home: &SkillHome) -> Arc<dyn QueryRunner> {
    with_notice_from(
        runner,
        home,
        &CheckEnv::from_process(),
        Version::current(),
        Box::new(background_lookup),
    )
}

fn with_notice_from(
    runner: Arc<dyn QueryRunner>,
    home: &SkillHome,
    env: &CheckEnv,
    current: Version,
    lookup: Lookup,
) -> Arc<dyn QueryRunner> {
    match UpdateNotice::start(home, env, now_secs(), current, lookup) {
        Some(notice) => Arc::new(NoticeRunner {
            inner: runner,
            notice,
        }),
        None => runner,
    }
}

#[cfg(test)]
mod tests;
