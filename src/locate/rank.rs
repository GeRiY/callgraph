//! A fájlonként összesített egyezések rangsora. Elsődleges a fájlban egyező KÜLÖNBÖZŐ kifejezések száma (egy
//! kifejezés sok találata ennél sokkal kevesebbet ér); azon belül kifejezésenként a legerősebb egyezés fajtája; végül
//! kis előny az ismétlésnek és a sok helyről hívott definíciónak. Holtversenyben az út dönt (stabil).

use std::collections::HashMap;

use super::matcher::{Hit, Kind, Symbol};

/// A különböző kifejezések szorzója: nagyobb, mint a fajták súlyának bármilyen összege.
const PER_TERM: u64 = 100_000;
/// Az ismétlés és a hívószám előnyének felső korlátja (a fajták súlykülönbsége alatt marad).
const BONUS_CAP: u64 = 9;

/// Egy fájl a rangsorban: az út (a kimenet alakjában), az egyezések és a pontszám.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FileMatch {
    pub path: String,
    pub hits: Vec<Hit>,
    pub score: u64,
}

impl FileMatch {
    /// Pontszámmal; üres egyezés-listára `None`.
    pub(crate) fn new(path: String, hits: Vec<Hit>) -> Option<Self> {
        if hits.is_empty() {
            return None;
        }
        let score = score(&hits);
        Some(Self { path, hits, score })
    }

    /// A fájlban egyező kifejezések (index), növekvő sorrendben, ismétlés nélkül.
    pub(crate) fn terms(&self) -> Vec<usize> {
        let mut terms: Vec<usize> = self.hits.iter().map(|hit| hit.term).collect();
        terms.sort_unstable();
        terms.dedup();
        terms
    }
}

/// A pontszám: különböző kifejezések × `PER_TERM` + kifejezésenként a legerősebb fajta súlya + korlátos előny az
/// ismétlésért és a definíció hívóiért.
pub(crate) fn score(hits: &[Hit]) -> u64 {
    let mut best: HashMap<usize, Kind> = HashMap::new();
    for hit in hits {
        best.entry(hit.term)
            .and_modify(|kind| *kind = (*kind).min(hit.kind))
            .or_insert(hit.kind);
    }
    let distinct = best.len() as u64;
    let kinds: u64 = best.values().map(|kind| kind.weight()).sum();
    let repeats = (hits.len() as u64).saturating_sub(distinct).min(BONUS_CAP);
    let callers = hits
        .iter()
        .filter(|hit| hit.kind == Kind::Definition)
        .filter_map(|hit| match &hit.symbol {
            Symbol::Method { callers, .. } => Some(callers.count as u64),
            _ => None,
        })
        .max()
        .unwrap_or(0)
        .min(BONUS_CAP);
    distinct * PER_TERM + kinds + repeats + callers
}

/// Rangsor: pontszám szerint csökkenő, holtversenyben út szerint növekvő sorrend.
pub(crate) fn rank(mut files: Vec<FileMatch>) -> Vec<FileMatch> {
    files.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.path.cmp(&b.path)));
    files
}

/// Csillagok a kiíráshoz, a legjobb fájlhoz mért pontszám szerint (★★★ / ★★ / ★).
pub(crate) fn stars(score: u64, top: u64) -> &'static str {
    let top = top.max(1);
    if score * 4 >= top * 3 {
        "★★★"
    } else if score * 5 >= top * 2 {
        "★★"
    } else {
        "★"
    }
}
