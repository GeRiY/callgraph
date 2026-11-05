//! Perzisztens projekt-gráf a projekt `.callgraph/` tárolójában: fájlonként egy bináris shard (a nyelvi frontend
//! `FileGraph`-ja veszteségmentesen + a hívás-sorok), egy kis manifest (út → mtime, méret, shard) és a hívó-index
//! bemenete (`callers.bin`). Fájlonként (mtime + méret) frissül; belőle épül a projekt-szintű szimbólum-keresés, a
//! hívó-index (mindkét irány) és a típus-hierarchia. Formátum és működés: `docs/PROJECT-GRAPH.md`.

mod codec;
mod pool;
mod record;
mod store;

#[cfg(test)]
mod tests;

use std::path::PathBuf;

use crate::lang::LanguageRegistry;

pub(crate) use pool::{SharedGraph, lock, shared};
pub(crate) use store::ProjectGraph;

/// A régi callgraph bejárási sorrendje: előbb a PHP-motor (fájlrendszer-sorrend), majd a JS-motor
/// (`readdirSync().sort()`: mappánként név szerint, ami komponensenkénti összehasonlítás). A hívó-index élei, és
/// így a kimenet holtversenyei is, ezt a sorrendet követik.
pub(crate) fn legacy_order(registry: &LanguageRegistry, files: Vec<PathBuf>) -> Vec<PathBuf> {
    let (mut js, mut rest): (Vec<_>, Vec<_>) = files
        .into_iter()
        .partition(|f| registry.for_path(f).is_some_and(|l| l.id() == "js"));
    js.sort();
    rest.extend(js);
    rest
}
