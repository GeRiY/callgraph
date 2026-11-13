//! Csomagfelismerés (offline, csak olvas): a projekt Composer- és npm / pnpm / yarn / bun-függőségei a manifestekből és
//! a lockokból. A profil (stack-sor, `--packages`), a framework-felismerés és a PHP framework-detektor ezen az egy
//! forráson át látja a csomagokat.
//!
//! - `mod.rs`: Facade — `Packages`, `open()`, `Packages::from_map()`;
//! - `model.rs`: `Package`, `Manifest`, `Installed`, `Ecosystem`;
//! - `sources/`: Strategy + Adapter — egy formátum egy típus (`ManifestSource`, `LockSource`);
//! - `registry.rs`: Registry / Factory — `ECOSYSTEMS` (melyik manifest és lockok), `FRAMEWORKS`;
//! - `builder.rs`: Builder — manifest + lock → csomagok; `project.rs`: Composite — gyökerenként (monorepo);
//! - `cache.rs`: Proxy — `.callgraph/packages.bin`, bélyeg szerint újraépítve; `files.rs`: DI a fájlrendszerhez;
//! - `stack.rs`: a kiemelt csomagok sora.
//!
//! Minden hibatűrő: hiányzó vagy hibás manifest / lock → az a rész üres, soha nem hiba.

mod builder;
mod cache;
mod files;
mod model;
mod project;
mod registry;
mod sources;
mod stack;

#[cfg(test)]
mod tests;

use std::fmt::Write as _;
use std::path::Path;

use crate::i18n::Lang;

pub(crate) use model::Package;
pub(crate) use project::ProjectPackages;
pub(crate) use stack::Stack;

#[cfg(test)]
pub(crate) use model::Ecosystem;
#[cfg(test)]
pub(crate) use project::EcosystemPackages;

use registry::FRAMEWORKS;
use sources::{Composer, ManifestSource};

/// A projekt csomagjai (Facade).
#[derive(Debug, Clone, Default)]
pub(crate) struct Packages {
    map: ProjectPackages,
}

/// A gyökér csomagtérképe: a `.callgraph/packages.bin`-ből, ha naprakész, különben a manifestekből és lockokból.
/// Soha nem bukik el; ha semmi nincs, üres.
pub(crate) fn open(root: &Path) -> Packages {
    let map = cache::load_or_build(root, &files::DiskFiles, &cache::DirCache::at(root));
    Packages::from_map(map)
}

/// A `composer.json` szövegének függőségei (név, dev-e) a manifest sorrendjében, a platform-bejegyzések nélkül —
/// a PHP framework-detektornak (ugyanaz az olvasó, mint a csomagtérképé).
pub(crate) fn composer_requires(json: &str) -> Vec<(String, bool)> {
    Composer
        .read(json)
        .map(|manifest| {
            manifest
                .deps
                .into_iter()
                .map(|dep| (dep.name, dep.dev))
                .collect()
        })
        .unwrap_or_default()
}

/// A `package.json` szövegének frameworkjei a `FRAMEWORKS` sorrendjében (a profil egységtesztjeihez).
#[cfg(test)]
pub(crate) fn node_frameworks(json: &str) -> Vec<&'static str> {
    let deps = sources::Npm
        .read(json)
        .map(|manifest| manifest.deps)
        .unwrap_or_default();
    FRAMEWORKS
        .iter()
        .filter(|(ecosystem, package, _)| {
            *ecosystem == model::Ecosystem::Node && deps.iter().any(|dep| dep.name == *package)
        })
        .map(|(_, _, name)| *name)
        .collect()
}

impl Packages {
    /// Egy kész csomagtérképből (a tesztek és az `open`).
    pub(crate) fn from_map(map: ProjectPackages) -> Self {
        Self { map }
    }

    /// A stack-sor adatai.
    pub(crate) fn stack(&self) -> Stack {
        Stack::of(&self.map)
    }

    /// Minden közvetlen függőség, részenként (a részek sorrendjében).
    pub(crate) fn all_direct(&self) -> Vec<&Package> {
        self.map.all().flat_map(|part| &part.packages).collect()
    }

    /// Nincs egyetlen közvetlen függőség sem.
    pub(crate) fn is_empty(&self) -> bool {
        self.all_direct().is_empty()
    }

    /// A függőségekből felismert frameworkök (`laravel`, `vue` …): részenként, a `FRAMEWORKS` sorrendjében,
    /// ismétlés nélkül.
    pub(crate) fn frameworks(&self) -> Vec<&'static str> {
        let mut out: Vec<&'static str> = Vec::new();
        for part in self.map.all() {
            for &(ecosystem, package, name) in FRAMEWORKS {
                let present = part
                    .packages
                    .iter()
                    .any(|p| p.ecosystem == ecosystem && p.name == package);
                if present && !out.contains(&name) {
                    out.push(name);
                }
            }
        }
        out
    }

    /// A `profile --packages` lista: részenként egy fejléc (a gyökérhez relatív mappa, `.` a gyökér, és a lock),
    /// alatta a közvetlen függőségek — előbb a prod, utána a dev, név szerint: `név  telepített (deklarált)  [dev]`.
    pub(crate) fn listing(&self, lang: Lang) -> String {
        let mut out = String::new();
        if self.is_empty() {
            out.push_str(lang.pick(
                "packages: no dependencies found (composer.json / package.json)\n",
                "csomagok: nincs függőség (composer.json / package.json)\n",
            ));
            return out;
        }
        for part in self.map.all().filter(|part| !part.packages.is_empty()) {
            let folder = part
                .root
                .strip_prefix(&self.map.root)
                .ok()
                .filter(|rel| !rel.as_os_str().is_empty())
                .map_or_else(
                    || ".".to_owned(),
                    |rel| rel.to_string_lossy().replace('\\', "/"),
                );
            let own = part
                .name
                .as_deref()
                .map(|name| format!(" ({name})"))
                .unwrap_or_default();
            let ecosystem = part.ecosystem.label();
            let _ = match (&part.lock, lang) {
                (Some(lock), Lang::En) => {
                    writeln!(out, "packages [{ecosystem}] {folder}{own} · {lock}")
                }
                (Some(lock), Lang::Hu) => {
                    writeln!(out, "csomagok [{ecosystem}] {folder}{own} · {lock}")
                }
                (None, Lang::En) => writeln!(
                    out,
                    "packages [{ecosystem}] {folder}{own} · no lock file (declared ranges only)"
                ),
                (None, Lang::Hu) => writeln!(
                    out,
                    "csomagok [{ecosystem}] {folder}{own} · nincs lock-fájl (csak a deklarált tartományok)"
                ),
            };
            let mut list: Vec<&Package> = part.packages.iter().collect();
            list.sort_by(|a, b| a.dev.cmp(&b.dev).then_with(|| a.name.cmp(&b.name)));
            let width = list
                .iter()
                .map(|package| package.name.chars().count())
                .max()
                .unwrap_or(0);
            for package in list {
                let mut line = format!("  {:<width$}", package.name);
                if let Some(installed) = &package.installed {
                    let _ = write!(line, "  {installed}");
                }
                if let Some(declared) = &package.declared {
                    let gap = if package.installed.is_some() {
                        " "
                    } else {
                        "  "
                    };
                    let _ = write!(line, "{gap}({declared})");
                }
                if package.dev {
                    line.push_str("  [dev]");
                }
                out.push_str(line.trim_end());
                out.push('\n');
            }
        }
        out
    }
}
