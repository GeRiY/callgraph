//! A helyi napi összesítő: `<skill>/stats/YYYY-MM.json` (nap → parancs → számlálók és sávok). Kicsi JSON,
//! olvasás-módosítás-írás atomikus átnevezéssel (ideiglenes fájl + `rename`); minden hibát elnyel, a parancsot soha
//! nem lassítja érdemben és nem rontja el. Két párhuzamos folyamat elveszíthet egy-egy növelést (az utolsó írás
//! nyer) — ez elfogadott; folyamaton belül (MCP-szerver, párhuzamos eszköz-hívások) egy zár sorba rendezi az írást.
//! Legfeljebb ~90 nap marad meg: egy új hónap első írásakor a régebbi havi fájlok törlődnek.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use super::buckets::{LatencyBucket, RankBucket, SizeBucket};
use super::day::Day;
use super::event::{Client, CommandKind, ErrorKind, Event, Outcome};

/// Ennyi napnál régebbi adat nem marad meg.
pub(crate) const RETENTION_DAYS: u32 = 90;

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
}

fn is_zero(n: &u64) -> bool {
    *n == 0
}

fn all_zero(values: &[u64; RankBucket::COUNT]) -> bool {
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
    }

    /// A hibák száma.
    pub(crate) fn error_count(&self) -> u64 {
        self.errors.values().sum()
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
}

impl Default for MonthFile {
    fn default() -> Self {
        Self {
            v: FORMAT,
            days: BTreeMap::new(),
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

    /// Egy hívás rögzítése az adott napra; minden hibát elnyel.
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

    /// Olvasás → módosítás → atomikus írás; egy új hónap első írásakor a lejárt hónapok törlése.
    fn update(&self, day: Day, change: impl FnOnce(&mut MonthFile)) {
        let _guard = WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let path = self.month_file(day);
        let new_month = !path.is_file();
        if new_month && std::fs::create_dir_all(&self.dir).is_err() {
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
fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(format!(".tmp-{}", std::process::id()));
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}
