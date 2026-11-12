//! A helyi statisztika opt-in beküldése (csak kliensoldal). Kemény szabály: SEMMI nem megy el, amíg a felhasználó
//! kifejezetten be nem kapcsolta (`telemetry on`: interaktív terminálban y/N megerősítés; agent nem kapcsolhatja be).
//!
//! - Állapot: `<skill>/stats/telemetry.json` (`consent.rs`); alapból nem döntött = KI.
//! - Kérdezés: csak interaktív terminálban (stdin ÉS stderr terminál), telepítéskor / az első frissítéskor, ha még
//!   nem döntött (Igen / Most nem / Soha); „Most nem” után egyszer még, ~14 nap múlva, a `stats` kimenetében (tipp,
//!   nem kérdés). „Soha” végleges.
//! - Szólás az agentnek (`agent_hint`): ritkán (14 naponként legfeljebb egyszer, összesen legfeljebb 3-szor, legalább
//!   50 rögzített hívás után), csak nem döntött / „most nem” állapotban; a felhasználó dönt.
//! - Beküldés (`send.rs`): a normál futásokra ráülve (CLI, MCP indulás + óránként), ha be van kapcsolva, az utolsó
//!   sikeres beküldés > 24 órája volt és van lezárt, be nem küldött nap → leválasztott háttérfolyamat
//!   (`callgraph telemetry send`), 5 s időkorlát, zárfájl; a napok csak HTTP 2xx után számítanak beküldöttnek.
//! - Tartalom (`payload.rs`, 2-es séma): a lezárt napok számlálói, és mellettük a parancs-sorrend élei és a
//!   munkamenet-sávok — út, név, szöveg, hash nélkül. A már bekapcsolt beküldésnél erről egyszer szól a CLI
//!   (`order_notice`). A helyi eseménynapló és a munkamenet-állapot soha nem megy el.
//! - Soha: CI-ban, `CALLGRAPH_STATS=0` mellett, `CALLGRAPH_TELEMETRY=0` vagy `DO_NOT_TRACK=1` (kemény tiltás)
//!   mellett, kikapcsolt állapotban.

mod consent;
mod payload;
mod prompt;
mod send;
#[cfg(test)]
mod tests;

use std::path::PathBuf;

pub(crate) use consent::{Choice, Consent};
pub(crate) use prompt::decided_line;
pub(crate) use send::{HttpSender, Sender};

use super::day::Day;
use super::policy::StatsEnv;
use super::store::StatsStore;
use crate::skill::SkillHome;

/// A kemény tiltás (`0` / `off` / `false` / `no`).
pub(crate) const KILL_ENV: &str = "CALLGRAPH_TELEMETRY";
/// A végpont felülírása.
pub(crate) const URL_ENV: &str = "CALLGRAPH_TELEMETRY_URL";
/// Az alapértelmezett végpont (POST JSON).
pub(crate) const DEFAULT_URL: &str = "https://telemetry.callgraph.eu/v1";
/// Az állapotfájl neve a stats-mappában.
pub(crate) const CONSENT_FILE: &str = "telemetry.json";
/// Az egyszeri „mostantól a parancs-sorrend is megy” tájékoztatás (2-es séma) jelzőfájlja a stats-mappában.
pub(crate) const ORDER_NOTICE_MARKER: &str = "telemetry-order-notice-shown";

const DAY_SECS: u64 = 24 * 60 * 60;
/// Két beküldés között legalább ennyi.
pub(crate) const SEND_INTERVAL: u64 = DAY_SECS;
/// Két kísérlet között (sikertelen szervernél) legalább ennyi.
pub(crate) const ATTEMPT_INTERVAL: u64 = 60 * 60;
/// A „most nem” utáni emlékeztető és két agent-tipp között legalább ennyi.
pub(crate) const REMIND_AFTER: u64 = 14 * DAY_SECS;
/// Legfeljebb ennyi agent-tipp összesen.
pub(crate) const MAX_HINTS: u32 = 3;
/// Agent-tipp csak legalább ennyi rögzített hívás után.
pub(crate) const MIN_CALLS: u64 = 50;
/// Legfeljebb ennyi kérdés összesen (az első + a „most nem” utáni egy).
pub(crate) const MAX_ASKS: u32 = 2;

/// A beküldést befolyásoló környezet.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct TelemetryEnv {
    pub stats: StatsEnv,
    /// `CALLGRAPH_TELEMETRY=0`.
    pub killed: bool,
    pub url: String,
}

impl TelemetryEnv {
    pub(crate) fn from_process() -> Self {
        Self {
            stats: StatsEnv::from_process(),
            // a DO_NOT_TRACK=1 (bármely nem üres, nem `0` érték) ugyanúgy mindent leállít
            killed: std::env::var(KILL_ENV).is_ok_and(|v| {
                matches!(
                    v.trim().to_ascii_lowercase().as_str(),
                    "0" | "off" | "false" | "no"
                )
            }) || std::env::var("DO_NOT_TRACK")
                .is_ok_and(|v| !v.trim().is_empty() && v.trim() != "0"),
            url: std::env::var(URL_ENV)
                .ok()
                .filter(|u| !u.trim().is_empty())
                .unwrap_or_else(|| DEFAULT_URL.to_owned()),
        }
    }

    /// Szabad-e egyáltalán a telemetriával foglalkozni (kérdezni, szólni, küldeni).
    pub(crate) fn allowed(&self) -> bool {
        self.stats.enabled() && !self.stats.ci && !self.killed
    }
}

/// Interaktív-e a futás (a kérdéshez stdin ÉS stderr terminál kell).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Tty {
    pub stdin: bool,
    pub stderr: bool,
}

impl Tty {
    pub(crate) fn from_process() -> Self {
        use std::io::IsTerminal;
        Self {
            stdin: std::io::stdin().is_terminal(),
            stderr: std::io::stderr().is_terminal(),
        }
    }

    pub(crate) fn interactive(self) -> bool {
        self.stdin && self.stderr
    }
}

/// Egy beküldési kísérlet eredménye.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SendOutcome {
    /// Nem kellett / nem szabad küldeni.
    Skipped,
    /// Elküldve (a napok száma).
    Sent(usize),
    /// Nem sikerült (a napok beküldetlenek maradnak).
    Failed(String),
}

/// A telemetria homlokzata: a környezet és a stats-mappa.
#[derive(Debug, Clone)]
pub(crate) struct Telemetry {
    env: TelemetryEnv,
    dir: PathBuf,
}

impl Telemetry {
    pub(crate) fn from_env(home: &SkillHome) -> Self {
        let env = TelemetryEnv::from_process();
        let dir = env.stats.data_dir(home);
        Self { env, dir }
    }

    #[cfg(test)]
    pub(crate) fn new(env: TelemetryEnv, dir: PathBuf) -> Self {
        Self { env, dir }
    }

    pub(crate) fn env(&self) -> &TelemetryEnv {
        &self.env
    }

    fn consent_path(&self) -> PathBuf {
        self.dir.join(CONSENT_FILE)
    }

    fn store(&self) -> StatsStore {
        StatsStore::new(self.dir.clone())
    }

    pub(crate) fn consent(&self) -> Consent {
        Consent::load(&self.consent_path())
    }

    pub(crate) fn save(&self, consent: &Consent) -> std::io::Result<()> {
        consent.save(&self.consent_path())
    }

    /// A beküldésre váró lezárt napok.
    pub(crate) fn pending(&self, consent: &Consent, today: Day) -> Vec<Day> {
        payload::pending_days(&self.store(), consent.last_sent_day.as_deref(), today)
    }

    /// Esedékes-e egy beküldés.
    pub(crate) fn due(&self, consent: &Consent, now: u64) -> bool {
        self.env.allowed()
            && consent.enabled
            && consent.choice == Some(Choice::Yes)
            && consent
                .last_send
                .is_none_or(|t| now.saturating_sub(t) >= SEND_INTERVAL)
            && consent
                .last_attempt
                .is_none_or(|t| now.saturating_sub(t) >= ATTEMPT_INTERVAL)
            && !self.pending(consent, Day::from_secs(now)).is_empty()
    }

    /// A normál futásokra ráülve: esedékes beküldésnél leválasztott háttérfolyamat (`callgraph telemetry send`).
    pub(crate) fn maybe_spawn(&self, now: u64) {
        if self.env.allowed() && self.due(&self.consent(), now) {
            crate::update::check::spawn_detached(["telemetry", "send"]);
        }
    }

    /// A beküldés (a háttérfolyamatban; a tesztek a `sender`-t cserélik).
    pub(crate) fn send_now(&self, sender: &dyn Sender, now: u64) -> SendOutcome {
        send::send_now(self, sender, now)
    }

    /// A pontos JSON, ami most elmenne (a beküldésre váró lezárt napok).
    pub(crate) fn preview(&self, now: u64) -> String {
        let consent = self.consent();
        let days = self.pending(&consent, Day::from_secs(now));
        let payload = payload::build(&self.store(), &days);
        serde_json::to_string_pretty(&payload).unwrap_or_default()
    }

    /// Az állapot szövege (`telemetry status`).
    pub(crate) fn status(&self, now: u64) -> String {
        let consent = self.consent();
        let state = match (consent.enabled, consent.choice) {
            (true, _) => crate::t!("on", "be"),
            (false, None) => crate::t!("off (not decided yet)", "ki (még nincs döntés)"),
            (false, Some(Choice::Later)) => crate::t!("off (not now)", "ki (most nem)"),
            (false, Some(Choice::Never)) => {
                crate::t!("off (never ask again)", "ki (soha ne kérdezzen)")
            }
            (false, _) => crate::t!("off", "ki"),
        };
        let last = consent.last_sent_day.clone().unwrap_or_else(|| "-".into());
        let pending = self.pending(&consent, Day::from_secs(now)).len();
        let mut text = crate::tf!(
            "telemetry: {state} · last sent day: {last} · closed days waiting: {pending} · endpoint: {}\n",
            "telemetry: {state} · utolsó beküldött nap: {last} · várakozó lezárt nap: {pending} · végpont: {}\n",
            self.env.url
        );
        if !self.env.allowed() {
            text.push_str(crate::t!(
                "Sending is blocked here (CALLGRAPH_TELEMETRY=0, CALLGRAPH_STATS=0, CI, or not a skill install).\n",
                "A beküldés itt tiltva (CALLGRAPH_TELEMETRY=0, CALLGRAPH_STATS=0, CI, vagy nem skill-telepítés).\n"
            ));
        }
        text
    }

    /// Döntés rögzítése (`telemetry on` a megerősítés után, `telemetry off`, a kérdés válasza).
    pub(crate) fn decide(&self, choice: Choice, now: u64) -> std::io::Result<()> {
        let mut consent = self.consent();
        consent.decide(choice, now);
        self.save(&consent)
    }

    /// Az egyszeri tájékoztatás (CLI stderr) azoknak, akiknél a beküldés MÁR be van kapcsolva: mostantól a
    /// parancs-sorrend napi számai is mennek (2-es séma). A jelzőfájl az első vizsgálatkor létrejön akkor is, ha a
    /// beküldés nincs bekapcsolva (aki később kapcsolja be, már az új leírást látta), így legfeljebb egyszer jelenik
    /// meg. Tiltott környezetben semmit nem csinál.
    pub(crate) fn order_notice(&self) -> Option<String> {
        if !self.env.allowed() {
            return None;
        }
        let marker = self.dir.join(ORDER_NOTICE_MARKER);
        if marker.exists() || std::fs::create_dir_all(&self.dir).is_err() {
            return None;
        }
        let claimed = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&marker)
            .is_ok();
        (claimed && self.consent().enabled).then(order_notice_text)
    }

    /// Vissza a „még nem döntött” állapotba: a beküldés ki, a döntés és a kérdezés-számlálók törölve (bekapcsolni
    /// továbbra is csak a `telemetry on` tud, terminálban).
    pub(crate) fn reset(&self) -> std::io::Result<()> {
        let mut consent = self.consent();
        consent.enabled = false;
        consent.choice = None;
        consent.decided_at = None;
        consent.asked_count = 0;
        consent.hint_count = 0;
        consent.hint_at = None;
        self.save(&consent)
    }
}

/// Az egyszeri tájékoztatás szövege (`Telemetry::order_notice`).
pub(crate) fn order_notice_text() -> String {
    crate::t!(
        "ℹ callgraph telemetry now also sends daily command-order counts (which command followed which, timing bands — no names or paths). Preview: callgraph.sh telemetry preview · off: callgraph.sh telemetry off",
        "ℹ a callgraph telemetria mostantól a parancsok sorrendjének napi számait is küldi (melyik parancs után melyik jött, időzítési sávok — név és útvonal nélkül). Előnézet: callgraph.sh telemetry preview · kikapcsolás: callgraph.sh telemetry off"
    )
    .to_owned()
}

/// Unix-másodperc most.
pub(crate) fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}
