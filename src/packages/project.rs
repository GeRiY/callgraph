//! Composite: a projekt csomagjai csomag-gyökerenként és ökoszisztémánként (`EcosystemPackages`). Gyökér: a projekt
//! gyökere, az első szintű almappák, ahol manifest van (a `node_modules`, `vendor`, `.git`, `.callgraph` kivételével),
//! és a gyökér `package.json` `workspaces` mintáinak mappái (első / második szint, csak az egyszerű `mappa/*` és
//! `mappa[/almappa]` alak). A lekérdezés a fájlhoz legközelebbi részt kérdezi; a stack-sor összesít.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::files::SourceFiles;
use super::model::{Ecosystem, Package};
use super::registry::ECOSYSTEMS;
use super::sources::{ManifestSource, Npm};

/// Mappák, amelyekben nem keresünk manifestet.
const SKIP: &[&str] = &["node_modules", "vendor", ".git", ".callgraph"];

/// Egy csomag-gyökér egy ökoszisztémája.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct EcosystemPackages {
    pub root: PathBuf,
    pub ecosystem: Ecosystem,
    /// A manifest saját csomagneve.
    pub name: Option<String>,
    /// A közvetlen függőségek a manifest sorrendjében.
    pub packages: Vec<Package>,
    /// A csak a lockban szereplő (tranzitív) csomagok — a névtér-feloldáshoz és a hiányzó függőségekhez.
    pub installed_only: Vec<Package>,
    /// PHP: a projekt saját psr-4 / psr-0 névterei.
    pub own_namespaces: Vec<String>,
    /// A használt lock fájlneve; lock nélkül (vagy olvashatatlan locknál) `None`.
    pub lock: Option<String>,
}

/// A projekt összes csomag-része, gyökerenként (a projekt gyökere elöl), azon belül a regiszter sorrendjében.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ProjectPackages {
    pub root: PathBuf,
    pub parts: Vec<EcosystemPackages>,
}

impl ProjectPackages {
    /// A fájlhoz / mappához legközelebbi (legmélyebb ős-gyökerű) rész az ökoszisztémában.
    #[cfg_attr(not(test), allow(dead_code))] // a szimbólum-feloldás (következő szakasz) használja
    pub(crate) fn nearest(&self, path: &Path, ecosystem: Ecosystem) -> Option<&EcosystemPackages> {
        self.parts
            .iter()
            .filter(|part| part.ecosystem == ecosystem && path.starts_with(&part.root))
            .max_by_key(|part| part.root.components().count())
    }

    /// Minden rész.
    pub(crate) fn all(&self) -> impl Iterator<Item = &EcosystemPackages> {
        self.parts.iter()
    }
}

/// A szóba jöhető csomag-gyökerek: (mappa, workspace-e). A workspace-mappa a saját lockja hiányában a gyökérét
/// használja (npm / pnpm / yarn workspace-ben csak a gyökérben van lock).
pub(crate) fn candidate_dirs(root: &Path, files: &dyn SourceFiles) -> Vec<(PathBuf, bool)> {
    let mut dirs = vec![(root.to_path_buf(), false)];
    dirs.extend(
        files
            .dirs(root)
            .into_iter()
            .filter(|dir| !skipped(dir))
            .map(|dir| (dir, false)),
    );
    let workspaces = files
        .read(&root.join(Npm.file()))
        .and_then(|text| Npm.read(&text))
        .map(|manifest| manifest.workspaces)
        .unwrap_or_default();
    for pattern in &workspaces {
        for dir in workspace_dirs(root, pattern, files) {
            match dirs.iter_mut().find(|(have, _)| *have == dir) {
                Some((_, workspace)) => *workspace = true,
                None => dirs.push((dir, true)),
            }
        }
    }
    dirs
}

/// A szóba jöhető manifest- és lock-fájlok (a cache kulcsa: ezek bélyege).
pub(crate) fn candidate_files(dirs: &[(PathBuf, bool)]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for (dir, _) in dirs {
        for spec in ECOSYSTEMS {
            out.push(dir.join(spec.manifest.file()));
            for lock in spec.locks {
                out.push(dir.join(lock.file()));
            }
        }
    }
    out
}

/// A csomagtérkép felépítése a már összegyűjtött gyökerekből.
pub(crate) fn build(
    root: &Path,
    dirs: &[(PathBuf, bool)],
    files: &dyn SourceFiles,
) -> ProjectPackages {
    let mut parts = Vec::new();
    for (dir, workspace) in dirs {
        let fallback = (*workspace && dir.as_path() != root).then_some(root);
        for spec in ECOSYSTEMS {
            parts.extend(super::builder::build(dir, spec, fallback, files));
        }
    }
    ProjectPackages {
        root: root.to_path_buf(),
        parts,
    }
}

/// Kimarad-e a mappa (név szerint).
fn skipped(dir: &Path) -> bool {
    dir.file_name()
        .and_then(|name| name.to_str())
        .is_none_or(|name| SKIP.contains(&name))
}

/// Egy `workspaces` minta mappái: `apps/web`, `web` (közvetlenül) vagy `packages/*` (a mappa almappái).
fn workspace_dirs(root: &Path, pattern: &str, files: &dyn SourceFiles) -> Vec<PathBuf> {
    let pattern = pattern
        .trim()
        .trim_start_matches("./")
        .trim_end_matches('/');
    let segments: Vec<&str> = pattern.split('/').collect();
    match segments.as_slice() {
        [one] if plain(one) => vec![root.join(one)],
        [one, "*"] if plain(one) => files
            .dirs(&root.join(one))
            .into_iter()
            .filter(|dir| !skipped(dir))
            .collect(),
        [one, two] if plain(one) && plain(two) => vec![root.join(one).join(two)],
        _ => Vec::new(),
    }
}

/// Glob- és út-jel nélküli mappanév.
fn plain(segment: &str) -> bool {
    !segment.is_empty()
        && segment != "."
        && segment != ".."
        && !segment.contains(['*', '?', '[', '{', '!'])
}
