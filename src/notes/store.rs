//! `NoteStore` (Repository): a `<projekt>/.callgraph/notes.jsonl` olvasása és atomikus írása.
//!
//! Soronként egy JSON-objektum (`"v": 1` verzió-mezővel). A nem értelmezhető sorokat (pl. egy későbbi verzió
//! sorait) az írás változatlanul megtartja. Írás: ideiglenes fájl ugyanabban a mappában, majd átnevezés.

use std::fs;
use std::path::{Path, PathBuf};

use super::note::{FORMAT_VERSION, Note};
use crate::error::{CallgraphError, Result};
use crate::project::ProjectStore;

/// A jegyzetfájl neve a `.callgraph/` mappában.
pub const NOTES_FILE: &str = "notes.jsonl";

/// Egy projekt jegyzetei.
#[derive(Debug, Clone)]
pub struct NoteStore {
    store: ProjectStore,
    file: PathBuf,
}

/// A beolvasott fájl: a jegyzetek és a nem értelmezett (megőrzendő) sorok.
struct Loaded {
    notes: Vec<Note>,
    foreign: Vec<String>,
}

impl NoteStore {
    /// A projekt-gyökér jegyzetei (fájlrendszer-művelet nélkül).
    pub fn at_root(root: &Path) -> Self {
        let store = ProjectStore::at_root(root);
        let file = store.dir().join(NOTES_FILE);
        Self { store, file }
    }

    /// A jegyzetfájl útja.
    pub fn path(&self) -> &Path {
        &self.file
    }

    /// Az összes jegyzet a fájl sorrendjében (nincs fájl → üres lista).
    pub fn list(&self) -> Result<Vec<Note>> {
        Ok(self.load()?.notes)
    }

    /// Új jegyzet; az időt a hívó adja (`created_at`, ISO-8601 UTC), az azonosítót a tároló.
    pub fn add(
        &self,
        path: &str,
        container: Option<String>,
        method: Option<String>,
        text: &str,
        created_at: &str,
    ) -> Result<Note> {
        let mut loaded = self.load()?;
        let id = loop {
            let id = crate::skill::clock::new_id();
            if !loaded.notes.iter().any(|n| n.id == id) {
                break id;
            }
        };
        let note = Note {
            v: FORMAT_VERSION,
            id,
            path: path.to_owned(),
            container,
            method,
            text: one_line(text),
            created_at: created_at.to_owned(),
        };
        loaded.notes.push(note.clone());
        self.save(&loaded)?;
        Ok(note)
    }

    /// Törlés azonosító szerint; a törölt jegyzet, vagy `None`, ha nincs ilyen.
    pub fn remove(&self, id: &str) -> Result<Option<Note>> {
        let mut loaded = self.load()?;
        let Some(index) = loaded.notes.iter().position(|n| n.id == id) else {
            return Ok(None);
        };
        let removed = loaded.notes.remove(index);
        self.save(&loaded)?;
        Ok(Some(removed))
    }

    fn load(&self) -> Result<Loaded> {
        let text = match fs::read_to_string(&self.file) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(source) => {
                return Err(CallgraphError::Read {
                    path: self.file.clone(),
                    source,
                });
            }
        };
        let mut loaded = Loaded {
            notes: Vec::new(),
            foreign: Vec::new(),
        };
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            match serde_json::from_str::<Note>(line) {
                Ok(note) => loaded.notes.push(note),
                Err(_) => loaded.foreign.push(line.to_owned()),
            }
        }
        Ok(loaded)
    }

    /// Atomikus írás: `notes.jsonl.tmp-<pid>` → átnevezés `notes.jsonl`-re (a `.callgraph/` a `*`-os `.gitignore`-ral).
    fn save(&self, loaded: &Loaded) -> Result<()> {
        self.store.ensure()?;
        let mut body = String::new();
        for note in &loaded.notes {
            body.push_str(&serde_json::to_string(note)?);
            body.push('\n');
        }
        for line in &loaded.foreign {
            body.push_str(line);
            body.push('\n');
        }
        let tmp = self
            .file
            .with_file_name(format!("{NOTES_FILE}.tmp-{}", std::process::id()));
        fs::write(&tmp, body)?;
        if let Err(e) = fs::rename(&tmp, &self.file) {
            let _ = fs::remove_file(&tmp);
            return Err(e.into());
        }
        Ok(())
    }
}

/// A jegyzet egy sor: a sortöréseket és tabokat szóközre cseréli, a széleit levágja.
fn one_line(text: &str) -> String {
    text.split(['\n', '\r', '\t'])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn temp_root() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "callgraph-notes-store-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    const AT: &str = "2026-11-05T20:27:13Z";

    #[test]
    fn add_list_remove_round_trip() {
        let root = temp_root();
        let store = NoteStore::at_root(&root);
        assert!(store.list().unwrap().is_empty());
        let a = store.add("src/a.rs", None, None, "file note", AT).unwrap();
        let b = store
            .add(
                "src/a.rs",
                Some("Shop".into()),
                Some("run".into()),
                "line one\nline two",
                "2026-11-05T21:00:00Z",
            )
            .unwrap();
        assert_eq!(a.id.len(), 8);
        assert_ne!(a.id, b.id);
        assert_eq!(b.text, "line one line two");
        assert_eq!(a.created_at, AT);
        let all = store.list().unwrap();
        assert_eq!(all, vec![a.clone(), b.clone()]);
        // a .callgraph/ a *-os .gitignore-t kapja, ideiglenes fájl nem marad
        assert!(root.join(".callgraph/.gitignore").is_file());
        let names: Vec<String> = fs::read_dir(root.join(".callgraph"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(!names.iter().any(|n| n.contains(".tmp")), "{names:?}");
        assert_eq!(store.remove(&a.id).unwrap(), Some(a));
        assert_eq!(store.remove("nope").unwrap(), None);
        assert_eq!(store.list().unwrap(), vec![b]);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn file_format_is_versioned_jsonl_and_keeps_foreign_lines() {
        let root = temp_root();
        let store = NoteStore::at_root(&root);
        store
            .add("a.php", Some("Shop".into()), Some("run".into()), "x", AT)
            .unwrap();
        let text = fs::read_to_string(store.path()).unwrap();
        let line = text.lines().next().unwrap();
        let value: serde_json::Value = serde_json::from_str(line).unwrap();
        assert_eq!(value["v"], 1);
        assert_eq!(value["path"], "a.php");
        assert_eq!(value["container"], "Shop");
        assert_eq!(value["method"], "run");
        assert_eq!(value["created_at"], AT);
        // file-jegyzetnél nincs container / method mező
        let file_note = store.add("a.php", None, None, "y", AT).unwrap();
        let text = fs::read_to_string(store.path()).unwrap();
        assert!(!text.lines().nth(1).unwrap().contains("method"));
        // egy idegen (pl. újabb verziójú) sor az írás után is megmarad
        fs::write(store.path(), format!("{text}{{\"v\":2,\"weird\":true}}\n")).unwrap();
        store.remove(&file_note.id).unwrap();
        let text = fs::read_to_string(store.path()).unwrap();
        assert_eq!(text.lines().count(), 2);
        assert!(text.contains("\"weird\":true"));
        let _ = fs::remove_dir_all(&root);
    }
}
