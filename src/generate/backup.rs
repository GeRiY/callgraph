//! Memento minta: minden módosítás előtt a fájl korábbi állapota a `.callgraph/backups/<relatív út>.<UTC
//! yyyymmddThhmmss>.bak` fájlba kerül (a projekt mappái között nem marad szemét, és a `.callgraph/` gitignore-olt);
//! fájlonként az utolsó 10 marad. Az `undo` ugyanebből állít vissza.

use std::path::{Path, PathBuf};

use super::error::{GenError, GenResult};
use super::fs::{Env, stamp};
use super::project::relative;
use crate::project::STORE_DIR;

/// Fájlonként ennyi mentés marad.
pub const KEEP: usize = 10;

/// A mentések kezelője.
pub struct Backups<'a> {
    env: Env<'a>,
}

/// Érvényes időbélyeg-e (`yyyymmddThhmmss`).
fn valid_stamp(text: &str) -> bool {
    text.len() == 15
        && text
            .bytes()
            .enumerate()
            .all(|(i, b)| if i == 8 { b == b'T' } else { b.is_ascii_digit() })
}

/// A mentés-fájl nevéből a `(fájlnév, időbélyeg)` pár.
fn split_name(name: &str) -> Option<(&str, &str)> {
    let (base, stamp) = name.strip_suffix(".bak")?.rsplit_once('.')?;
    valid_stamp(stamp).then_some((base, stamp))
}

impl<'a> Backups<'a> {
    pub fn new(env: Env<'a>) -> Self {
        Self { env }
    }

    fn dir(&self) -> PathBuf {
        self.env.root.join(STORE_DIR).join("backups")
    }

    fn path_for(&self, rel: &str, stamp: &str) -> PathBuf {
        let base = self.dir().join(rel);
        let name = base
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        base.with_file_name(format!("{name}.{stamp}.bak"))
    }

    /// A fájl mentései: `(időbélyeg, út)`, a legújabb elöl.
    pub fn list(&self, rel: &str) -> Vec<(String, PathBuf)> {
        let probe = self.path_for(rel, "x");
        let (Some(dir), Some(base)) = (
            probe.parent(),
            Path::new(rel).file_name().and_then(|n| n.to_str()),
        ) else {
            return Vec::new();
        };
        let mut found: Vec<(String, PathBuf)> = self
            .env
            .fs
            .list(dir)
            .unwrap_or_default()
            .into_iter()
            .filter(|e| !e.is_dir)
            .filter_map(|e| {
                let name = e.path.file_name()?.to_str()?.to_owned();
                let (b, s) = split_name(&name)?;
                (b == base).then(|| (s.to_owned(), e.path.clone()))
            })
            .collect();
        found.sort_by(|a, b| b.0.cmp(&a.0));
        found
    }

    /// A fájl legújabb mentése.
    pub fn newest(&self, rel: &str) -> Option<PathBuf> {
        self.list(rel).into_iter().next().map(|(_, path)| path)
    }

    /// A legutóbb mentett fájl (a legnagyobb időbélyegű mentés): `(relatív út, a mentés útja)`.
    pub fn newest_overall(&self) -> Option<(String, PathBuf)> {
        let base_dir = self.dir();
        let mut stack = vec![base_dir.clone()];
        let mut best: Option<(String, String, PathBuf)> = None;
        while let Some(dir) = stack.pop() {
            for entry in self.env.fs.list(&dir).unwrap_or_default() {
                if entry.is_dir {
                    stack.push(entry.path);
                    continue;
                }
                let Some(name) = entry.path.file_name().and_then(|n| n.to_str()) else {
                    continue;
                };
                let Some((base, stamp)) = split_name(name) else {
                    continue;
                };
                let Some(parent) = entry.path.parent().and_then(|p| relative(&base_dir, p)) else {
                    continue;
                };
                let rel = if parent.is_empty() {
                    base.to_owned()
                } else {
                    format!("{parent}/{base}")
                };
                if best.as_ref().is_none_or(|(s, _, _)| stamp > s.as_str()) {
                    best = Some((stamp.to_owned(), rel, entry.path.clone()));
                }
            }
        }
        best.map(|(_, rel, path)| (rel, path))
    }

    /// Menti a fájl jelenlegi tartalmát; a visszaadott érték a mentés projekt-relatív útja. Ha a `.callgraph/` nem
    /// írható, hibát ad (a hívó ilyenkor nem módosít).
    pub fn save(&self, rel: &str, content: &str) -> GenResult<String> {
        let mut secs = self.env.clock.now_secs();
        // az új mentés időbélyege mindig nagyobb a fájl legutóbbi mentéséénél: azonos másodpercben sem írunk felül
        // mentést, és az „újabb” sorrend akkor is igaz marad, ha az óra elmaradna egy korábban léptetett bélyegtől
        let newest = self.list(rel).into_iter().next().map(|(s, _)| s);
        for _ in 0..10_000 {
            if newest.as_ref().is_none_or(|n| stamp(secs) > *n) {
                break;
            }
            secs += 1;
        }
        let target = self.path_for(rel, &stamp(secs));
        self.env.fs.write(&target, content).map_err(|e| {
            GenError::new(crate::tf!(
                "cannot write the backup under {}: {} — refusing to modify the file",
                "nem írható a mentés ide: {}: {} — a fájlt nem módosítom",
                STORE_DIR,
                e
            ))
        })?;
        self.prune(rel);
        Ok(relative(self.env.root, &target).unwrap_or_else(|| target.display().to_string()))
    }

    /// A régi mentések törlése: fájlonként a legújabb `KEEP` marad.
    fn prune(&self, rel: &str) {
        for (_, path) in self.list(rel).into_iter().skip(KEEP) {
            let _ = self.env.fs.remove(&path);
        }
    }
}
