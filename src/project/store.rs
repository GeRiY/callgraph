//! `ProjectStore` (Repository): a projekten belüli `.callgraph/` mappa.
//!
//! Minden eszköz-adat (cache, index, config) itt él, a projekt gyökerén belül; a mappa saját,
//! `*`-os `.gitignore`-t kap, így a projekt `.gitignore`-ját nem kell módosítani.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::entity::Project;
use crate::error::Result;

/// A tároló mappa neve a projekt gyökerében.
pub const STORE_DIR: &str = ".callgraph";
const GITIGNORE: &str = ".gitignore";
const GITIGNORE_BODY: &str = "*\n";

/// A projekt `.callgraph/` mappájának elérése. Az út-lekérdezések nem írnak; csak az `ensure` ír.
#[derive(Debug, Clone)]
pub struct ProjectStore {
    root: PathBuf,
    dir: PathBuf,
}

impl ProjectStore {
    /// A projekt tárolója (fájlrendszer-művelet nélkül).
    pub fn new(project: &Project) -> Self {
        Self::at_root(project.root())
    }

    /// A tároló egy gyökér-útból.
    pub fn at_root(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            dir: root.join(STORE_DIR),
        }
    }

    /// A `.callgraph/` mappa útja.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// A cache fájlja (`cache.jsonl`).
    pub fn cache_path(&self) -> PathBuf {
        self.dir.join("cache.jsonl")
    }

    /// Az index fájlja (`index.jsonl`).
    pub fn index_path(&self) -> PathBuf {
        self.dir.join("index.jsonl")
    }

    /// A konfiguráció fájlja (`config.json`).
    pub fn config_path(&self) -> PathBuf {
        self.dir.join("config.json")
    }

    /// Létrehozza a mappát és a `*`-os `.gitignore`-t (ha még nincs); többszöri hívás ártalmatlan.
    /// Hibát ad, ha a mappa vagy a `.gitignore` symlinkként a projekt gyökerén kívülre mutatna.
    pub fn ensure(&self) -> Result<()> {
        let root = self.root.canonicalize()?;
        if let Ok(meta) = fs::symlink_metadata(&self.dir) {
            if !meta.is_dir() {
                return Err(outside(&self.dir));
            }
        } else {
            fs::create_dir_all(&self.dir)?;
        }
        if !self.dir.canonicalize()?.starts_with(&root) {
            return Err(outside(&self.dir));
        }
        let ignore = self.dir.join(GITIGNORE);
        match fs::symlink_metadata(&ignore) {
            Ok(meta) if meta.file_type().is_symlink() => Err(outside(&ignore)),
            Ok(_) => Ok(()),
            Err(_) => Ok(fs::write(&ignore, GITIGNORE_BODY)?),
        }
    }
}

fn outside(path: &Path) -> crate::error::CallgraphError {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        format!(
            "{}: nem a projekt gyökerén belüli közönséges mappa/fájl",
            path.display()
        ),
    )
    .into()
}
