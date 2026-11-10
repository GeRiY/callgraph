//! A javaslatok tárolója: `<projekt>/.callgraph/results/results.jsonl` (a `.callgraph/` `*`-os `.gitignore`-ja
//! lefedi). Soronként egy eredmény: azonosító, létrehozás ideje, parancs-fajta, a `--project` értéke, és a
//! javaslatok futtatható argumentumai. Az utolsó 50 marad meg, 24 óra után lejár; az azonosító projektenként
//! növekvő (`r1`, `r2`, …: a legnagyobb tárolt + 1, a legutolsó rekord a lejárat után is megmarad, hogy ne
//! kezdődjön újra). Írás: fájlzár (`results.lock`, mint a `refresh.lock`) + atomikus átnevezés.

use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::suggestion::Suggestion;

/// Ennyi eredmény marad meg.
pub(crate) const CAPACITY: usize = 50;
/// Ennyi másodperc után jár le egy eredmény.
pub(crate) const EXPIRY_SECS: u64 = 24 * 60 * 60;
/// A mappa neve a `.callgraph/` alatt.
pub(crate) const DIR: &str = "results";
const FILE: &str = "results.jsonl";
const LOCK: &str = "results.lock";

/// Egy tárolt eredmény.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct StoredResult {
    pub id: u64,
    /// Létrehozás (Unix-másodperc).
    pub t: u64,
    /// A parancs-fajta neve (`locate`, `show`, `profile`).
    pub cmd: String,
    /// A hívás `--project` értéke (alias vagy mappa), ha volt — a sorban és a `next` kimenetében visszaadva.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    pub s: Vec<Suggestion>,
}

/// Egy projekt eredmény-tárolója.
#[derive(Debug, Clone)]
pub(crate) struct ResultStore {
    dir: PathBuf,
    /// A projekt gyökere: az első íráskor a `.callgraph/` (és a `.gitignore`) ezen át jön létre.
    root: Option<PathBuf>,
}

impl ResultStore {
    /// A projekt `.callgraph/results/` mappája.
    pub(crate) fn for_root(root: &Path) -> Self {
        Self {
            dir: root.join(crate::project::STORE_DIR).join(DIR),
            root: Some(root.to_path_buf()),
        }
    }

    /// Egy megadott mappa (tesztek).
    #[cfg(test)]
    pub(crate) fn at(dir: PathBuf) -> Self {
        Self { dir, root: None }
    }

    pub(crate) fn dir(&self) -> &Path {
        &self.dir
    }

    /// Egy új eredmény mentése; az új azonosító, vagy `None`, ha nem sikerült (akkor nincs javaslat-sor).
    pub(crate) fn save(
        &self,
        cmd: &str,
        project: Option<&str>,
        suggestions: &[Suggestion],
        now: u64,
    ) -> Option<u64> {
        if let Some(root) = &self.root {
            crate::project::ProjectStore::at_root(root).ensure().ok()?;
        }
        std::fs::create_dir_all(&self.dir).ok()?;
        let _lock = lock(&self.dir.join(LOCK))?;
        let mut records = self.load();
        let id = records.iter().map(|r| r.id).max().unwrap_or(0) + 1;
        records.push(StoredResult {
            id,
            t: now,
            cmd: cmd.to_owned(),
            project: project.map(str::to_owned),
            s: suggestions.to_vec(),
        });
        records.retain(|r| r.id == id || now.saturating_sub(r.t) < EXPIRY_SECS);
        let skip = records.len().saturating_sub(CAPACITY);
        let text: String = records
            .iter()
            .skip(skip)
            .filter_map(|r| serde_json::to_string(r).ok())
            .map(|line| line + "\n")
            .collect();
        write_atomic(&self.dir.join(FILE), &text).ok()?;
        Some(id)
    }

    /// Egy még érvényes eredmény.
    pub(crate) fn get(&self, id: u64, now: u64) -> Option<StoredResult> {
        self.load()
            .into_iter()
            .find(|r| r.id == id && now.saturating_sub(r.t) < EXPIRY_SECS)
    }

    fn load(&self) -> Vec<StoredResult> {
        std::fs::read_to_string(self.dir.join(FILE))
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect()
    }
}

/// Kizárólagos fájlzár legfeljebb 2 másodperc várakozással; zárat nem támogató fájlrendszeren zár nélkül megy
/// (az írás atomikus). `None`: egy másik folyamat tartja.
fn lock(path: &Path) -> Option<Option<File>> {
    let Ok(file) = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)
    else {
        return Some(None);
    };
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match file.try_lock() {
            Ok(()) => return Some(Some(file)),
            Err(TryLockError::Error(_)) => return Some(None),
            Err(TryLockError::WouldBlock) if Instant::now() >= deadline => return None,
            Err(TryLockError::WouldBlock) => std::thread::sleep(Duration::from_millis(10)),
        }
    }
}

/// Írás ideiglenes fájlba, majd átnevezés.
pub(crate) fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(format!(".tmp-{}", std::process::id()));
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}
