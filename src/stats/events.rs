//! A helyi eseménynapló: `<stats>/events/YYYY-MM-DD.jsonl` (UTC-nap), mért hívásonként egy JSON-sor. CSAK helyi, a
//! beküldés soha nem olvassa. Egy sor csak rögzített neveket, sávokat, időt és sorszámot tartalmaz: út, fájl-,
//! szimbólum- vagy projektnév, keresési kifejezés, hibaüzenet és ezek hash-e nincs benne (a `rel` / `rep` jelzők a
//! munkamenet-állapotból jönnek, `session.rs`; a `use_file` / `use_sym` csak a sáv, hogy a cél benne volt-e az előző
//! kimenetben, `reuse.rs`). 14 napig marad: egy új nap első sorának írásakor a régebbi
//! napi fájlok törlődnek.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::day::Day;
use super::event::{Client, CommandKind, ErrorKind};
use super::session::Rel;

/// Az eseménynapló mappája a stats-mappán belül.
pub(crate) const EVENTS_DIR: &str = "events";
/// Ennyi napig marad meg az eseménynapló (a mai nappal együtt).
pub(crate) const EVENT_DAYS: u32 = 14;

/// Egy eseménysor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct EventLine {
    /// A kezdés ideje (RFC 3339, UTC, ezredmásodperccel).
    pub(crate) t0: String,
    /// A futási idő (ms).
    pub(crate) ms: u64,
    pub(crate) cmd: CommandKind,
    pub(crate) via: Client,
    pub(crate) ok: bool,
    pub(crate) err: Option<ErrorKind>,
    /// A célfájl nyelve (rögzített lista).
    pub(crate) lang: Option<String>,
    /// A célfájl méretének sávja.
    #[serde(rename = "in")]
    pub(crate) input: Option<String>,
    /// A kimenet méretének sávja (token).
    pub(crate) out: String,
    pub(crate) rel: Rel,
    /// Az érintés-szám sávja (1, 2-3, 4+).
    pub(crate) rep: Option<String>,
    pub(crate) sess: u64,
    pub(crate) seq: u64,
    /// A célfájl az előző hívás kimenetében: `hit1` / `hit5` / `hit` / `miss` / `-` (`reuse.rs`).
    #[serde(default = "dash")]
    pub(crate) use_file: String,
    /// A kért szimbólum az előző hívás kimenetében: `hit` / `miss` / `-`.
    #[serde(default = "dash")]
    pub(crate) use_sym: String,
}

/// A hiányzó sáv (a régebbi sorokban nincs `use_file` / `use_sym`).
fn dash() -> String {
    "-".to_owned()
}

/// Unix-ms → `YYYY-MM-DDTHH:MM:SS.mmmZ`.
pub(crate) fn rfc3339_ms(ms: u64) -> String {
    let secs = ms / 1000;
    let (y, m, d) = Day::from_secs(secs).ymd();
    let sod = secs % 86_400;
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{:03}Z",
        sod / 3600,
        sod / 60 % 60,
        sod % 60,
        ms % 1000
    )
}

/// Unix-ms most.
pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

fn day_file(dir: &Path, day: Day) -> PathBuf {
    dir.join(EVENTS_DIR).join(format!("{day}.jsonl"))
}

/// Egy sor hozzáfűzése a nap fájljához; egy új nap első sora előtt a lejárt napok törlése. Minden hibát elnyel.
pub(crate) fn append(dir: &Path, day: Day, line: &EventLine) {
    let events = dir.join(EVENTS_DIR);
    if std::fs::create_dir_all(&events).is_err() {
        return;
    }
    let path = day_file(dir, day);
    let new_day = !path.is_file();
    let Ok(mut text) = serde_json::to_string(line) else {
        return;
    };
    text.push('\n');
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = file.write_all(text.as_bytes());
    }
    if new_day {
        prune(&events, day);
    }
}

/// A `today`-hez képest `EVENT_DAYS` napnál régebbi napi fájlok törlése (más nevű fájlhoz nem nyúl).
pub(crate) fn prune(events: &Path, today: Day) {
    let Ok(entries) = std::fs::read_dir(events) else {
        return;
    };
    let names: Vec<String> = entries
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .collect();
    for name in expired_days(&names, today) {
        let _ = std::fs::remove_file(events.join(name));
    }
}

/// A lejárt napi fájlok nevei: `YYYY-MM-DD.jsonl`, amely legalább `EVENT_DAYS` nappal régebbi a mainál.
pub(crate) fn expired_days(names: &[String], today: Day) -> Vec<String> {
    let cutoff = today.minus(EVENT_DAYS);
    names
        .iter()
        .filter(|name| {
            name.strip_suffix(".jsonl")
                .and_then(Day::parse)
                .is_some_and(|day| day <= cutoff)
        })
        .cloned()
        .collect()
}

/// A `from..=to` napok eseményei időrendben (a sérült sorok kimaradnak).
pub(crate) fn read(dir: &Path, from: Day, to: Day) -> Vec<EventLine> {
    let mut lines = Vec::new();
    let mut day = from;
    while day <= to {
        if let Ok(text) = std::fs::read_to_string(day_file(dir, day)) {
            lines.extend(
                text.lines()
                    .filter_map(|line| serde_json::from_str::<EventLine>(line).ok()),
            );
        }
        day = day.plus(1);
    }
    lines
}
