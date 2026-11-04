//! `ProjectLocator`: fájl / mappa → projekt-gyökér, és fájlok csoportosítása projektenként.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::aliases::{AliasSource, NoAliases};
use super::entity::Project;

/// A projekt-gyökeret jelző bejegyzések (a legközelebbi felfelé nyer).
const MARKERS: [&str; 3] = ["composer.json", "package.json", ".git"];

/// Egy projekt és a hozzá tartozó bemeneti fájlok.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectGroup {
    pub project: Project,
    pub files: Vec<PathBuf>,
}

/// Megkeresi egy bemenet projekt-gyökerét; az alias-forrás a konstruktoron át jön (DI).
pub struct ProjectLocator {
    aliases: Box<dyn AliasSource + Send + Sync>,
}

impl Default for ProjectLocator {
    /// Alias nélküli locator.
    fn default() -> Self {
        Self::new(Box::new(NoAliases))
    }
}

impl ProjectLocator {
    /// Locator a megadott alias-forrással.
    pub fn new(aliases: Box<dyn AliasSource + Send + Sync>) -> Self {
        Self { aliases }
    }

    /// Egy fájlhoz vagy mappához tartozó projekt.
    pub fn locate(&self, input: &Path) -> Project {
        self.locate_cached(input, &mut HashMap::new())
    }

    /// Több bemenet csoportosítása projektenként (az első előfordulás sorrendjében).
    pub fn group<I, P>(&self, inputs: I) -> Vec<ProjectGroup>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        let mut cache = HashMap::new();
        let mut groups: Vec<ProjectGroup> = Vec::new();
        for input in inputs {
            let project = self.locate_cached(input.as_ref(), &mut cache);
            let file = input.as_ref().to_path_buf();
            match groups.iter_mut().find(|g| g.project == project) {
                Some(g) => g.files.push(file),
                None => groups.push(ProjectGroup {
                    project,
                    files: vec![file],
                }),
            }
        }
        groups
    }

    /// `cache`: kiindulási mappa → gyökér, hogy a közös szülők bejárása ne ismétlődjön.
    fn locate_cached(&self, input: &Path, cache: &mut HashMap<PathBuf, PathBuf>) -> Project {
        let start = start_dir(input);
        let root = match cache.get(&start) {
            Some(r) => r.clone(),
            None => {
                let r = find_root(&start).unwrap_or_else(|| start.clone());
                cache.insert(start, r.clone());
                r
            }
        };
        let project = Project::new(root);
        match self.aliases.alias_for(project.root()) {
            Some(alias) => project.with_alias(alias),
            None => project,
        }
    }
}

/// A bejárás kiindulási mappája: a (symlinkeket feloldó) mappa, fájlnál a szülője.
fn start_dir(input: &Path) -> PathBuf {
    let resolved = input.canonicalize().unwrap_or_else(|_| input.to_path_buf());
    if resolved.is_dir() {
        return resolved;
    }
    match resolved.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

/// Felfelé a legközelebbi jelölő-bejegyzést tartalmazó mappa.
fn find_root(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .find(|dir| MARKERS.iter().any(|m| dir.join(m).exists()))
        .map(Path::to_path_buf)
}
