//! Builder: egy mappa + egy ökoszisztéma manifestjének és lockjának összefésülése — deklarált tartomány, a lock
//! pontos verziója, dev-jelző, PHP-névterek. A csak a lockban szereplő (tranzitív) csomagok külön listába kerülnek.
//! Hibatűrés egy helyen: hibás manifest → üres rész, hiányzó / hibás lock → csak tartományok.

use std::collections::HashSet;
use std::path::Path;

use super::files::SourceFiles;
use super::model::{Dependency, Installed, Package};
use super::project::EcosystemPackages;
use super::registry::EcosystemSpec;

/// A mappa egy ökoszisztémájának csomagjai; `None`, ha nincs ott a manifest. `fallback_lock`: ahol a lockot keresni
/// kell, ha a mappában nincs (workspace-csomagnál a gyökér).
pub(crate) fn build(
    dir: &Path,
    spec: &EcosystemSpec,
    fallback_lock: Option<&Path>,
    files: &dyn SourceFiles,
) -> Option<EcosystemPackages> {
    let text = files.read(&dir.join(spec.manifest.file()))?;
    let manifest = spec.manifest.read(&text).unwrap_or_default();
    let found = read_lock(dir, spec, files)
        .or_else(|| fallback_lock.and_then(|other| read_lock(other, spec, files)));
    let (lock, installed) = match found {
        Some((name, list)) if !list.is_empty() => (Some(name.to_owned()), list),
        _ => (None, Vec::new()),
    };
    let packages: Vec<Package> = manifest
        .deps
        .iter()
        .map(|dep| {
            let hit = pick(&installed, dep);
            Package {
                name: dep.name.clone(),
                ecosystem: spec.id,
                declared: Some(dep.range.clone()).filter(|range| !range.is_empty()),
                installed: hit
                    .map(|item| item.version.clone())
                    .filter(|version| !version.is_empty()),
                dev: dep.dev,
                namespaces: hit.map(|item| item.namespaces.clone()).unwrap_or_default(),
            }
        })
        .collect();
    let mut seen: HashSet<&str> = manifest.deps.iter().map(|dep| dep.name.as_str()).collect();
    let mut installed_only: Vec<Package> = Vec::new();
    for item in &installed {
        if !seen.insert(item.name.as_str()) {
            continue;
        }
        installed_only.push(Package {
            name: item.name.clone(),
            ecosystem: spec.id,
            declared: None,
            installed: Some(item.version.clone()).filter(|version| !version.is_empty()),
            dev: false,
            namespaces: item.namespaces.clone(),
        });
    }
    Some(EcosystemPackages {
        root: dir.to_path_buf(),
        ecosystem: spec.id,
        name: manifest.name,
        packages,
        installed_only,
        own_namespaces: manifest.own_namespaces,
        lock,
    })
}

/// A mappa első létező lockja (a regiszter sorrendjében) és a bejegyzései; olvashatatlan / hibás lockra üres lista.
fn read_lock(
    dir: &Path,
    spec: &EcosystemSpec,
    files: &dyn SourceFiles,
) -> Option<(&'static str, Vec<Installed>)> {
    let lock = spec
        .locks
        .iter()
        .find(|lock| files.exists(&dir.join(lock.file())))?;
    let installed = files
        .read(&dir.join(lock.file()))
        .map(|text| lock.read(&text))
        .unwrap_or_default();
    Some((lock.file(), installed))
}

/// A függőség lock-bejegyzése: ha több verzió van (yarn), az, amelyik a deklarált tartományra oldódott fel.
fn pick<'a>(installed: &'a [Installed], dep: &Dependency) -> Option<&'a Installed> {
    installed
        .iter()
        .find(|item| item.name == dep.name && item.ranges.contains(&dep.range))
        .or_else(|| installed.iter().find(|item| item.name == dep.name))
}
