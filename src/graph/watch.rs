//! Fájlfigyelő az MCP-szerverhez (`callgraph mcp`): a folyamatban betöltött projekt-gyökereket figyeli (`notify`:
//! macOS FSEvents, Linux inotify, Windows ReadDirectoryChangesW), és a változott fájlokat röviddel a változás után
//! (`DEBOUNCE` csend, legfeljebb `MAX_DELAY` késéssel) frissíti a memóriában lévő gráfban, majd ment — így a
//! következő lekérdezés nem elemez. A kimenetet nem változtatja: a lekérdezések továbbra is ellenőrzik a bélyeget;
//! ha a figyelés nem indul (korlát, nem támogatott fájlrendszer), egyszer szól a stderr-en, és minden marad a
//! lekérdezésenkénti ellenőrzésen. Kikapcsolás: `CALLGRAPH_WATCH=0|off|false`.

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use notify::event::{AccessKind, AccessMode, ModifyKind};
use notify::{Event, EventKind, RecursiveMode, Watcher};

use super::SharedGraph;
use super::refresh::{RefreshLock, refresh_paths};
use crate::analysis::Analyzer;
use crate::io::FileWalker;
use crate::project::STORE_DIR;

/// Ennyi csend után frissül a gráf (egy több fájlt író művelet így egyetlen frissítés).
pub(crate) const DEBOUNCE: Duration = Duration::from_millis(300);
/// Folyamatos írás mellett is legfeljebb ennyi késéssel frissül.
pub(crate) const MAX_DELAY: Duration = Duration::from_secs(2);
/// Ezek alatt soha nem frissít (a tároló maga, verziókezelő, build-kimenet, függőségek); a többit a bejárás szűrője
/// dönti el (`FileWalker::includes`).
const IGNORED_DIRS: [&str; 5] = [STORE_DIR, ".git", "target", "node_modules", "vendor"];
/// A mentés előtt ennyit vár a `refresh.lock`-ra; ha más tartja, csak a memóriában frissít (a következő lekérdezés ment).
const LOCK_WAIT: Duration = Duration::from_millis(500);

static ENABLED: AtomicBool = AtomicBool::new(false);
static WARNED: AtomicBool = AtomicBool::new(false);

/// A figyelés bekapcsolása a folyamatra (az MCP-szerver indításakor); a `CALLGRAPH_WATCH=0|off|false` letiltja.
pub(crate) fn enable() {
    let off = std::env::var("CALLGRAPH_WATCH").is_ok_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "0" | "off" | "false" | "no"
        )
    });
    ENABLED.store(!off, Ordering::Relaxed);
}

/// Be van-e kapcsolva a figyelés (a `graph::shared` egy új gyökérnél ennek alapján indít figyelőt).
pub(crate) fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// A gyökér figyelése egy háttérszálon (a figyelő is ott jön létre, így az indulása a lekérdezést nem lassítja).
/// `false`, ha a szál nem indult el.
pub(crate) fn start(root: PathBuf, graph: SharedGraph) -> bool {
    std::thread::Builder::new()
        .name("callgraph-watch".to_owned())
        .spawn(move || watch(&root, &graph))
        .is_ok()
}

fn watch(root: &Path, graph: &SharedGraph) {
    let (tx, rx) = mpsc::channel::<notify::Result<Event>>();
    let mut watcher = match notify::recommended_watcher(tx) {
        Ok(watcher) => watcher,
        Err(error) => return warn_once(root, &error),
    };
    if let Err(error) = watcher.watch(root, RecursiveMode::Recursive) {
        return warn_once(root, &error);
    }
    run(root, graph, &rx);
}

/// Egyszeri figyelmeztetés a stderr-en (az MCP-ben a stdout a protokollé).
fn warn_once(root: &Path, error: &notify::Error) {
    if !WARNED.swap(true, Ordering::Relaxed) {
        eprintln!(
            "{}",
            crate::tf!(
                "callgraph: file watching is unavailable ({}: {error}); queries check changed files themselves",
                "callgraph: a fájlfigyelés nem érhető el ({}: {error}); a lekérdezések maguk nézik a változott fájlokat",
                root.display()
            )
        );
    }
}

/// Az események gyűjtése és a csend utáni frissítés, amíg a figyelő él.
fn run(root: &Path, graph: &SharedGraph, rx: &Receiver<notify::Result<Event>>) {
    let analyzer = Analyzer::with_defaults();
    let walker = FileWalker::for_registry(analyzer.registry());
    let mut pending = Debouncer::new(DEBOUNCE, MAX_DELAY);
    loop {
        let received = match pending.wait(Instant::now()) {
            None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
            Some(wait) => rx.recv_timeout(wait),
        };
        match received {
            Ok(Ok(event)) => pending.push(changed_paths(root, &event, &walker), Instant::now()),
            // egy figyelő-hiba nem állítja meg a figyelést (a lekérdezések úgyis ellenőriznek)
            Ok(Err(_)) | Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        if let Some(batch) = pending.take(Instant::now()) {
            apply(root, graph, &batch, &analyzer);
        }
    }
}

/// Egy összegyűlt adag frissítése a gráfban; mentés, ha a `refresh.lock` megszerezhető (különben a következő
/// lekérdezés ment). A hiba (pl. egy épp írt, még nem olvasható fájl) nem baj: a lekérdezés újra megnézi.
pub(crate) fn apply(root: &Path, graph: &SharedGraph, batch: &[PathBuf], analyzer: &Analyzer) {
    let guard = RefreshLock::acquire(&root.join(STORE_DIR), LOCK_WAIT);
    let mut graph = super::lock(graph);
    if refresh_paths(&mut graph, root, batch, analyzer).is_ok() && guard.is_some() {
        let _ = graph.save();
    }
}

/// Egy esemény frissítendő útjai: a hozzáférés-események (olvasás) kimaradnak, az írás utáni bezárás nem; út nélküli
/// esemény (pl. túlcsordult eseménysor) → a teljes gyökér.
pub(crate) fn changed_paths(root: &Path, event: &Event, walker: &FileWalker) -> Vec<PathBuf> {
    match event.kind {
        EventKind::Access(AccessKind::Close(AccessMode::Write)) => {}
        EventKind::Access(_) => return Vec::new(),
        _ => {}
    }
    if event.paths.is_empty() {
        return vec![root.to_path_buf()];
    }
    // mappa csak létrehozáskor / törléskor / átnevezéskor számít (Windowson egy mappa „módosul”, ha benne fájl változik)
    let structural = matches!(
        event.kind,
        EventKind::Create(_) | EventKind::Remove(_) | EventKind::Modify(ModifyKind::Name(_))
    );
    event
        .paths
        .iter()
        .filter(|path| relevant(root, path, structural, walker))
        .cloned()
        .collect()
}

/// A gyökér alatti, nem kihagyott út: egy kódfájl, vagy (`structural` eseménynél) egy mappa — létrehozott, törölt,
/// átnevezett: a frissítés bejárja / a benne ismert fájlokat eltávolítja. Egy nem létező, nem kód-kiterjesztésű út
/// egy törölt mappa is lehet, ezért az is marad.
pub(crate) fn relevant(root: &Path, path: &Path, structural: bool, walker: &FileWalker) -> bool {
    let Ok(rel) = path.strip_prefix(root) else {
        return false;
    };
    let ignored = rel.components().any(|component| match component {
        Component::Normal(name) => IGNORED_DIRS.iter().any(|dir| name == *dir),
        _ => true,
    });
    if ignored {
        return false;
    }
    if path.is_dir() {
        structural && walker.includes_dir(root, path)
    } else if path.exists() {
        walker.includes(root, path)
    } else {
        walker.includes(root, path) || (structural && walker.includes_dir(root, path))
    }
}

/// Az események összegyűjtése: az utolsó esemény után `quiet` csenddel, de az első után legfeljebb `max` késéssel
/// adja ki az adagot (determinisztikus, a hívó adja az időt — tesztelhető).
#[derive(Debug)]
pub(crate) struct Debouncer {
    quiet: Duration,
    max: Duration,
    paths: BTreeSet<PathBuf>,
    first: Option<Instant>,
    last: Option<Instant>,
}

impl Debouncer {
    pub(crate) fn new(quiet: Duration, max: Duration) -> Self {
        Self {
            quiet,
            max,
            paths: BTreeSet::new(),
            first: None,
            last: None,
        }
    }

    /// Új utak; üres listánál semmi sem változik (az időzítés sem).
    pub(crate) fn push(&mut self, paths: Vec<PathBuf>, now: Instant) {
        if paths.is_empty() {
            return;
        }
        self.paths.extend(paths);
        self.first.get_or_insert(now);
        self.last = Some(now);
    }

    /// Mikor esedékes a következő adag (`None`: nincs függőben semmi).
    fn due(&self) -> Option<Instant> {
        let (first, last) = (self.first?, self.last?);
        Some((last + self.quiet).min(first + self.max))
    }

    /// Mennyit várjon a hívó a következő eseményre (`None`: korlát nélkül).
    pub(crate) fn wait(&self, now: Instant) -> Option<Duration> {
        self.due().map(|due| due.saturating_duration_since(now))
    }

    /// Az esedékes adag (rendezve, ismétlés nélkül), vagy `None`, ha még nem esedékes / üres.
    pub(crate) fn take(&mut self, now: Instant) -> Option<Vec<PathBuf>> {
        if self.due()? > now {
            return None;
        }
        self.first = None;
        self.last = None;
        Some(std::mem::take(&mut self.paths).into_iter().collect())
    }
}
