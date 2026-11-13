//! Strategy + Adapter: minden manifest- és lock-formátum egy stratégia, amely a saját formátumát a közös modellre
//! (`Manifest`, `Installed`) adaptálja. Egy új formátum = egy új típus + egy sor a regiszterben (`registry.rs`).
//! Hibás vagy ismeretlen alakú bemenetre a manifest `None`, a lock üres — soha nem hiba, nem pánik.

mod bun;
mod composer;
mod npm;
mod pnpm;
mod yarn;

#[cfg(test)]
mod tests;

use serde_json::Value;

pub(crate) use bun::BunLock;
pub(crate) use composer::{Composer, ComposerLock};
pub(crate) use npm::{Npm, NpmLock};
pub(crate) use pnpm::PnpmLock;
pub(crate) use yarn::YarnLock;

use super::model::{Dependency, Installed, Manifest};

/// Egy manifest-formátum (`composer.json`, `package.json`).
pub(crate) trait ManifestSource {
    /// A fájl neve a csomag gyökerében.
    fn file(&self) -> &'static str;
    /// A manifest értelmezése; nem értelmezhető szövegre `None`.
    fn read(&self, text: &str) -> Option<Manifest>;
}

/// Egy lock-formátum (`composer.lock`, `package-lock.json`, `pnpm-lock.yaml`, `yarn.lock`, `bun.lock`).
pub(crate) trait LockSource {
    /// A fájl neve a csomag gyökerében.
    fn file(&self) -> &'static str;
    /// A lock bejegyzései; nem értelmezhető szövegre üres.
    fn read(&self, text: &str) -> Vec<Installed>;
}

/// Egy JSON-objektum (`require`, `dependencies` …) függőségei a listához; a már felvett név (egy korábbi szakaszból)
/// és a `keep`-en el nem fogadott név kimarad.
fn push_deps(
    deps: &mut Vec<Dependency>,
    section: Option<&Value>,
    dev: bool,
    keep: fn(&str) -> bool,
) {
    let Some(map) = section.and_then(Value::as_object) else {
        return;
    };
    for (name, range) in map {
        if !keep(name) || deps.iter().any(|dep| dep.name == *name) {
            continue;
        }
        deps.push(Dependency {
            name: name.clone(),
            range: range.as_str().unwrap_or_default().trim().to_owned(),
            dev,
        });
    }
}

/// Egy lock-bejegyzés név + verzió alapján.
fn installed(name: &str, version: &str) -> Installed {
    Installed {
        name: name.to_owned(),
        version: version.to_owned(),
        ..Installed::default()
    }
}

/// Egy objektum-mező szövege.
fn text_field(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_owned)
}
