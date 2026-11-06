//! `callgraph note add|list|rm` (PILOT): jegyzetek fájlhoz / metódushoz, a forráson kívül
//! (`<projekt>/.callgraph/notes.jsonl`, lásd `crate::notes` és `docs/NOTES.md`).
//!
//! A projekt-gyökér, mint a `show`-nál: `--project <alias|út>` (előbb a `projects.json` aliasa, utána mappa-út),
//! különben a munkakönyvtár. A jegyzet útja ehhez a gyökérhez relatív.

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::cli::{Command, NoteAction};
use crate::error::{CallgraphError, Result};
use crate::notes::{NoteFilter, NoteStore, list_line, orphaned, relative_path};
use crate::query::{QueryEngine, Selector, SelectorSet};
use crate::skill::SkillHome;

/// A jegyzet-parancs; a registry (alias-feloldás) a `SkillHome`-ból jön.
pub struct NoteCommand {
    action: NoteAction,
    project: Option<String>,
    home: SkillHome,
}

impl NoteCommand {
    pub fn new(action: NoteAction, project: Option<String>, home: SkillHome) -> Self {
        Self {
            action,
            project,
            home,
        }
    }

    /// A projekt-gyökér (kanonikus): alias › mappa-út › munkakönyvtár.
    fn root(&self) -> Result<PathBuf> {
        super::project_root(&self.home, self.project.as_deref())
    }

    fn execute(&self, out: &mut dyn Write) -> Result<()> {
        let root = self.root()?;
        match &self.action {
            NoteAction::Add { target, text } => Self::add(&root, target, text, out),
            NoteAction::List { path } => Self::list(&root, path.as_deref(), out),
            NoteAction::Rm { id } => Self::remove(&root, id, out),
        }
    }

    /// `note add`: a horgony ellenőrzése (a fájl létezik, a metódus név szerint megvan), majd a mentés; kiírja
    /// az új azonosítót és a horgonyt.
    fn add(root: &Path, target: &str, text: &str, out: &mut dyn Write) -> Result<()> {
        let (file, method) = match target.split_once("::") {
            Some((file, method)) => (file, Some(method)),
            None => (target, None),
        };
        if method.is_some_and(str::is_empty) {
            return Err(CallgraphError::Usage(
                crate::t!("empty method selector", "üres metódus-szelektor").into(),
            ));
        }
        let rel = relative_path(root, Path::new(file)).filter(|rel| !rel.is_empty());
        let Some(rel) = rel.filter(|rel| root.join(rel).exists()) else {
            return Err(CallgraphError::Usage(crate::tf!(
                "Not found: {file} (relative to the project root {})",
                "Nem található: {file} (a(z) {} projekt-gyökérhez képest)",
                root.display()
            )));
        };
        let (container, method) = match method {
            None => (None, None),
            Some(method) => {
                let (container, name) = resolve_method(root, &rel, method)?;
                (container, Some(name))
            }
        };
        let note = NoteStore::at_root(root).add(
            &rel,
            container,
            method,
            text,
            &crate::skill::clock::now_iso(),
        )?;
        writeln!(out, "{}  {}", note.id, note.anchor())?;
        Ok(())
    }

    /// `note list [<út>]`: `<id>  <horgony>  <idő>  📝 <szöveg>`, a már nem létező horgonyúak `[orphaned]` jellel.
    fn list(root: &Path, path: Option<&str>, out: &mut dyn Write) -> Result<()> {
        let scope: Vec<PathBuf> = path.map(PathBuf::from).into_iter().collect();
        let filter = NoteFilter::new(root, &scope, None)?;
        let notes: Vec<_> = NoteStore::at_root(root)
            .list()?
            .into_iter()
            .filter(|n| filter.matches(n))
            .collect();
        if notes.is_empty() {
            writeln!(out, "{}", crate::t!("(no notes)", "(nincs jegyzet)"))?;
            return Ok(());
        }
        let lang = crate::i18n::lang();
        for (note, orphan) in notes.iter().zip(orphaned(root, &notes)) {
            writeln!(out, "{}", list_line(note, None, orphan, lang))?;
        }
        Ok(())
    }

    /// `note rm <id>`.
    fn remove(root: &Path, id: &str, out: &mut dyn Write) -> Result<()> {
        match NoteStore::at_root(root).remove(id)? {
            Some(note) => {
                writeln!(
                    out,
                    "{}",
                    crate::tf!("removed: {id}  {}", "törölve: {id}  {}", note.anchor())
                )?;
                Ok(())
            }
            None => Err(CallgraphError::Usage(crate::tf!(
                "No such note: {id} (see: callgraph note list)",
                "Nincs ilyen jegyzet: {id} (lásd: callgraph note list)"
            ))),
        }
    }
}

/// `metódus` vagy `Konténer::metódus` → (konténer, a gráfbeli teljes név). A `show` szelektor-illesztését
/// használja (pl. `store.ts::fetch` → `store.fetch`); konténer nélkül akkor tölti ki, ha minden találaté azonos.
fn resolve_method(root: &Path, rel: &str, method: &str) -> Result<(Option<String>, String)> {
    let (wanted, name) = match method.rsplit_once("::") {
        Some((container, name)) => (Some(container), name),
        None => (None, method),
    };
    let selectors = SelectorSet {
        items: vec![Selector {
            file: PathBuf::from(rel),
            method: Some(name.to_owned()),
        }],
        selected: true,
    };
    let rows = QueryEngine::new(Some(root), None).query(&selectors)?;
    let found: Vec<&crate::model::Method> = match rows.first() {
        Some(row) => {
            let names = row.methods.as_deref().unwrap_or_default();
            row.graph
                .methods
                .iter()
                .filter(|m| names.contains(&m.name))
                .filter(|m| wanted.is_none_or(|c| m.container.as_deref() == Some(c)))
                .collect()
        }
        None => Vec::new(),
    };
    let Some(first) = found.first() else {
        return Err(CallgraphError::Usage(crate::tf!(
            "No such method: {rel}::{method} (see --format outline for the file's full list)",
            "Nincs ilyen metódus: {rel}::{method} (lásd: --format outline a fájl teljes listájához)"
        )));
    };
    let container = match wanted {
        Some(c) => Some(c.to_owned()),
        None if found.iter().all(|m| m.container == first.container) => first.container.clone(),
        None => None,
    };
    Ok((container, first.name.clone()))
}

impl Command for NoteCommand {
    fn run(&self, out: &mut dyn Write, err: &mut dyn Write) -> Result<i32> {
        match self.execute(out) {
            Ok(()) => Ok(0),
            Err(CallgraphError::Usage(message)) => {
                writeln!(err, "{message}")?;
                Ok(1)
            }
            Err(other) => {
                writeln!(err, "{other}")?;
                Ok(1)
            }
        }
    }
}
