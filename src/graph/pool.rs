//! A folyamatban betöltött projekt-gráfok (gyökerenként egy): az első lekérdezés a lemezről tölti be, a többi a
//! memóriában lévőt frissíti. A CLI-ben ez egy hívás; az MCP-szerver (`callgraph mcp`) egy hosszan futó folyamat,
//! ott a gráf és a hívó-index kérésről kérésre megmarad, és kérésenként csak a változott fájlok frissülnek.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};

use super::ProjectGraph;
use crate::project::ProjectStore;

/// Egy projekt közösen használt gráfja.
pub(crate) type SharedGraph = Arc<Mutex<ProjectGraph>>;

static POOL: OnceLock<Mutex<HashMap<PathBuf, SharedGraph>>> = OnceLock::new();

/// A gyökér projekt-gráfja (a `.callgraph/` tárolót létrehozza). `None`, ha a tároló nem hozható létre (pl.
/// írásvédett projekt) — ilyenkor a lekérdezés a gráf nélkül, fájlonként fut.
pub(crate) fn shared(root: &Path) -> Option<SharedGraph> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let store = ProjectStore::at_root(&root);
    store.ensure().ok()?;
    let pool = POOL.get_or_init(Default::default);
    let mut pool = pool.lock().unwrap_or_else(PoisonError::into_inner);
    let graph = pool
        .entry(root)
        .or_insert_with(|| Arc::new(Mutex::new(ProjectGraph::open(store.dir().to_path_buf()))));
    Some(Arc::clone(graph))
}

/// A gráf zárolása; egy korábbi, pánikkal megszakadt kérés után is használható (a gráf minden kérésnél frissül).
pub(crate) fn lock(graph: &SharedGraph) -> MutexGuard<'_, ProjectGraph> {
    graph.lock().unwrap_or_else(PoisonError::into_inner)
}
