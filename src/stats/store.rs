//! A helyi napi összesítő: `<skill>/stats/YYYY-MM.json` (nap → parancs → számlálók és sávok). Kicsi JSON,
//! olvasás-módosítás-írás atomikus átnevezéssel (ideiglenes fájl + `rename`); minden hibát elnyel, a parancsot soha
//! nem lassítja érdemben és nem rontja el. Két párhuzamos folyamat elveszíthet egy-egy növelést (az utolsó írás
//! nyer) — ez elfogadott; folyamaton belül (MCP-szerver, párhuzamos eszköz-hívások) egy zár sorba rendezi az írást.
//! Legfeljebb ~90 nap marad meg: egy új hónap első írásakor a régebbi havi fájlok törlődnek.
//!
//! Naponként a parancsok mellett az átmenet-élek (`edges`: melyik parancs után melyik jött, a viszony és a szünet
//! sávja; `session.rs`) és a lezárt munkamenetek sávjai (`sessions`) is itt vannak — ezek is csak rögzített nevek
//! és sávok. Egy mért hívás (`record_call`) a munkamenet-állapotot (`session.json`) és a helyi eseménynaplót
//! (`events/`) is lépteti, ugyanazon zár alatt.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use super::buckets::{InputBucket, LatencyBucket, RankBucket, SizeBucket};
use super::day::Day;
use super::event::{Client, CommandKind, ErrorKind, Event, Outcome};
use super::events::{EventLine, append as append_event, rfc3339_ms};
use super::session::{SESSION_FILE, SessionState, Sessions, Visit, add_edge, rep_band};
use super::target::Call;

/// Ennyi napnál régebbi adat nem marad meg.
pub(crate) const RETENTION_DAYS: u32 = 90;

/// Az egyszeri „helyi számlálás” tájékoztatás jelzőfájlja a stats-mappában.
pub(crate) const NOTICE_MARKER: &str = "notice-shown";

/// A havi fájl formátum-verziója.
const FORMAT: u32 = 1;

/// A folyamaton belüli írások sorba rendezése.
static WRITE_LOCK: Mutex<()> = Mutex::new(());

/// Egy parancs számlálói egy napra (vagy összegezve egy időszakra). A tömbök a sávok sorrendjében
/// (`LatencyBucket::LABELS`, `SizeBucket::LABELS`; a `hits`: 1., 2–5., 6+., nem volt a listán).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Counters {
    pub(crate) calls: u64,
    pub(crate) clients: BTreeMap<Client, u64>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) errors: BTreeMap<ErrorKind, u64>,
    pub(crate) latency: [u64; LatencyBucket::COUNT],
    pub(crate) size: [u64; SizeBucket::COUNT],
    #[serde(skip_serializing_if = "is_zero")]
    pub(crate) truncated: u64,
    /// Csak a `locate`-nél: a következő megnyitott fájl helye a rangsorban.
    #[serde(skip_serializing_if = "all_zero")]
    pub(crate) hits: [u64; RankBucket::COUNT],
    /// Hányszor kapott javaslat-sort (`next`) ennek a parancsnak az eredménye.
    #[serde(skip_serializing_if = "is_zero")]
    pub(crate) next_shown: u64,
    /// Hányszor futtatták egy javaslatát (`next rN k`).
    #[serde(skip_serializing_if = "is_zero")]
    pub(crate) next_used: u64,
    /// A célfájl méretének sávjai (`InputBucket::LABELS`; fájl-cél nélküli hívás nem számít).
    #[serde(skip_serializing_if = "all_zero")]
    pub(crate) in_size: [u64; InputBucket::COUNT],
    /// A célfájl nyelve (rögzített lista, `target::language`) → hívások.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) lang: BTreeMap<String, u64>,
}

fn is_zero(n: &u64) -> bool {
    *n == 0
}

fn all_zero<const N: usize>(values: &[u64; N]) -> bool {
    values.iter().all(|n| *n == 0)
}

impl Counters {
    /// Egy hívás hozzáadása.
    pub(crate) fn add_event(&mut self, event: &Event) {
        self.calls += 1;
        *self.clients.entry(event.client).or_default() += 1;
        if let Outcome::Error(kind) = event.outcome {
            *self.errors.entry(kind).or_default() += 1;
        }
        self.latency[event.latency.index()] += 1;
        self.size[event.size.index()] += 1;
        if event.truncated {
            self.truncated += 1;
        }
    }

    /// A hívás céljának nyelve és mérete (csak fájl-célnál).
    pub(crate) fn add_target(&mut self, lang: Option<&str>, input: Option<InputBucket>) {
        if let Some(lang) = lang {
            *self.lang.entry(lang.to_owned()).or_default() += 1;
        }
        if let Some(input) = input {
            self.in_size[input.index()] += 1;
        }
    }

    /// Egy másik számláló hozzáadása (napok összegzése).
    pub(crate) fn merge(&mut self, other: &Self) {
        self.calls += other.calls;
        for (client, n) in &other.clients {
            *self.clients.entry(*client).or_default() += n;
        }
        for (kind, n) in &other.errors {
            *self.errors.entry(*kind).or_default() += n;
        }
        add_arrays(&mut self.latency, &other.latency);
        add_arrays(&mut self.size, &other.size);
        self.truncated += other.truncated;
        add_arrays(&mut self.hits, &other.hits);
        self.next_shown += other.next_shown;
        self.next_used += other.next_used;
        add_arrays(&mut self.in_size, &other.in_size);
        for (lang, n) in &other.lang {
            *self.lang.entry(lang.clone()).or_default() += n;
        }
    }

    /// A hibák száma.
    pub(crate) fn error_count(&self) -> u64 {
        self.errors.values().sum()
    }

    /// A felhasználói hibák (használat, nincs meg, nem támogatott): számolva, de az `errors.jsonl`-be nem kerülnek.
    pub(crate) fn user_error_count(&self) -> u64 {
        self.errors
            .iter()
            .filter(|(kind, _)| kind.is_user_error())
            .map(|(_, n)| n)
            .sum()
    }

    /// A belső hibák (I/O, egyéb): ezek az `errors.jsonl`-ben is ott vannak.
    pub(crate) fn internal_error_count(&self) -> u64 {
        self.error_count() - self.user_error_count()
    }
}

fn add_arrays<const N: usize>(into: &mut [u64; N], from: &[u64; N]) {
    for (a, b) in into.iter_mut().zip(from) {
        *a += b;
    }
}

/// Egy havi fájl: nap → parancs → számlálók.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct MonthFile {
    pub(crate) v: u32,
    pub(crate) days: BTreeMap<Day, BTreeMap<CommandKind, Counters>>,
    /// Nap → átmenet-él kulcsa (`{előző}[!]>{ez}|{rel}|{szünet}`, `session.rs`) → darab; legfeljebb 500 kulcs
    /// naponként, a többi `other`. A régebbi (v0.9.4-es) fájlokban nincs: üres.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) edges: BTreeMap<Day, BTreeMap<String, u64>>,
    /// Nap → a lezárt munkamenetek sávjai (az utolsó hívásuk napjára számítva).
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) sessions: BTreeMap<Day, Sessions>,
}

impl Default for MonthFile {
    fn default() -> Self {
        Self {
            v: FORMAT,
            days: BTreeMap::new(),
            edges: BTreeMap::new(),
            sessions: BTreeMap::new(),
        }
    }
}

impl MonthFile {
    /// Betöltés; hiányzó vagy sérült fájlnál üres.
    pub(crate) fn load(path: &Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    /// A szöveg-alak (tömör JSON, sorvégi újsorral).
    pub(crate) fn to_json(&self) -> String {
        let mut text = serde_json::to_string(self).unwrap_or_default();
        text.push('\n');
        text
    }

    /// Egy nap egy parancsának számlálói (létrehozza, ha nincs).
    pub(crate) fn counters(&mut self, day: Day, command: CommandKind) -> &mut Counters {
        self.days
            .entry(day)
            .or_default()
            .entry(command)
            .or_default()
    }
}

/// Összesítés egy időszakra, parancsonként.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Totals {
    pub(crate) commands: BTreeMap<CommandKind, Counters>,
}

impl Totals {
    /// Az összes hívás.
    pub(crate) fn calls(&self) -> u64 {
        self.commands.values().map(|c| c.calls).sum()
    }

    /// A hívások egy klienstől.
    pub(crate) fn client_calls(&self, client: Client) -> u64 {
        self.commands
            .values()
            .filter_map(|c| c.clients.get(&client))
            .sum()
    }
}

/// A napi összesítők mappája (`<skill>/stats`, vagy a `CALLGRAPH_STATS_DIR`).
#[derive(Debug, Clone)]
pub(crate) struct StatsStore {
    dir: PathBuf,
}

impl StatsStore {
    pub(crate) fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    pub(crate) fn dir(&self) -> &Path {
        &self.dir
    }

    fn month_file(&self, day: Day) -> PathBuf {
        self.dir.join(format!("{}.json", day.month_key()))
    }

    /// Csak a napi számlálók egy hívásra az adott napra (munkamenet és eseménynapló nélkül; a tesztek
    /// adat-feltöltése). Minden hibát elnyel.
    #[cfg(test)]
    pub(crate) fn record(&self, day: Day, event: &Event) {
        self.update(day, |file| {
            file.counters(day, event.command).add_event(event);
        });
    }

    /// Egy `locate`-rangsor-találat rögzítése az adott napra; minden hibát elnyel.
    pub(crate) fn record_hit(&self, day: Day, hit: RankBucket) {
        self.update(day, |file| {
            file.counters(day, CommandKind::Locate).hits[hit.index()] += 1;
        });
    }

    /// Az egyszeri tájékoztatás jelzőfájlja: `true`, ha most jött létre (akkor kell kiírni a tájékoztatást),
    /// `false`, ha már volt, vagy nem hozható létre (ilyenkor sem írunk ki semmit, hogy ne ismétlődjön).
    pub(crate) fn claim_notice(&self) -> bool {
        if std::fs::create_dir_all(&self.dir).is_err() {
            return false;
        }
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(self.dir.join(NOTICE_MARKER))
            .is_ok()
    }

    /// Egy javaslat-sor (`used: false`) vagy egy elfogadott javaslat (`used: true`) rögzítése.
    pub(crate) fn record_next(&self, day: Day, command: CommandKind, used: bool) {
        self.update(day, |file| {
            let counters = file.counters(day, command);
            if used {
                counters.next_used += 1;
            } else {
                counters.next_shown += 1;
            }
        });
    }

    /// Egy mért hívás: a napi számlálók (a cél nyelvével és méretével), a munkamenet léptetése (`session.json`), az
    /// átmenet-él, egy lezárult előző munkamenet sávjai és vég-éle, és egy sor a helyi eseménynaplóba. Minden
    /// hibát elnyel.
    pub(crate) fn record_call(&self, call: &Call) {
        let _guard = WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        if std::fs::create_dir_all(&self.dir).is_err() {
            return;
        }
        let day = Day::from_secs(call.end_ms / 1000);
        let start_ms = call.end_ms.saturating_sub(call.ms);
        let session_path = self.dir.join(SESSION_FILE);
        let mut state = SessionState::load(&session_path);
        let key = state.key_bytes();
        let keys = call.target.as_ref().map(|t| t.keys(&key));
        let error = match call.event.outcome {
            Outcome::Ok => None,
            Outcome::Error(kind) => Some(kind),
        };
        let step = state.advance(Visit {
            start_ms,
            end_ms: call.end_ms,
            day,
            command: call.event.command,
            failed: error.is_some(),
            keys,
        });
        let _ = write_atomic(&session_path, &state.to_json());
        let target = call.target.as_ref();
        append_event(
            &self.dir,
            day,
            &EventLine {
                t0: rfc3339_ms(start_ms),
                ms: call.ms,
                cmd: call.event.command,
                via: call.event.client,
                ok: error.is_none(),
                err: error,
                lang: target.and_then(|t| t.lang).map(str::to_owned),
                input: target.and_then(|t| t.input).map(|b| b.label().to_owned()),
                out: call.event.size.label().to_owned(),
                rel: step.rel,
                rep: step.rep.map(|n| rep_band(n).to_owned()),
                sess: step.sess,
                seq: step.seq,
            },
        );
        // a lezárult munkamenet az utolsó hívásának napjára számít (csak ha annak a hónapnak még van fájlja)
        if let Some(closed) = &step.closed {
            let same_month = closed.day.month_start() == day.month_start();
            self.update_locked(closed.day, same_month, |file| {
                add_edge(
                    file.edges.entry(closed.day).or_default(),
                    &closed.end_edge(),
                );
                file.sessions.entry(closed.day).or_default().add(closed);
            });
        }
        self.update_locked(day, true, |file| {
            let counters = file.counters(day, call.event.command);
            counters.add_event(&call.event);
            if let Some(target) = target {
                counters.add_target(target.lang, target.input);
            }
            add_edge(file.edges.entry(day).or_default(), &step.edge);
        });
    }

    /// Olvasás → módosítás → atomikus írás; egy új hónap első írásakor a lejárt hónapok törlése.
    fn update(&self, day: Day, change: impl FnOnce(&mut MonthFile)) {
        let _guard = WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        self.update_locked(day, true, change);
    }

    /// Mint az `update`, a zár már a hívónál; `create: false` = nem létező havi fájlt nem hoz létre.
    fn update_locked(&self, day: Day, create: bool, change: impl FnOnce(&mut MonthFile)) {
        let path = self.month_file(day);
        let new_month = !path.is_file();
        if new_month && (!create || std::fs::create_dir_all(&self.dir).is_err()) {
            return;
        }
        let mut file = MonthFile::load(&path);
        change(&mut file);
        let _ = write_atomic(&path, &file.to_json());
        if new_month {
            self.prune(day);
        }
    }

    /// A `today`-hez képest `RETENTION_DAYS`-nél régebbi hónapok fájljainak törlése.
    pub(crate) fn prune(&self, today: Day) {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return;
        };
        let names: Vec<String> = entries
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .collect();
        for name in expired_months(&names, today) {
            let _ = std::fs::remove_file(self.dir.join(name));
        }
    }

    /// Egy nap parancsonkénti számlálói (üres, ha nincs adat).
    pub(crate) fn day_counters(&self, day: Day) -> BTreeMap<CommandKind, Counters> {
        MonthFile::load(&self.month_file(day))
            .days
            .remove(&day)
            .unwrap_or_default()
    }

    /// Egy nap átmenet-élei és lezárt munkamenetei (üres / `None`, ha nincs adat).
    pub(crate) fn day_flow(&self, day: Day) -> (BTreeMap<String, u64>, Option<Sessions>) {
        let mut file = MonthFile::load(&self.month_file(day));
        let edges = file.edges.remove(&day).unwrap_or_default();
        let sessions = file.sessions.remove(&day).filter(|s| !s.is_empty());
        (edges, sessions)
    }

    /// Az átmenet-élek összege a `from..=to` napokra.
    pub(crate) fn edge_totals(&self, from: Day, to: Day) -> BTreeMap<String, u64> {
        let mut totals = BTreeMap::new();
        let mut month = from.month_start();
        while month <= to {
            let file = MonthFile::load(&self.month_file(month));
            for edges in file.edges.range(from..=to).map(|(_, edges)| edges) {
                for (key, n) in edges {
                    *totals.entry(key.clone()).or_default() += n;
                }
            }
            month = month.next_month_start();
        }
        totals
    }

    /// Az összesítés a `from..=to` napokra.
    pub(crate) fn totals(&self, from: Day, to: Day) -> Totals {
        let mut totals = Totals::default();
        let mut month = from.month_start();
        while month <= to {
            let file = MonthFile::load(&self.month_file(month));
            for commands in file.days.range(from..=to).map(|(_, commands)| commands) {
                for (command, counters) in commands {
                    totals.commands.entry(*command).or_default().merge(counters);
                }
            }
            month = month.next_month_start();
        }
        totals
    }
}

/// A lejárt havi fájlok nevei: `YYYY-MM.json`, amelynek az utolsó napja is régebbi `RETENTION_DAYS` napnál.
/// Más nevű fájlhoz nem nyúl.
pub(crate) fn expired_months(names: &[String], today: Day) -> Vec<String> {
    let cutoff = today.minus(RETENTION_DAYS);
    names
        .iter()
        .filter(|name| {
            name.strip_suffix(".json")
                .and_then(Day::parse_month)
                .is_some_and(|month| month.next_month_start() <= cutoff)
        })
        .cloned()
        .collect()
}

/// Írás ideiglenes fájlba, majd átnevezés (a párhuzamos olvasó nem lát félkész fájlt).
pub(crate) fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(format!(".tmp-{}", std::process::id()));
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}
