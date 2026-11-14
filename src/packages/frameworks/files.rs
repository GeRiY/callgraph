//! DI: a framework-kötések fájl-hozzáférése (egy fájl szövege, egy mappa fájljai) egy kis trait mögött, így a
//! page-gyökér felismerése, a page-lista és a render-hívások forrásbeli argumentumai memóriabeli fixture-rel
//! tesztelhetők.

use std::path::{Path, PathBuf};

pub(crate) use crate::packages::files::Stamp;

/// Egy mappa bejárásának felső korlátja (a page-mappa kicsi; egy rossz gyökér se járja be a fél lemezt).
const MAX_FILES: usize = 5000;

/// A projekt fájljainak olvasása.
pub(crate) trait ProjectFiles {
    /// A fájl szövege; ha nem olvasható, `None`.
    fn read(&self, path: &Path) -> Option<String>;
    /// A mappa fájljai rekurzívan (abszolút utak), rendezve; nem létező mappára üres.
    fn files_under(&self, dir: &Path) -> Vec<PathBuf>;
    /// A fájl változás-bélyege (mtime + méret + ctime); ha nincs ilyen fájl vagy a fájlrendszer nem ad bélyeget,
    /// `None`. A lemezes cache (`laravel/cache.rs`) ebből dönti el, hogy a levezetett adat még él-e.
    fn stamp(&self, _path: &Path) -> Option<Stamp> {
        None
    }
}

/// Az éles fájlrendszer.
pub(crate) struct DiskProjectFiles;

impl ProjectFiles for DiskProjectFiles {
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

    fn read(&self, path: &Path) -> Option<String> {
        let bytes = std::fs::read(path).ok()?;
        Some(crate::io::decode_source(&bytes).into_owned())
    }

    fn files_under(&self, dir: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        let mut stack = vec![dir.to_path_buf()];
        while let Some(current) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&current) else {
                continue;
            };
            for entry in entries.flatten() {
                let Ok(kind) = entry.file_type() else {
                    continue;
                };
                if kind.is_dir() {
                    if entry.file_name() != "node_modules" {
                        stack.push(entry.path());
                    }
                } else if kind.is_file() {
                    out.push(entry.path());
                }
            }
            if out.len() >= MAX_FILES {
                break;
            }
        }
        out.sort();
        out
    }
}
