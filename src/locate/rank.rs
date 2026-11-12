//! A fájlonként összesített egyezések rangsora. Kifejezésenként a legerősebb egyezés fajtája számít (a pontos
//! definíció messze a legtöbbet ér, a név-részlet és a string-literál jóval kevesebbet), a kifejezés ritkasága
//! szerint súlyozva (a sok fájlban előforduló, általános kifejezés kevesebbet ér — IDF), és a tesztfájlok kisebb
//! súllyal (az agent jellemzően a megvalósítást keresi; a teszt a listában marad, csak hátrébb). Egy kifejezés sok
//! találata csak kis, korlátos előnyt ad. A rangsor két szintű: elöl a pontos egyezésű fájlok (egy definíció neve
//! vagy a fájlnév maga egy kifejezés — több kifejezésnél se előzze meg egy pontos komponenst a sok kifejezésre csak
//! stringgel illő fordítási / build-fájl), utánuk a többi; szinten belül a pontszám. Holtversenyben az út dönt
//! (stabil).
//!
//! Erős jel a több kifejezést egyesítő azonosító is (`toggleFavorite` a `toggle` + `favorite` kifejezésekre,
//! `PodcastEpisodeController` a `podcast` + `episode`-ra): a fájl a pontos szintre kerül, és a kifejezések számával
//! növő, a ritkaságtól független pontot kap — így akkor is elöl van, ha a kifejezések külön-külön túl általánosak.
//! A típusdeklarációs fájl (`.d.ts`, `.pyi`) definíció- és többkifejezéses egyezése kevesebbet ér egy valódi
//! definíciónál, és nem emeli a fájlt a pontos szintre (a sok felsorolt név miatt ne kerüljön a lista elejére).
//!
//! A property / mező pontos neve definíció-súlyú, de a pontos szintre csak akkor emel, ha legfeljebb
//! `PROPERTY_EXACT_MAX_FILES` fájlban definiált: a sok modellben ismétlődő mező (`$fillable`, `status`) ne kerüljön a
//! pontos osztály- / függvény-találatok elé (a pontszámban a ritkaság-súlyozás úgyis kevesebbet ad neki).

use std::collections::{HashMap, HashSet};

use super::matcher::{Hit, Kind, Symbol};

/// Az ismétlés és a hívószám előnyének felső korlátja (a fajták súlykülönbsége alatt marad).
const BONUS_CAP: u64 = 9;
/// A tesztfájl pontszámának szorzója (ezrelékben).
const TEST_FACTOR_PERMILLE: u64 = 600;
/// A több kifejezést egyesítő azonosító pontja kifejezésenként (a ritkaságtól független: a kifejezések együttese
/// akkor is ritka, ha külön-külön általánosak).
const MULTI_TERM_WEIGHT: u64 = 1500;
/// A típusdeklarációs fájl definíció-egyezésének súlya (a valódi definíció 1000-e helyett).
const DECLARATION_DEFINITION_WEIGHT: u64 = 400;
/// Egy kifejezés pontos property-definíciója legfeljebb ennyi fájlban emeli a fájlt a pontos szintre.
const PROPERTY_EXACT_MAX_FILES: usize = 3;

/// Egy fájl a rangsorban: az út (a kimenet alakjában), az egyezések, a pontszám, és hogy a fájl egy azonosítója
/// legfeljebb hány (független) kifejezést egyesít.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FileMatch {
    pub path: String,
    pub hits: Vec<Hit>,
    pub score: u64,
    pub multi: usize,
}

impl FileMatch {
    /// Az alap-pontszámmal (a ritkaság nélkül; a `rank` számolja újra); üres egyezés-listára `None`.
    pub(crate) fn new(path: String, hits: Vec<Hit>) -> Option<Self> {
        if hits.is_empty() {
            return None;
        }
        let score = score(&hits);
        Some(Self {
            path,
            hits,
            score,
            multi: 0,
        })
    }

    /// A legtöbb kifejezést egyesítő azonosító kifejezés-száma (`Matcher::multi_terms`).
    pub(crate) fn with_multi(mut self, multi: usize) -> Self {
        self.multi = multi;
        self
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

/// A fajta súlya a fájlban: a típusdeklarációs fájl definíciója kevesebbet ér.
fn file_weight(kind: Kind, declaration: bool) -> u64 {
    if declaration && kind == Kind::Definition {
        DECLARATION_DEFINITION_WEIGHT
    } else {
        weight(kind)
    }
}

/// A több kifejezést egyesítő azonosító pontja: legalább két kifejezéstől, a kifejezések számával nő.
fn multi_bonus(multi: usize) -> u64 {
    if multi >= 2 {
        MULTI_TERM_WEIGHT * multi as u64
    } else {
        0
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
        || (name.starts_with("test_") && name.ends_with(".py"))
        || name.ends_with("_test.py")
        || name == "conftest.py"
        || name.ends_with("_test.go")
}

/// Csak típusokat tartalmazó deklarációs fájl-e az út (`*.d.ts`, `*.d.mts`, `*.d.cts`, `*.pyi`).
pub(crate) fn is_declaration_path(path: &str) -> bool {
    let lower = path.replace('\\', "/").to_ascii_lowercase();
    let name = lower.rsplit('/').next().unwrap_or(&lower);
    [".d.ts", ".d.mts", ".d.cts", ".pyi"]
        .iter()
        .any(|ext| name.ends_with(ext))
}

/// Rangsor: a pontszám a kifejezések ritkaságával súlyozva (IDF: `1 + ln(fájlok / a kifejezésre illő fájlok)`,
/// ezrelékben), tesztfájlnál (`is_test`: a gyökérhez relatív úton dönt, pl. `is_test_path`) kisebb szorzóval;
/// elöl a pontos egyezésű fájlok (`exact`), szinten belül csökkenő pontszám, holtversenyben út szerint növekvő
/// sorrend.
pub(crate) fn rank(mut files: Vec<FileMatch>, is_test: impl Fn(&str) -> bool) -> Vec<FileMatch> {
    let total = files.len().max(1) as f64;
    let mut df: HashMap<usize, usize> = HashMap::new();
    // kifejezésenként: hány fájl definiálja pontos névvel property-ként
    let mut property_df: HashMap<usize, usize> = HashMap::new();
    for file in &files {
        let terms: HashSet<usize> = file.hits.iter().map(|hit| hit.term).collect();
        for term in terms {
            *df.entry(term).or_default() += 1;
        }
        let properties: HashSet<usize> = file
            .hits
            .iter()
            .filter(|hit| exact_property(hit))
            .map(|hit| hit.term)
            .collect();
        for term in properties {
            *property_df.entry(term).or_default() += 1;
        }
    }
    let rare_property =
        |term: usize| property_df.get(&term).copied().unwrap_or(0) <= PROPERTY_EXACT_MAX_FILES;
    let idf_permille = |term: usize| -> u64 {
        let matched = df.get(&term).copied().unwrap_or(1).max(1) as f64;
        ((1.0 + (total / matched).ln()) * 1000.0).round() as u64
    };
    for file in &mut files {
        let best = best_kinds(&file.hits);
        let declaration = is_declaration_path(&file.path);
        let weighted: u64 = best
            .iter()
            .map(|(term, kind)| file_weight(*kind, declaration) * idf_permille(*term) / 1000)
            .sum();
        // a deklarációs fájlban a többkifejezéses név is a kisebb definíció-súllyal számít
        let multi = multi_bonus(file.multi) * file_weight(Kind::Definition, declaration)
            / weight(Kind::Definition);
        let mut score = weighted + multi + bonus(&file.hits, best.len());
        if is_test(&file.path) {
            score = score * TEST_FACTOR_PERMILLE / 1000;
        }
        file.score = score;
    }
    files.sort_by(|a, b| {
        exact(b, &rare_property)
            .cmp(&exact(a, &rare_property))
            .then_with(|| b.score.cmp(&a.score))
            .then_with(|| a.path.cmp(&b.path))
    });
    files
}

/// A pontos szinten van-e a fájl: egy definíció neve (osztály, komponens, függvény, konstans, útvonal-definíció,
/// ritka property) vagy a fájl neve / törzse maga egy kifejezés, vagy egy azonosítója legalább két kifejezést
/// egyesít. A típusdeklarációs fájl definíciója és többkifejezéses neve ehhez nem elég (a fájlneve igen), ahogy a
/// sok fájlban definiált property sem (`rare_property`: a kifejezés property-definíciója ritka-e).
fn exact(file: &FileMatch, rare_property: &dyn Fn(usize) -> bool) -> bool {
    let declaration = is_declaration_path(&file.path);
    (file.multi >= 2 && !declaration)
        || file.hits.iter().any(|hit| match hit.kind {
            Kind::FileName => true,
            Kind::Definition if exact_property(hit) => !declaration && rare_property(hit.term),
            Kind::Definition => !declaration,
            _ => false,
        })
}

/// Pontos property-definíció-e az egyezés.
fn exact_property(hit: &Hit) -> bool {
    hit.kind == Kind::Definition && matches!(hit.symbol, Symbol::Property { .. })
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
