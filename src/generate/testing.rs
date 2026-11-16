//! Közös teszt-fixture a generátor, a Vue-generátor, a preset és az átnevezés tesztjeihez: memóriabeli
//! fájlrendszer (`/p` a gyökér) és rögzített, léptethető óra. A `read` / `list` hívásokat számolja, így az IO-ra
//! felső korlátot lehet tenni (regresszióvédelem).

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use super::fs::{Clock, DirEntry, FileSystem};

/// A teszt-projekt gyökere.
pub const ROOT: &str = "/p";

/// Memóriabeli fájlrendszer (abszolút utak).
#[derive(Default)]
pub struct MemFs {
    pub files: RefCell<BTreeMap<PathBuf, String>>,
    reads: Cell<usize>,
    lists: Cell<usize>,
    read_log: RefCell<Vec<PathBuf>>,
}

impl MemFs {
    pub fn with(files: &[(&str, &str)]) -> Self {
        let fs = Self::default();
        for (path, text) in files {
            fs.put(path, text);
        }
        fs
    }

    pub fn get(&self, rel: &str) -> Option<String> {
        self.files.borrow().get(&Path::new(ROOT).join(rel)).cloned()
    }

    pub fn put(&self, rel: &str, text: &str) {
        self.files
            .borrow_mut()
            .insert(Path::new(ROOT).join(rel), text.to_owned());
    }

    pub fn snapshot(&self) -> BTreeMap<PathBuf, String> {
        self.files.borrow().clone()
    }

    /// A gyökér alatti, a mappa-előtagra illő fájlok (megjeleníthető útként).
    pub fn names_under(&self, rel: &str) -> Vec<String> {
        let dir = Path::new(ROOT).join(rel);
        self.files
            .borrow()
            .keys()
            .filter(|p| p.starts_with(&dir))
            .map(|p| p.display().to_string())
            .collect()
    }

    /// Van-e biztonsági mentés a fájlról (`.callgraph/backups/<út>…`).
    pub fn has_backup_of(&self, rel: &str) -> bool {
        // az elválasztó platformfüggő (Windows: `\`): normalizált összevetés
        let prefix = Path::new(ROOT)
            .join(".callgraph/backups")
            .join(rel)
            .to_string_lossy()
            .replace('\\', "/");
        self.files
            .borrow()
            .keys()
            .any(|p| p.to_string_lossy().replace('\\', "/").starts_with(&prefix))
    }

    /// Eddigi `read` hívások száma.
    pub fn read_calls(&self) -> usize {
        self.reads.get()
    }

    /// Hányszor olvasták a fájlt (a gyökérhez képesti úttal) a számlálók utolsó nullázása óta.
    pub fn reads_of(&self, rel: &str) -> usize {
        let path = Path::new(ROOT).join(rel);
        self.read_log
            .borrow()
            .iter()
            .filter(|p| **p == path)
            .count()
    }

    /// Eddigi `list` hívások száma.
    pub fn list_calls(&self) -> usize {
        self.lists.get()
    }

    /// Nullázza a számlálókat (a vizsgált művelet előtt).
    pub fn reset_counters(&self) {
        self.reads.set(0);
        self.lists.set(0);
        self.read_log.borrow_mut().clear();
    }
}

impl FileSystem for MemFs {
    fn read(&self, path: &Path) -> io::Result<String> {
        self.reads.set(self.reads.get() + 1);
        self.read_log.borrow_mut().push(path.to_path_buf());
        self.files
            .borrow()
            .get(path)
            .cloned()
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
    }

    fn write(&self, path: &Path, content: &str) -> io::Result<()> {
        self.files
            .borrow_mut()
            .insert(path.to_path_buf(), content.to_owned());
        Ok(())
    }

    fn create_new(&self, path: &Path, content: &str) -> io::Result<()> {
        if self.files.borrow().contains_key(path) {
            return Err(io::Error::from(io::ErrorKind::AlreadyExists));
        }
        self.write(path, content)
    }

    fn exists(&self, path: &Path) -> bool {
        self.files.borrow().contains_key(path)
    }

    fn list(&self, dir: &Path) -> io::Result<Vec<DirEntry>> {
        self.lists.set(self.lists.get() + 1);
        let mut entries: BTreeMap<PathBuf, bool> = BTreeMap::new();
        for path in self.files.borrow().keys() {
            let Ok(rest) = path.strip_prefix(dir) else {
                continue;
            };
            let mut parts = rest.components();
            let Some(first) = parts.next() else {
                continue;
            };
            let is_dir = parts.next().is_some();
            entries.insert(dir.join(first.as_os_str()), is_dir);
        }
        if entries.is_empty() {
            return Err(io::Error::from(io::ErrorKind::NotFound));
        }
        Ok(entries
            .into_iter()
            .map(|(path, is_dir)| DirEntry { path, is_dir })
            .collect())
    }

    fn remove(&self, path: &Path) -> io::Result<()> {
        self.files.borrow_mut().remove(path);
        Ok(())
    }
}

/// Rögzített, léptethető óra.
pub struct FixedClock(pub Cell<u64>);

impl Clock for FixedClock {
    fn now_secs(&self) -> u64 {
        self.0.get()
    }
}
