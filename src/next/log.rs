//! Helyi esemény-napló a 2. fázishoz (rangsorolás): `<projekt>/.callgraph/results/log.jsonl`, hálózat nincs.
//! Események: `shown` (eredmény-id, a javaslatok fajtái), `accepted` (id, választás, fajta, a tárgyak rövid
//! hash-e), és az elfogadás utáni legfeljebb 2 parancsra egy-egy `step` (`backtrack: true`, ha ugyanarra a
//! fájlra / névre egy MÁS parancs jött — a javaslat valószínűleg nem az volt, ami kellett). Út, név, szöveg nem
//! kerül bele, csak rövid hash. ~1 MB fölött `log.1.jsonl`-re forgat (a régebbi elvész).

use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::suggestion::SuggestionKind;

/// Ekkora méret fölött forgat.
pub(crate) const ROTATE_BYTES: u64 = 1024 * 1024;
/// Ennyi parancsot figyel egy elfogadás után.
pub(crate) const BACKTRACK_WINDOW: usize = 2;
const FILE: &str = "log.jsonl";
const ROTATED: &str = "log.1.jsonl";
/// A napló végéből ennyit olvas a visszalépés-jelhez.
const TAIL_BYTES: u64 = 8 * 1024;

/// Egy napló-esemény.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "ev", rename_all = "snake_case")]
pub(crate) enum LogEvent {
    Shown {
        t: u64,
        id: u64,
        cmd: String,
        kinds: Vec<SuggestionKind>,
    },
    Accepted {
        t: u64,
        id: u64,
        choice: usize,
        kind: SuggestionKind,
        /// A javasolt parancs tárgyainak hash-e (fájlnév, metódusnév, hívó-minta neve).
        subj: Vec<String>,
        /// A javasolt parancs argumentumainak hash-e.
        sig: String,
    },
    Step {
        t: u64,
        backtrack: bool,
    },
}

/// A napló egy projekt `results/` mappájában.
#[derive(Debug, Clone)]
pub(crate) struct EventLog {
    dir: PathBuf,
}

impl EventLog {
    pub(crate) fn new(dir: &Path) -> Self {
        Self {
            dir: dir.to_path_buf(),
        }
    }

    /// Egy esemény hozzáfűzése (minden hibát elnyel).
    pub(crate) fn append(&self, event: &LogEvent) {
        let path = self.dir.join(FILE);
        if std::fs::metadata(&path).is_ok_and(|m| m.len() > ROTATE_BYTES) {
            let _ = std::fs::rename(&path, self.dir.join(ROTATED));
        }
        let Ok(mut line) = serde_json::to_string(event) else {
            return;
        };
        line.push('\n');
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path);
        if let Ok(mut file) = file {
            let _ = file.write_all(line.as_bytes());
        }
    }

    /// Egy (nem `next`) parancs a javaslat-figyelés szemszögéből: ha az utolsó elfogadás óta még
    /// `BACKTRACK_WINDOW`-nál kevesebb parancs volt, egy `step` (visszalépés, ha ugyanarra a tárgyra más parancs jött).
    pub(crate) fn observe(&self, subjects: &[String], args: &[String], now: u64) {
        if !self.dir.join(FILE).is_file() {
            return;
        }
        let tail = self.tail();
        let Some(at) = tail
            .iter()
            .rposition(|e| matches!(e, LogEvent::Accepted { .. }))
        else {
            return;
        };
        let LogEvent::Accepted { subj, sig, .. } = &tail[at] else {
            return;
        };
        let steps = tail[at + 1..]
            .iter()
            .filter(|e| matches!(e, LogEvent::Step { .. }))
            .count();
        if steps >= BACKTRACK_WINDOW {
            return;
        }
        let hashed: Vec<String> = subjects.iter().map(|s| hash(s)).collect();
        let backtrack = *sig != hash_args(args) && hashed.iter().any(|h| subj.contains(h));
        self.append(&LogEvent::Step { t: now, backtrack });
    }

    /// Az utolsó események (a napló vége, sérült sorok nélkül).
    pub(crate) fn tail(&self) -> Vec<LogEvent> {
        let Ok(mut file) = std::fs::File::open(self.dir.join(FILE)) else {
            return Vec::new();
        };
        let len = file.metadata().map_or(0, |m| m.len());
        let start = len.saturating_sub(TAIL_BYTES);
        let mut text = String::new();
        if file.seek(SeekFrom::Start(start)).is_err() {
            return Vec::new();
        }
        let mut bytes = Vec::new();
        let _ = file.read_to_end(&mut bytes);
        text.push_str(&String::from_utf8_lossy(&bytes));
        // a levágott első sor töredék lehet
        let skip = usize::from(start > 0);
        text.lines()
            .skip(skip)
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect()
    }
}

/// Rövid, nem kriptográfiai hash (FNV-1a 64, hexa): csak összevetésre.
pub(crate) fn hash(text: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.bytes() {
        h ^= u64::from(byte);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// Az argumentumok hash-e (a `--project` / `--label` értéke nélkül: a burokbeli és a tárolt alak összevethető).
pub(crate) fn hash_args(args: &[String]) -> String {
    let mut kept: Vec<&str> = Vec::new();
    let mut it = args.iter().skip(1);
    while let Some(a) = it.next() {
        if a == "--project" || a == "--label" {
            it.next();
            continue;
        }
        let a = a.as_str();
        kept.push(a.rsplit('/').next().unwrap_or(a));
    }
    hash(&kept.join("\u{1f}"))
}
