//! `callgraph refresh [--project <alias|út>] [--background] [<út>…]`: a megadott fájlok / mappák (út nélkül a
//! projekt minden változott, új és törölt fájlja) naprakészre hozása a projekt-gráfban (`.callgraph/`), majd mentés
//! és egysoros összegzés. Egy git pull vagy sok fájlt író agent után így a következő lekérdezés nem elemez.
//!
//! Egyszerre egy frissítő dolgozik egy tárolón (`graph::RefreshLock`, `.callgraph/refresh.lock`): ha egy másik
//! rövid várakozás után is tartja, ez kihagyja (0-s kilépési kód). `--background`: önmagát indítja leválasztva
//! (`--background` nélkül, mint a háttér-frissítés-ellenőrzés), és azonnal visszatér.

use std::ffi::OsString;
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use crate::analysis::Analyzer;
use crate::cli::Command;
use crate::error::Result;
use crate::graph::{ProjectGraph, RefreshLock, Refreshed, refresh_all, refresh_paths};
use crate::project::ProjectStore;
use crate::skill::SkillHome;

/// Ennyit vár egy másik frissítőre, mielőtt kihagyja.
const LOCK_WAIT: Duration = Duration::from_secs(2);

/// A frissítő parancs; a `--project` aliast a `SkillHome` registryje oldja fel.
pub struct RefreshCommand {
    paths: Vec<PathBuf>,
    project: Option<String>,
    background: bool,
    home: SkillHome,
}

impl RefreshCommand {
    pub fn new(
        paths: Vec<PathBuf>,
        project: Option<String>,
        background: bool,
        home: SkillHome,
    ) -> Self {
        Self {
            paths,
            project,
            background,
            home,
        }
    }

    /// A háttér-folyamat argumentumai: a feloldott gyökér és az utak (`--` után), `--background` nélkül.
    fn background_args(&self, root: &std::path::Path) -> Vec<OsString> {
        let mut args: Vec<OsString> = vec![
            "refresh".into(),
            "--project".into(),
            root.as_os_str().to_owned(),
        ];
        if !self.paths.is_empty() {
            args.push("--".into());
            args.extend(self.paths.iter().map(|path| path.as_os_str().to_owned()));
        }
        args
    }
}

/// A frissítés a gyökérre (a hívó a zárat már tartja): a megadott utak, vagy a teljes projekt; mentés.
pub(crate) fn refresh_root(root: &std::path::Path, paths: &[PathBuf]) -> Result<Refreshed> {
    let store = ProjectStore::at_root(root);
    let analyzer = Analyzer::with_defaults();
    let mut graph = ProjectGraph::open(store.dir().to_path_buf());
    let done = if paths.is_empty() {
        refresh_all(&mut graph, root, &analyzer)?
    } else {
        refresh_paths(&mut graph, root, paths, &analyzer)?
    };
    graph.save()?;
    Ok(done)
}

impl Command for RefreshCommand {
    fn run(&self, out: &mut dyn Write, _err: &mut dyn Write) -> Result<i32> {
        let root = super::project_root(&self.home, self.project.as_deref())?;
        let shown = root.to_string_lossy();
        let shown = crate::query::strip_verbatim(&shown);
        // ha a háttér-folyamat nem indult el, helyben fut tovább
        if self.background && crate::update::check::spawn_detached(self.background_args(&root)) {
            writeln!(
                out,
                "{}",
                crate::tf!(
                    "refresh: started in the background ({})",
                    "frissítés: elindult a háttérben ({})",
                    shown
                )
            )?;
            return Ok(0);
        }
        let store = ProjectStore::at_root(&root);
        store.ensure()?;
        let Some(_lock) = RefreshLock::acquire(store.dir(), LOCK_WAIT) else {
            writeln!(
                out,
                "{}",
                crate::t!(
                    "refresh: skipped (another refresh is running)",
                    "frissítés: kihagyva (egy másik frissítés fut)"
                )
            )?;
            return Ok(0);
        };
        let done = refresh_root(&root, &self.paths)?;
        writeln!(
            out,
            "{}",
            crate::tf!(
                "refresh: {} updated, {} removed, {} unchanged ({})",
                "frissítés: {} frissítve, {} eltávolítva, {} változatlan ({})",
                done.updated,
                done.removed,
                done.unchanged,
                shown
            )
        )?;
        Ok(0)
    }
}
