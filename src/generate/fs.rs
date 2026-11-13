//! Fájlrendszer és óra, befecskendezhető (DI) felületen: az éles megvalósítás a lemezre ír, a tesztek memóriában
//! futnak rögzített órával. A generátor minden I/O-ja ezen át megy (`Generator`, `Backups`, osztály-index).

use std::io;
use std::path::{Path, PathBuf};

/// Egy mappa-bejegyzés.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    pub path: PathBuf,
    pub is_dir: bool,
}

/// A generátor által használt fájlműveletek (abszolút utakkal).
pub trait FileSystem {
    /// A fájl szövege (csak érvényes UTF-8: a nem UTF-8 fájlt nem írjuk vissza átkódolva).
    fn read(&self, path: &Path) -> io::Result<String>;
    /// Felülírja (vagy létrehozza) a fájlt; a szülő mappákat létrehozza.
    fn write(&self, path: &Path, content: &str) -> io::Result<()>;
    /// Csak új fájlt hoz létre: létező fájlra `AlreadyExists` hiba (soha nem ír felül).
    fn create_new(&self, path: &Path, content: &str) -> io::Result<()>;
    fn exists(&self, path: &Path) -> bool;
    fn list(&self, dir: &Path) -> io::Result<Vec<DirEntry>>;
    fn remove(&self, path: &Path) -> io::Result<()>;
    /// Védett-e a fájl (`.callgraphignore` `protect` minta): védettet nem módosítunk.
    fn is_protected(&self, _path: &Path) -> bool {
        false
    }
}

/// Unix-másodperc forrása.
pub trait Clock {
    fn now_secs(&self) -> u64;
}

/// Az éles fájlrendszer.
#[derive(Debug, Default, Clone, Copy)]
pub struct RealFs;

impl FileSystem for RealFs {
    fn read(&self, path: &Path) -> io::Result<String> {
        std::fs::read_to_string(path)
    }

    fn write(&self, path: &Path, content: &str) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, content)
    }

    fn create_new(&self, path: &Path, content: &str) -> io::Result<()> {
        use std::io::Write;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        file.write_all(content.as_bytes())
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn list(&self, dir: &Path) -> io::Result<Vec<DirEntry>> {
        let mut entries = Vec::new();
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            // a szimbolikus linket nem követjük (hurok ellen): nem mappa
            let is_dir = entry.file_type().is_ok_and(|t| t.is_dir());
            entries.push(DirEntry {
                path: entry.path(),
                is_dir,
            });
        }
        Ok(entries)
    }

    fn remove(&self, path: &Path) -> io::Result<()> {
        std::fs::remove_file(path)
    }

    fn is_protected(&self, path: &Path) -> bool {
        crate::io::is_protected(path)
    }
}

/// A rendszeróra.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_secs(&self) -> u64 {
        crate::update::check::now_secs()
    }
}

/// `yyyymmddThhmmss` (UTC) a Unix-másodpercből; a biztonsági mentések nevében ez rendez időrendben.
pub fn stamp(secs: u64) -> String {
    let (year, month, day) =
        crate::update::usage::civil(i64::try_from(secs / 86_400).unwrap_or(0));
    let rest = secs % 86_400;
    format!(
        "{year:04}{month:02}{day:02}T{:02}{:02}{:02}",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

/// Mappák, amelyekbe a PHP-fájl bejárás nem lép be.
fn skipped_dir(name: &str) -> bool {
    name.starts_with('.') || matches!(name, "vendor" | "node_modules" | "storage" | "cache")
}

/// A mappa `.php` fájljai lustán (mélységi bejárás, mappánként rendezve): a hívó az első találatnál megállhat, a
/// többi mappát már nem is listázzuk.
pub fn walk_php<'a>(fs: &'a dyn FileSystem, dir: PathBuf) -> impl Iterator<Item = PathBuf> + 'a {
    let mut stack = vec![dir];
    let mut pending: std::vec::IntoIter<PathBuf> = Vec::new().into_iter();
    std::iter::from_fn(move || {
        loop {
            if let Some(path) = pending.next() {
                return Some(path);
            }
            let current = stack.pop()?;
            let Ok(mut entries) = fs.list(&current) else {
                continue;
            };
            entries.sort_by(|a, b| a.path.cmp(&b.path));
            let mut files = Vec::new();
            for entry in entries.into_iter().rev() {
                let name = entry
                    .path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_owned();
                if entry.is_dir {
                    if !skipped_dir(&name) {
                        stack.push(entry.path);
                    }
                } else if name.ends_with(".php") {
                    files.push(entry.path);
                }
            }
            files.reverse();
            pending = files.into_iter();
        }
    })
}

/// A generátor környezete: projekt-gyökér, valamint a befecskendezett fájlrendszer és óra.
#[derive(Clone, Copy)]
pub struct Env<'a> {
    pub root: &'a Path,
    pub fs: &'a dyn FileSystem,
    pub clock: &'a dyn Clock,
}
