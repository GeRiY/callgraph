//! Előre-frissítés (`callgraph refresh`, a Claude Code-hook és az MCP-figyelő): a megadott fájlok (vagy a teljes
//! projekt) naprakészre hozása a gráfban, hogy a következő lekérdezésnek ne kelljen elemeznie. Két frissítő nem fut
//! egyszerre ugyanazon a tárolón: `RefreshLock` (`.callgraph/refresh.lock`, az operációs rendszer fájlzára — egy
//! összeomlott folyamat után magától feloldódik).

use std::collections::HashSet;
use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

use super::ProjectGraph;
use super::store::Refreshed;
use crate::analysis::Analyzer;
use crate::error::Result;
use crate::io::FileWalker;

/// A zár fájlneve a `.callgraph/` mappában (a `.gitignore` `*`-a lefedi; a tároló törlése nem érinti).
const LOCK_FILE: &str = "refresh.lock";

/// Egy frissítő kizárólagos joga a tárolóra; az eldobásakor (a fájl bezárásakor) szabadul fel.
pub(crate) struct RefreshLock {
    _file: Option<File>,
}

impl RefreshLock {
    /// A zár megszerzése legfeljebb `wait` ideig próbálkozva; `None`, ha egy másik frissítő tartja. Ha a
    /// fájlrendszer nem támogat zárat (vagy a zárfájl nem hozható létre), zár nélkül megy tovább — az írások
    /// atomikusak, a manifest a rekordokat bélyeggel ellenőrzi, így ez legfeljebb újraelemzést okoz.
    pub(crate) fn acquire(store: &Path, wait: Duration) -> Option<Self> {
        let Ok(file) = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(store.join(LOCK_FILE))
        else {
            return Some(Self { _file: None });
        };
        let deadline = Instant::now() + wait;
        loop {
            match file.try_lock() {
                Ok(()) => return Some(Self { _file: Some(file) }),
                Err(TryLockError::Error(_)) => return Some(Self { _file: None }),
                Err(TryLockError::WouldBlock) if Instant::now() >= deadline => return None,
                Err(TryLockError::WouldBlock) => std::thread::sleep(Duration::from_millis(25)),
            }
        }
    }
}

/// A teljes projekt naprakészre hozása: bejárás (mint a `callers`), a változott / új fájlok elemzése, a törölt /
/// átnevezett fájlok eltávolítása, a szűkített tények frissítése.
pub(crate) fn refresh_all(
    graph: &mut ProjectGraph,
    root: &Path,
    analyzer: &Analyzer,
) -> Result<Refreshed> {
    let files = super::legacy_order(
        analyzer.registry(),
        FileWalker::for_registry(analyzer.registry()).collect(&[root.to_path_buf()], &mut |_| {}),
    );
    graph.update(&files, &[], true, analyzer)
}

/// A megadott utak (fájlok vagy mappák; relatív út a gyökérhez képest) naprakészre hozása. A létező kódfájlok
/// (amelyeket a bejárás is felvenne) elemződnek, ha változtak; a már nem létezők — egy törölt mappa minden ismert
/// fájlja — kikerülnek. A gyökéren kívüli utak és a nem kódfájlok kimaradnak.
pub(crate) fn refresh_paths(
    graph: &mut ProjectGraph,
    root: &Path,
    paths: &[PathBuf],
    analyzer: &Analyzer,
) -> Result<Refreshed> {
    let (present, gone) = plan(graph, root, paths, analyzer);
    graph.update(&present, &gone, false, analyzer)
}

/// A frissítendő fájlok (bejárási alakban) és a törlendő kulcsok.
fn plan(
    graph: &ProjectGraph,
    root: &Path,
    paths: &[PathBuf],
    analyzer: &Analyzer,
) -> (Vec<PathBuf>, Vec<String>) {
    let walker = FileWalker::for_registry(analyzer.registry());
    let mut present: Vec<PathBuf> = Vec::new();
    let mut gone: Vec<String> = Vec::new();
    for path in paths {
        let Some(abs) = within_root(root, path) else {
            continue;
        };
        if abs.is_dir() {
            if !walker.includes_dir(root, &abs) {
                continue;
            }
            let found: Vec<PathBuf> = walker
                .collect(std::slice::from_ref(&abs), &mut |_| {})
                .into_iter()
                .filter(|file| walker.includes(root, file))
                .collect();
            let keys: HashSet<String> = found
                .iter()
                .map(|file| file.to_string_lossy().into_owned())
                .collect();
            gone.extend(
                graph
                    .tracked_under(&abs)
                    .into_iter()
                    .filter(|key| !keys.contains(key)),
            );
            present.extend(found);
        } else if abs.is_file() {
            if walker.includes(root, &abs) {
                present.push(abs);
            }
        } else {
            gone.extend(graph.tracked_under(&abs));
        }
    }
    let mut seen = HashSet::new();
    present.retain(|file| seen.insert(file.clone()));
    gone.sort();
    gone.dedup();
    (present, gone)
}

/// Az út a (kanonikus) gyökér alatt, a bejárás alakjában (kanonikus, symlinkek feloldva); egy nem létező út
/// (törölt fájl / mappa) a legközelebbi létező szülője kanonikus alakjából. `None`, ha a gyökéren kívül esik.
pub(crate) fn within_root(root: &Path, path: &Path) -> Option<PathBuf> {
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    let resolved = canonical_or_parent(&joined)?;
    resolved.starts_with(root).then_some(resolved)
}

/// A kanonikus út; ha nem létezik, a legközelebbi létező őse kanonikusan + a maradék (`..` a maradékban: `None`).
fn canonical_or_parent(path: &Path) -> Option<PathBuf> {
    if let Ok(real) = path.canonicalize() {
        return Some(real);
    }
    for ancestor in path.ancestors().skip(1) {
        let Ok(base) = ancestor.canonicalize() else {
            continue;
        };
        let rest = path.strip_prefix(ancestor).ok()?;
        if rest
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        {
            return None;
        }
        return Some(base.join(rest));
    }
    None
}
