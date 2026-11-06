//! `locate`: kiindulópont-kereső. Az agent a feladatából kiszedett neveket és fogalmakat (függvény-, osztály-,
//! fájlnevek, domain-szavak) egy hívásban adja át; a válasz a kész projekt-térképből (forrásfájl olvasása és
//! újraelemzés nélkül) egy rövid, rangsorolt fájl-lista, fájlonként az indoklással.
//!
//! Felépítés (Facade: `Locator`): `source` (a projekt fájljainak szűkített gráfja) → `callers` (név szerinti
//! hívószám) → `matcher` (fájlonkénti egyezések) → `rank` (összesítés, rangsor) → `reasons` + `format` (kimenet).
//! A kifejezések bontása és az egyes / többes szám tűrés: `terms`. A név-index kérésenként a memóriában készül
//! (a kifejezések előkészítve, a tények egy menetben); perzisztálni ráér, ha a mérés szerint kell.

mod callers;
mod format;
mod matcher;
mod rank;
mod reasons;
mod source;
mod terms;

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};

use crate::analysis::Analyzer;
use crate::error::{CallgraphError, Result};

use callers::CallerCounts;
use format::Report;
use matcher::Matcher;
use rank::{FileMatch, rank};
use source::ProjectSource;
use terms::Term;

/// A kiindulópont-kereső egy projekt-gyökérre.
pub struct Locator {
    root: PathBuf,
    analyzer: Analyzer,
}

impl Locator {
    /// A (kanonikus alakra hozott) gyökérre.
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.canonicalize().unwrap_or_else(|_| root.to_path_buf()),
            analyzer: Analyzer::with_defaults(),
        }
    }

    /// A kifejezésekre adott válasz szövege; `all`: minden egyező fájl (alapból az első 5). Kifejezés nélkül
    /// használati hiba.
    pub fn run<S: AsRef<str>>(&self, inputs: &[S], all: bool) -> Result<String> {
        let terms = Term::parse_all(inputs);
        if terms.is_empty() {
            return Err(CallgraphError::Usage(usage().into()));
        }
        let ranked = ProjectSource::new(&self.root, &self.analyzer).with_files(|files| {
            let callers = CallerCounts::build(files.iter().map(|file| file.graph));
            let matcher = Matcher::new(&terms, &callers);
            let matched: Vec<FileMatch> = files
                .iter()
                .filter_map(|file| {
                    let rel = self.relative(file.path);
                    let hits = matcher.file_hits(&rel, file.graph, file.code);
                    FileMatch::new(file.path.to_string_lossy().into_owned(), hits)
                })
                .collect();
            // a teszt-felismerés a gyökérhez relatív úton (a gyökér útja maga ne számítson)
            rank(matched, |path| {
                rank::is_test_path(&self.relative(Path::new(path)))
            })
        })?;
        let text = Report {
            terms: &terms,
            ranked: &ranked,
            all,
        }
        .render();
        // az utak a projekt-gyökérhez relatívan, a többi lekérdezéssel azonos `root:` sorral
        let engine = crate::query::QueryEngine::new(Some(self.root.as_path()), None);
        Ok(engine.relativize(&text))
    }

    /// Az út a gyökérhez képest (a fájlnév- és út-egyezéshez).
    fn relative(&self, path: &Path) -> String {
        path.strip_prefix(&self.root)
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned()
    }
}

/// A parancs használata (a CLI és a hibaüzenet közös szövege).
pub(crate) fn usage() -> &'static str {
    crate::t!(
        "callgraph locate <term>... [--all] [--project <alias|dir>]  (names and domain terms from the task: functions, classes, files, columns, routes)",
        "callgraph locate <kifejezés>... [--all] [--project <alias|mappa>]  (a feladat nevei és fogalmai: függvények, osztályok, fájlok, oszlopok, útvonalak)"
    )
}
