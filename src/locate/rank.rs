//! A fájlonként összesített egyezések rangsora. Kifejezésenként a legerősebb egyezés fajtája számít (a pontos
//! definíció messze a legtöbbet ér, a név-részlet és a string-literál jóval kevesebbet), a kifejezés ritkasága
//! szerint súlyozva (a sok fájlban előforduló, általános kifejezés kevesebbet ér — IDF), és a tesztfájlok kisebb
//! súllyal (az agent jellemzően a megvalósítást keresi; a teszt a listában marad, csak hátrébb). Egy kifejezés sok
//! találata csak kis, korlátos előnyt ad. Holtversenyben az út dönt (stabil).

use std::collections::{HashMap, HashSet};

use super::matcher::{Hit, Kind, Symbol};

/// Az ismétlés és a hívószám előnyének felső korlátja (a fajták súlykülönbsége alatt marad).
const BONUS_CAP: u64 = 9;
/// A tesztfájl pontszámának szorzója (ezrelékben).
const TEST_FACTOR_PERMILLE: u64 = 600;

/// Egy fájl a rangsorban: az út (a kimenet alakjában), az egyezések és a pontszám.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FileMatch {
    pub path: String,
    pub hits: Vec<Hit>,
    pub score: u64,
}

impl FileMatch {
    /// Az alap-pontszámmal (a ritkaság nélkül; a `rank` számolja újra); üres egyezés-listára `None`.
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

/// Az egyezés-fajta súlya: a pontos definíció és a fájlnév messze erősebb, mint a név-részlet, a string és a hívás
/// (így egy pontos osztálynév nem marad le a több kifejezésre csak név-részlettel illő fájlok mögött).
fn weight(kind: Kind) -> u64 {
    match kind {
        Kind::Definition => 1000,
        Kind::FileName => 800,
        Kind::NamePart => 300,
        Kind::StringLit => 300,
        Kind::Call => 200,
    }
}

/// Kifejezésenként a legerősebb egyezés fajtája.
fn best_kinds(hits: &[Hit]) -> HashMap<usize, Kind> {
    let mut best: HashMap<usize, Kind> = HashMap::new();
    for hit in hits {
        best.entry(hit.term)
            .and_modify(|kind| *kind = (*kind).min(hit.kind))
            .or_insert(hit.kind);
    }
    best
}

/// A korlátos előny: az ismétlés és a definíció hívói (a legtöbbet hívott egyező definícióé).
fn bonus(hits: &[Hit], distinct: usize) -> u64 {
    let repeats = (hits.len() as u64)
        .saturating_sub(distinct as u64)
        .min(BONUS_CAP);
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
    repeats + callers
}

/// Az alap-pontszám (ritkaság nélkül): kifejezésenként a legerősebb fajta súlya + korlátos előny az ismétlésért és
/// a definíció hívóiért.
pub(crate) fn score(hits: &[Hit]) -> u64 {
    let best = best_kinds(hits);
    best.values().map(|kind| weight(*kind)).sum::<u64>() + bonus(hits, best.len())
}

/// Tesztfájl-e az út (`tests/`, `__tests__/`, `*.spec.*`, `*.test.*`, `*Test.php`, `*_test.rs`…).
pub(crate) fn is_test_path(path: &str) -> bool {
    let lower = path.replace('\\', "/").to_ascii_lowercase();
    let name = lower.rsplit('/').next().unwrap_or(&lower);
    lower.starts_with("tests/")
        || lower.starts_with("test/")
        || ["/tests/", "/test/", "/__tests__/", "/spec/"]
            .iter()
            .any(|dir| lower.contains(dir))
        || name.contains(".spec.")
        || name.contains(".test.")
        || name.ends_with("test.php")
        || name.ends_with("_test.rs")
        || name.ends_with("tests.swift")
}

/// Rangsor: a pontszám a kifejezések ritkaságával súlyozva (IDF: `1 + ln(fájlok / a kifejezésre illő fájlok)`,
/// ezrelékben), tesztfájlnál (`is_test`: a gyökérhez relatív úton dönt, pl. `is_test_path`) kisebb szorzóval;
/// csökkenő pontszám, holtversenyben út szerint növekvő sorrend.
pub(crate) fn rank(mut files: Vec<FileMatch>, is_test: impl Fn(&str) -> bool) -> Vec<FileMatch> {
    let total = files.len().max(1) as f64;
    let mut df: HashMap<usize, usize> = HashMap::new();
    for file in &files {
        let terms: HashSet<usize> = file.hits.iter().map(|hit| hit.term).collect();
        for term in terms {
            *df.entry(term).or_default() += 1;
        }
    }
    let idf_permille = |term: usize| -> u64 {
        let matched = df.get(&term).copied().unwrap_or(1).max(1) as f64;
        ((1.0 + (total / matched).ln()) * 1000.0).round() as u64
    };
    for file in &mut files {
        let best = best_kinds(&file.hits);
        let weighted: u64 = best
            .iter()
            .map(|(term, kind)| weight(*kind) * idf_permille(*term) / 1000)
            .sum();
        let mut score = weighted + bonus(&file.hits, best.len());
        if is_test(&file.path) {
            score = score * TEST_FACTOR_PERMILLE / 1000;
        }
        file.score = score;
    }
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
