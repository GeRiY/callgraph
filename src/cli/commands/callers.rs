//! A projekt-indexből fordított hívókat kiíró Command.

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::analysis::Analyzer;
use crate::cli::Command;
use crate::error::Result;
use crate::graph::ProjectGraph;
use crate::io::FileWalker;
use crate::project::{Project, ProjectLocator, ProjectStore};

/// Szintaktikai hibás fájl: gyökérhez relatív út és az első hiba sora.
type BrokenFile = (String, u32);

/// A hívók, a hibás fájlok és (`--by-layer`) a rétegek osztályozója.
type Callers = (
    Vec<crate::index::CallerRow>,
    Vec<BrokenFile>,
    Option<crate::profile::LayerScope>,
);

/// A projekten belüli, minta szerinti hívókat JSONL-ként írja ki.
pub struct CallersCommand {
    pattern: String,
    depth: usize,
    project: Option<PathBuf>,
    /// `--by-layer`: szintenként rétegek szerint rendezve (a hívotthoz legközelebbi réteg elöl), `layer` mezővel.
    by_layer: bool,
}

/// Egy sor a `--by-layer` kimenetben: a szokásos mezők és a hívó fájl szerepe (ismeretlennél nincs `layer`).
#[derive(serde::Serialize)]
struct LayeredRow<'a> {
    #[serde(flatten)]
    row: &'a crate::index::CallerRow,
    #[serde(skip_serializing_if = "Option::is_none")]
    layer: Option<&'static str>,
}

impl CallersCommand {
    pub fn new(pattern: String, depth: usize, project: Option<PathBuf>) -> Self {
        Self {
            pattern,
            depth,
            project,
            by_layer: false,
        }
    }

    /// A `--by-layer` kapcsoló (alapból ki: a kimenet változatlan).
    pub fn by_layer(mut self, on: bool) -> Self {
        self.by_layer = on;
        self
    }

    /// A projekt-gyökér ugyanúgy, mint az elemzésnél (ProjectLocator): a megadott útról vagy a munkakönyvtárról
    /// felfelé a legközelebbi composer.json / package.json / .git — így a cache és az index egy helyen van.
    fn root(&self) -> Project {
        let start = self.project.as_deref().unwrap_or_else(|| Path::new("."));
        ProjectLocator::default().locate(start)
    }

    /// A projekt-gráfból (`.callgraph/`, a folyamatban közös példány) a minta hívói; előtte a gráf
    /// naprakészre hozva: csak a változott fájlok elemződnek újra, a törölt / átnevezett fájlok kikerülnek.
    /// Mellé a szintaktikai hibás fájlok (gyökérhez relatív út, az első hiba sora) a figyelmeztetéshez, és
    /// `--by-layer`-nél a rétegek a profillal azonos besorolással (a frissített gráfból).
    fn callers(&self, project: &Project) -> Result<Callers> {
        let store = ProjectStore::new(project);
        store.ensure()?;
        let analyzer = Analyzer::with_defaults();
        let files = crate::graph::legacy_order(
            analyzer.registry(),
            FileWalker::for_registry(analyzer.registry())
                .collect(&[project.root().to_path_buf()], &mut |_| {}),
        );
        let shared = crate::graph::shared(project.root());
        let mut guard = shared.as_ref().map(crate::graph::lock);
        let mut owned = None;
        let graph: &mut ProjectGraph = match guard.as_deref_mut() {
            Some(graph) => graph,
            None => owned.insert(ProjectGraph::open(store.dir().to_path_buf())),
        };
        graph.refresh(&files, true, &analyzer)?;
        graph.ensure_callers(&analyzer)?;
        graph.save()?;
        let issues = files
            .iter()
            .filter_map(|file| {
                let stored = graph.slim_graph(&file.to_string_lossy())?;
                crate::query::format::parse_issue(project.root(), file, stored)
            })
            .collect();
        let scope = if self.by_layer {
            let mut scope = crate::profile::LayerScope::new(project.root(), None, true)
                .map_err(crate::error::CallgraphError::Usage)?;
            scope.settle_with(&files, graph);
            Some(scope)
        } else {
            None
        };
        Ok((
            graph.caller_index().callers(&self.pattern, self.depth)?,
            issues,
            scope,
        ))
    }
}

impl Command for CallersCommand {
    fn run(&self, out: &mut dyn Write, _err: &mut dyn Write) -> Result<i32> {
        let project = self.root();
        let (mut rows, issues, scope) = self.callers(&project)?;
        // a JSONL-kimenet gépi: a szintaktikai hibás fájlok figyelmeztetése a hibakimenetre megy (kilépési kód: 0)
        let warning = crate::query::format::parse_trailer(&issues, crate::i18n::lang());
        _err.write_all(warning.as_bytes())?;
        let Some(scope) = scope else {
            for row in rows {
                serde_json::to_writer(&mut *out, &row)?;
                writeln!(out)?;
            }
            return Ok(0);
        };
        // stabil rendezés: szinten belül a réteg, azon belül az eredeti sorrend
        rows.sort_by_key(|row| (row.level, scope.order_key(&row.file)));
        for row in &rows {
            let layered = LayeredRow {
                row,
                layer: scope.role_of_key(&row.file).map(|role| role.name()),
            };
            serde_json::to_writer(&mut *out, &layered)?;
            writeln!(out)?;
        }
        Ok(0)
    }
}
