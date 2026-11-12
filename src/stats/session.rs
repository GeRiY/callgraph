//! A helyi munkamenet-állapot: `<stats>/session.json` (SOHA nem megy el). Csak ehhez kell: melyik munkamenet hányadik
//! hívása ez (`sess` / `seq`), mi a viszonya az előző híváshoz (`rel`: ugyanaz a fájl / mappa / projekt, más
//! projekt), hányadszor nyúl ugyanahhoz a fájlhoz (`rep`), és a napi összesítő átmenet-élei és munkamenet-sávjai.
//!
//! Új munkamenet indul, ha az előző hívás vége és ennek a kezdete között több mint 30 perc telt el. A fájl egy
//! telepítésenkénti véletlen kulcsot (hexa, egyszer jön létre) és a cél kulcsolt hash-eit tartja
//! (`HMAC-SHA256(kulcs, út)`, `target.rs`) — út, név vagy kulcs nélküli hash nincs benne. Két párhuzamos folyamat
//! közül az utolsó írás nyer (mint a napi fájlnál); folyamaton belül a `store.rs` zárja sorba rendezi.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::day::Day;
use super::event::CommandKind;
use super::hmac::{from_hex, random_key, to_hex};
use super::target::Keys;

/// Az állapotfájl neve a stats-mappában.
pub(crate) const SESSION_FILE: &str = "session.json";
/// Ennél hosszabb szünet (ms) után új munkamenet indul.
pub(crate) const SESSION_GAP_MS: u64 = 30 * 60 * 1000;
/// Ennyi különböző fájl érintés-számát tartja egy munkamenetben (utána az újak nem számolódnak).
pub(crate) const MAX_TOUCHES: usize = 500;

/// A hívás viszonya az előzőhöz ugyanabban a munkamenetben.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Rel {
    SameFile,
    SameDir,
    SameProject,
    OtherProject,
    /// Ennek vagy az előzőnek nincs célja, vagy ez a munkamenet első hívása.
    None,
}

impl Rel {
    #[cfg(test)]
    pub(crate) const ALL: [Self; 5] = [
        Self::SameFile,
        Self::SameDir,
        Self::SameProject,
        Self::OtherProject,
        Self::None,
    ];

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::SameFile => "same_file",
            Self::SameDir => "same_dir",
            Self::SameProject => "same_project",
            Self::OtherProject => "other_project",
            Self::None => "none",
        }
    }

    /// Két cél viszonya.
    pub(crate) fn between(previous: Option<&Keys>, current: Option<&Keys>) -> Self {
        let (Some(previous), Some(current)) = (previous, current) else {
            return Self::None;
        };
        if previous.file.is_some() && previous.file == current.file {
            Self::SameFile
        } else if previous.dir == current.dir {
            Self::SameDir
        } else if previous.project == current.project {
            Self::SameProject
        } else {
            Self::OtherProject
        }
    }
}

/// Hányadszor érinti ugyanazt a fájlt a munkamenetben: 1, 2–3, 4+.
pub(crate) fn rep_band(count: u32) -> &'static str {
    match count {
        0 | 1 => "1",
        2 | 3 => "2-3",
        _ => "4+",
    }
}

fn rep_index(count: u32) -> usize {
    match count {
        0 | 1 => 0,
        2 | 3 => 1,
        _ => 2,
    }
}

/// Az előző hívás vége és a mostani kezdete közti szünet sávja (a 30 percnél hosszabb már új munkamenet).
pub(crate) fn gap_band(gap_ms: u64) -> &'static str {
    match gap_ms {
        0..5_000 => "<5s",
        5_000..60_000 => "5-60s",
        60_000..600_000 => "1-10m",
        _ => "10-30m",
    }
}

/// A gap-sávok (a tesztek és az adatvédelmi ellenőrzés szótára); a `-` a munkamenet eleje / vége.
#[cfg(test)]
pub(crate) const GAP_BANDS: [&str; 5] = ["<5s", "5-60s", "1-10m", "10-30m", "-"];
/// Az érintés-szám sávjai (`rep`, `rep_max`): 1, 2–3, 4+.
#[cfg(test)]
pub(crate) const REP_LABELS: [&str; 3] = ["1", "2-3", "4+"];

// A `Sessions` tömbjeinek sávjai: `len` (hívások) 1, 2–5, 6–20, 21–50, 50+; `dur` <1 perc, 1–10 perc,
// 10–60 perc, 60 perc+; `rep_max` (a legtöbbször érintett fájl érintés-száma) 1, 2–3, 4+.

fn len_index(calls: u64) -> usize {
    match calls {
        0 | 1 => 0,
        2..=5 => 1,
        6..=20 => 2,
        21..=50 => 3,
        _ => 4,
    }
}

fn dur_index(ms: u64) -> usize {
    match ms {
        0..60_000 => 0,
        60_000..600_000 => 1,
        600_000..3_600_000 => 2,
        _ => 3,
    }
}

/// A lezárt munkamenetek sávos számai egy napra (a napi összesítőben és a beküldésben).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Sessions {
    pub(crate) count: u64,
    pub(crate) len: [u64; 5],
    pub(crate) dur: [u64; 4],
    pub(crate) rep_max: [u64; 3],
}

impl Sessions {
    /// Egy lezárt munkamenet hozzáadása. Ha egyetlen fájlt sem érintett, a `rep_max` nem nő.
    pub(crate) fn add(&mut self, closed: &Closed) {
        self.count += 1;
        self.len[len_index(closed.len)] += 1;
        self.dur[dur_index(closed.dur_ms)] += 1;
        if closed.rep_max > 0 {
            self.rep_max[rep_index(closed.rep_max)] += 1;
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.count == 0
    }
}

/// Egy most lezárult (az új munkamenet indulásakor) előző munkamenet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Closed {
    /// Az utolsó hívásának napja (ide számít).
    pub(crate) day: Day,
    /// Az utolsó hívásának parancsa (a `…>end` él).
    pub(crate) last: CommandKind,
    /// A hívások száma.
    pub(crate) len: u64,
    /// Az első hívás kezdetétől az utolsó végéig.
    pub(crate) dur_ms: u64,
    /// A legtöbbször érintett fájl érintés-száma (0 = egyetlen fájl sem).
    pub(crate) rep_max: u32,
}

impl Closed {
    /// A munkamenet-vég él kulcsa.
    pub(crate) fn end_edge(&self) -> String {
        format!("{}>end|none|-", self.last.name())
    }
}

/// Egy hívás helye a munkamenetben.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Step {
    pub(crate) sess: u64,
    pub(crate) seq: u64,
    pub(crate) rel: Rel,
    /// A célfájl érintés-száma ebben a munkamenetben (fájl nélkül `None`).
    pub(crate) rep: Option<u32>,
    /// Az átmenet-él kulcsa: `{előző}[!]>{ez}|{rel}|{szünet}`, az első hívásnál `start>{ez}|none|-`.
    pub(crate) edge: String,
    /// A most lezárult előző munkamenet, ha ez a hívás újat nyitott.
    pub(crate) closed: Option<Closed>,
}

/// Egy hívás a munkamenet-számításhoz (csak hash-ek és rögzített nevek).
#[derive(Debug, Clone)]
pub(crate) struct Visit {
    pub(crate) start_ms: u64,
    pub(crate) end_ms: u64,
    pub(crate) day: Day,
    pub(crate) command: CommandKind,
    pub(crate) failed: bool,
    pub(crate) keys: Option<Keys>,
}

/// A helyi munkamenet-állapot.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct SessionState {
    /// A telepítésenkénti véletlen kulcs (32 bájt hexában); soha nem megy el.
    pub(crate) key: String,
    /// Az aktuális munkamenet sorszáma (0 = még nem volt hívás).
    pub(crate) sess: u64,
    /// Az aktuális munkamenet hívásainak száma.
    pub(crate) seq: u64,
    /// Az aktuális munkamenet első hívásának kezdete (Unix-ms).
    pub(crate) start_ms: u64,
    /// Az utolsó hívás vége (Unix-ms).
    pub(crate) end_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) last_day: Option<Day>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) last_cmd: Option<CommandKind>,
    pub(crate) last_failed: bool,
    /// Az utolsó hívás céljának kulcsolt hash-ei.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) last: Option<Keys>,
    /// Kulcsolt fájl-hash → érintés-szám az aktuális munkamenetben.
    pub(crate) touches: BTreeMap<String, u32>,
}

impl SessionState {
    /// Betöltés; hiányzó vagy sérült fájlnál üres (új kulcs a `key_bytes`-nál).
    pub(crate) fn load(path: &Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub(crate) fn to_json(&self) -> String {
        let mut text = serde_json::to_string(self).unwrap_or_default();
        text.push('\n');
        text
    }

    /// A kulcs bájtjai; ha nincs (vagy érvénytelen), most jön létre.
    pub(crate) fn key_bytes(&mut self) -> Vec<u8> {
        if let Some(bytes) = from_hex(&self.key).filter(|b| b.len() == 32) {
            return bytes;
        }
        let key = random_key();
        self.key = to_hex(&key);
        key.to_vec()
    }

    /// Az aktuális munkamenet összegzése lezáráskor (ha volt benne hívás).
    fn closing(&self) -> Option<Closed> {
        if self.sess == 0 || self.seq == 0 {
            return None;
        }
        Some(Closed {
            day: self.last_day?,
            last: self.last_cmd?,
            len: self.seq,
            dur_ms: self.end_ms.saturating_sub(self.start_ms),
            rep_max: self.touches.values().copied().max().unwrap_or(0),
        })
    }

    /// Egy hívás hozzáadása: munkamenet-határ, sorszám, viszony, érintés-szám, átmenet-él.
    pub(crate) fn advance(&mut self, visit: Visit) -> Step {
        let gap = visit.start_ms.saturating_sub(self.end_ms);
        let continues = self.sess > 0 && self.last_cmd.is_some() && gap <= SESSION_GAP_MS;
        let closed = if continues { None } else { self.closing() };
        if !continues {
            self.sess += 1;
            self.seq = 0;
            self.start_ms = visit.start_ms;
            self.touches.clear();
            self.last = None;
        }
        self.seq += 1;
        let rel = if continues {
            Rel::between(self.last.as_ref(), visit.keys.as_ref())
        } else {
            Rel::None
        };
        let edge = match (continues, self.last_cmd) {
            (true, Some(previous)) => format!(
                "{}{}>{}|{}|{}",
                previous.name(),
                if self.last_failed { "!" } else { "" },
                visit.command.name(),
                rel.name(),
                gap_band(gap)
            ),
            _ => format!("start>{}|none|-", visit.command.name()),
        };
        let rep = visit
            .keys
            .as_ref()
            .and_then(|k| k.file.as_ref())
            .map(|file| {
                if let Some(count) = self.touches.get_mut(file) {
                    *count += 1;
                    *count
                } else {
                    if self.touches.len() < MAX_TOUCHES {
                        self.touches.insert(file.clone(), 1);
                    }
                    1
                }
            });
        self.end_ms = visit.end_ms.max(visit.start_ms);
        self.last_day = Some(visit.day);
        self.last_cmd = Some(visit.command);
        self.last_failed = visit.failed;
        self.last = visit.keys;
        Step {
            sess: self.sess,
            seq: self.seq,
            rel,
            rep,
            edge,
            closed,
        }
    }
}

/// Egy él hozzáadása a napi élekhez: legfeljebb `MAX_EDGES` különböző kulcs, az újabbak az `other`-be.
pub(crate) fn add_edge(edges: &mut BTreeMap<String, u64>, key: &str) {
    if let Some(n) = edges.get_mut(key) {
        *n += 1;
    } else if edges.len() < MAX_EDGES {
        edges.insert(key.to_owned(), 1);
    } else {
        *edges.entry(OTHER_EDGE.to_owned()).or_default() += 1;
    }
}

/// Egy nap legfeljebb ennyi különböző él-kulcsot tart.
pub(crate) const MAX_EDGES: usize = 500;
/// A túlcsorduló élek kulcsa.
pub(crate) const OTHER_EDGE: &str = "other";
