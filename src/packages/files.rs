//! DI: a csomagtérkép fájlrendszer-hozzáférése egy kis trait mögött (olvasás, bélyeg, létezés, almappák), így a
//! builder, a Composite és a cache memóriabeli fixture-rel tesztelhető.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Egy forrásfájl változás-bélyege (mint a projekt-gráfnál: mtime + méret + ctime, ns).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Stamp {
    pub mtime: u64,
    pub size: u64,
    pub ctime: u64,
}

/// A manifestek és lockok olvasása.
pub(crate) trait SourceFiles {
    /// A fájl szövege; ha nem olvasható, `None`.
    fn read(&self, path: &Path) -> Option<String>;
    /// A fájl bélyege; ha nincs ilyen fájl, `None`.
    fn stamp(&self, path: &Path) -> Option<Stamp>;
    /// Létezik-e a fájl.
    fn exists(&self, path: &Path) -> bool {
        self.stamp(path).is_some()
    }
    /// A mappa közvetlen almappái, név szerint rendezve.
    fn dirs(&self, dir: &Path) -> Vec<PathBuf>;
}

/// Az éles fájlrendszer.
pub(crate) struct DiskFiles;

impl SourceFiles for DiskFiles {
    fn read(&self, path: &Path) -> Option<String> {
        std::fs::read_to_string(path).ok()
    }

    fn stamp(&self, path: &Path) -> Option<Stamp> {
        if !path.is_file() {
            return None;
        }
        let stamp = crate::graph::FileStamp::of(path);
        Some(Stamp {
            mtime: stamp.mtime,
            size: stamp.size,
            ctime: stamp.ctime,
        })
    }

    fn dirs(&self, dir: &Path) -> Vec<PathBuf> {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Vec::new();
        };
        let mut dirs: Vec<PathBuf> = entries
            .filter_map(|entry| entry.ok().map(|e| e.path()))
            .filter(|path| path.is_dir())
            .collect();
        dirs.sort();
        dirs
    }
}
