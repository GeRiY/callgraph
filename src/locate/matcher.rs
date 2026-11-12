//! Egy fájl egyezései a kifejezésekre, a szűkített tényekből (definíciók, fájlnév, név-részletek, string-literálok,
//! hívások) — forrásfájl olvasása nélkül. A kifejezések kevesen vannak, ezért fájlonként egyetlen menet a tényeken.
//! A több szavas kifejezés (`Phrase`) csak string-literálra illeszt, egészében; a `route::név` (`Route`) a névvel
//! egyenlő string-literálra (a `routes/*.php` fájlban definíció: `->name('név')`) és a `route::név` hívásra.

use super::callers::{CallerCounts, Callers};
use super::terms::{Term, TermKind, same_word, short_name, split_member, words};
use crate::model::FileGraph;

/// Az egyezés fajtája, a legerősebbtől a leggyengébbig (a sorrend a rangsor és a kiírás sorrendje is).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Kind {
    /// A konténer (osztály, struct, komponens, modul), metódus / függvény, konstans vagy property / mező neve maga a
    /// kifejezés; a `route::név` kifejezésnél az útvonal definíciója (`routes/*.php`: `->name('név')`).
    Definition,
    /// A fájl neve a kifejezés (kiterjesztéssel vagy anélkül), vagy az út vége (`pages/checkout.vue`).
    FileName,
    /// A kifejezés szavai egy név szavai között (`refund` ~ `RefundJob`, `refunds_table`).
    NamePart,
    /// Egy string-literál tartalmazza a kifejezést (útvonal, fordítási kulcs, oszlopnév).
    StringLit,
    /// Egy hívás célja illik a kifejezésre (pontos név vagy név-részlet) — a hívó fájlt hozza.
    Call,
}

/// Amire az egyezés mutat a fájlban.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Symbol {
    File,
    Container {
        kind: String,
        name: String,
        line: u32,
        end: u32,
    },
    Method {
        container: Option<String>,
        name: String,
        line: u32,
        end: u32,
        callers: Callers,
    },
    Constant {
        name: String,
        line: u32,
    },
    /// Property / mező (PHP-ban `$`-jellel: `$fillable`).
    Property {
        container: Option<String>,
        name: String,
        line: u32,
    },
    /// JSON-kulcs (a teljes kulcs-út: `devDependencies.laravel-vite-plugin`).
    Key {
        name: String,
        line: u32,
    },
    Str {
        value: String,
        line: u32,
    },
    Call {
        caller: Option<String>,
        target: String,
        line: u32,
    },
}

/// Egy egyezés: melyik kifejezés (index a kifejezés-listában), milyen fajta, mire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Hit {
    pub term: usize,
    pub kind: Kind,
    pub symbol: Symbol,
}

/// A kifejezések illesztése egy fájl tényeire.
pub(crate) struct Matcher<'a> {
    terms: &'a [Term],
    callers: &'a CallerCounts,
}

impl<'a> Matcher<'a> {
    pub(crate) fn new(terms: &'a [Term], callers: &'a CallerCounts) -> Self {
        Self { terms, callers }
    }

    /// A fájl összes egyezése. `rel`: a gyökérhez relatív út; `code`: kódfájl-e. A dokumentum / adat (md, html, json) a
    /// fájlnevével, a JSON még a kulcsával is illeszthető — de csak pontosan (a teljes kulcs-út vagy az utolsó
    /// tagja, `laravel-vite-plugin`, `scripts.build`): a címsorok és a név-részletek zajt adnának.
    pub(crate) fn file_hits(&self, rel: &str, graph: &FileGraph, code: bool) -> Vec<Hit> {
        let mut hits = Vec::new();
        self.file_name(rel, code, &mut hits);
        if code {
            self.definitions(graph, &mut hits);
            self.properties(rel, graph, &mut hits);
            self.strings(rel, graph, &mut hits);
            self.calls(graph, &mut hits);
        } else {
            self.document_keys(graph, &mut hits);
        }
        hits
    }

    /// A legtöbb kifejezést egyesítő azonosító (definíció neve vagy fájlnév) független kifejezéseinek száma a fájl
    /// egyezéseiből: `toggleFavorite` a `toggle` + `favorite` kifejezésekre 2. A string-literál és a hívás nem
    /// számít (csak a fájlban definiált név és a fájl neve).
    pub(crate) fn multi_terms(&self, hits: &[Hit]) -> usize {
        let mut groups: Vec<(&Symbol, Vec<usize>)> = Vec::new();
        for hit in hits.iter().filter(|hit| identifier_hit(hit)) {
            match groups.iter().position(|(symbol, _)| *symbol == &hit.symbol) {
                Some(at) => {
                    let terms = &mut groups[at].1;
                    if !terms.contains(&hit.term) {
                        terms.push(hit.term);
                    }
                }
                None => groups.push((&hit.symbol, vec![hit.term])),
            }
        }
        groups
            .iter()
            .map(|(_, terms)| self.independent(terms))
            .max()
            .unwrap_or(0)
    }

    /// A független kifejezések száma: amelyiknek minden szavát egy másik kifejezés szavai lefedik (`refund` a
    /// `RefundController` mellett; az azonos szavú `favorite` / `favorites` párból a későbbi), nem számít külön.
    fn independent(&self, terms: &[usize]) -> usize {
        terms
            .iter()
            .filter(|&&a| {
                !terms
                    .iter()
                    .any(|&b| b != a && self.covers(b, a) && (b < a || !self.covers(a, b)))
            })
            .count()
    }

    /// Az `outer` kifejezés szavai között megvan-e az `inner` minden szava (egyes / többes szám tűréssel).
    fn covers(&self, outer: usize, inner: usize) -> bool {
        let (Some(outer), Some(inner)) = (self.terms.get(outer), self.terms.get(inner)) else {
            return false;
        };
        !inner.words.is_empty()
            && inner
                .words
                .iter()
                .all(|w| outer.words.iter().any(|x| same_word(x, w)))
    }

    /// A JSON-kulcsok (`kind: key`) pontos egyezése: a teljes kulcs-út vagy az utolsó tagja.
    fn document_keys(&self, graph: &FileGraph, hits: &mut Vec<Hit>) {
        for key in graph.constants.iter().filter(|k| k.kind == "key") {
            let path = key.name.to_lowercase();
            let last = path.rsplit('.').next().unwrap_or(&path);
            for (i, term) in self.terms.iter().enumerate() {
                if term.is_plain() && (term.lower == path || term.lower == last) {
                    let symbol = Symbol::Key {
                        name: key.name.clone(),
                        line: key.line,
                    };
                    Self::push(hits, i, Kind::Definition, symbol);
                }
            }
        }
    }

    fn push(hits: &mut Vec<Hit>, term: usize, kind: Kind, symbol: Symbol) {
        hits.push(Hit { term, kind, symbol });
    }

    fn file_name(&self, rel: &str, code: bool, hits: &mut Vec<Hit>) {
        let path = rel.replace('\\', "/").to_lowercase();
        let name = path.rsplit('/').next().unwrap_or(&path);
        let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
        // a szavak az eredeti írásmódból (a kisbetűsítés a camelCase-határt eltüntetné)
        let original = rel.rsplit(['/', '\\']).next().unwrap_or(rel);
        let stem_words = words(original.rsplit_once('.').map_or(original, |(stem, _)| stem));
        for (i, term) in self.terms.iter().enumerate() {
            if !term.is_plain() {
                continue;
            }
            let t = term.lower.replace('\\', "/");
            let exact = name == t
                || stem == t
                || (t.contains('/') && (path == t || path.ends_with(&format!("/{t}"))));
            if exact {
                Self::push(hits, i, Kind::FileName, Symbol::File);
            } else if code && term.matches_words(&stem_words) {
                Self::push(hits, i, Kind::NamePart, Symbol::File);
            }
        }
    }

    fn definitions(&self, graph: &FileGraph, hits: &mut Vec<Hit>) {
        for c in &graph.containers {
            let symbol = || Symbol::Container {
                kind: c.kind.clone(),
                name: c.name.clone(),
                line: c.line,
                end: c.end_line,
            };
            self.name_hits(&c.name, None, symbol, hits);
        }
        for m in &graph.methods {
            let symbol = || Symbol::Method {
                container: m.container.clone(),
                name: m.name.clone(),
                line: m.line,
                end: m.end_line.unwrap_or(m.line),
                callers: self.callers.of(m.container.as_deref(), &m.name),
            };
            self.name_hits(&m.name, m.container.as_deref(), symbol, hits);
        }
        for k in &graph.constants {
            let symbol = || Symbol::Constant {
                name: k.name.clone(),
                line: k.line,
            };
            self.name_hits(&k.name, k.container.as_deref(), symbol, hits);
        }
    }

    /// Property-k / mezők (PHP property és promoted constructor property, TS/JS class field, Rust struct field,
    /// Swift / Python / Go property, Vue prop): a név pontosan (a `$` / `#` jel nélkül is: `fillable` = `$fillable`,
    /// vagy `Típus::$név`) definíció, a szavai között név-részlet.
    fn properties(&self, rel: &str, graph: &FileGraph, hits: &mut Vec<Hit>) {
        let php = rel.to_ascii_lowercase().ends_with(".php");
        for p in &graph.properties {
            let bare = p.name.trim_start_matches(['$', '#']);
            if bare.is_empty() {
                continue;
            }
            let lower = bare.to_lowercase();
            let owner = p.container.as_deref().map(str::to_lowercase);
            // a kiírt név: PHP-ban `$`-jellel, ahogy a forrásban áll
            let shown = if php {
                format!("${bare}")
            } else {
                p.name.clone()
            };
            let mut name_words: Option<Vec<String>> = None;
            for (i, term) in self.terms.iter().enumerate() {
                if !term.is_plain() {
                    continue;
                }
                let member = term.member.as_ref().is_some_and(|(want_owner, want_name)| {
                    want_name.trim_start_matches(['$', '#']) == lower
                        && owner.as_deref().is_some_and(|o| short_name(o) == want_owner)
                });
                let kind = if term.lower.trim_start_matches(['$', '#']) == lower || member {
                    Kind::Definition
                } else if term.matches_words(name_words.get_or_insert_with(|| words(bare))) {
                    Kind::NamePart
                } else {
                    continue;
                };
                let symbol = Symbol::Property {
                    container: p.container.clone(),
                    name: shown.clone(),
                    line: p.line,
                };
                Self::push(hits, i, kind, symbol);
            }
        }
    }

    /// Egy definíció neve: pontos egyezés (a név, a rövid neve, vagy `Típus::név`), különben név-részlet.
    fn name_hits(
        &self,
        name: &str,
        owner: Option<&str>,
        symbol: impl Fn() -> Symbol,
        hits: &mut Vec<Hit>,
    ) {
        let lower = name.to_lowercase();
        let owner = owner.map(str::to_lowercase);
        // a JS objektum-literál metódusa (`api.refund`) a saját „típusát” a nevében hordja
        let (inner_owner, bare) = split_member(&lower);
        let mut name_words: Option<Vec<String>> = None;
        for (i, term) in self.terms.iter().enumerate() {
            if !term.is_plain() {
                continue;
            }
            let exact = term.is_name(name)
                || term.is_member(owner.as_deref(), &lower)
                || term.is_member(inner_owner, bare);
            if exact {
                Self::push(hits, i, Kind::Definition, symbol());
            } else if term.matches_words(name_words.get_or_insert_with(|| words(name))) {
                Self::push(hits, i, Kind::NamePart, symbol());
            }
        }
    }

    /// String-literálok: a név / fogalom részstringként (legalább 3 betű), a több szavas kifejezés egészében (a
    /// szóközök számától függetlenül), az útvonalnév pontosan — a `routes/` alatti PHP-fájlban definícióként.
    fn strings(&self, rel: &str, graph: &FileGraph, hits: &mut Vec<Hit>) {
        let route_kind = if is_route_file(rel) {
            Kind::Definition
        } else {
            Kind::StringLit
        };
        for s in &graph.strings {
            let value = s.value.to_lowercase();
            let mut flat: Option<String> = None;
            for (i, term) in self.terms.iter().enumerate() {
                let kind = match &term.kind {
                    TermKind::Name => {
                        let found = term.lower.chars().count() >= 3 && value.contains(&term.lower);
                        found.then_some(Kind::StringLit)
                    }
                    TermKind::Phrase => {
                        let flat = flat.get_or_insert_with(|| collapse_spaces(&value));
                        flat.contains(&term.lower).then_some(Kind::StringLit)
                    }
                    TermKind::Route(name) => (value.trim() == name.as_str()).then_some(route_kind),
                };
                if let Some(kind) = kind {
                    let symbol = Symbol::Str {
                        value: s.value.clone(),
                        line: s.line,
                    };
                    Self::push(hits, i, kind, symbol);
                }
            }
        }
    }

    fn calls(&self, graph: &FileGraph, hits: &mut Vec<Hit>) {
        for call in graph.calls.iter().filter(|call| !call.is_dynamic()) {
            let target = call.resolved.as_deref().unwrap_or(&call.target);
            // a névtér nélkül: `App\Services\RefundService::processRefund` → `RefundService::processRefund`
            let short = target.rsplit('\\').next().unwrap_or(target);
            let (owner, bare) = split_member(short);
            if bare.is_empty() {
                continue;
            }
            let lower = target.to_lowercase();
            let short_lower = short.to_lowercase();
            let bare_lower = bare.to_lowercase();
            let owner_lower = owner.map(str::to_lowercase);
            // a szavak az eredeti írásmódból (`processRefund` → `process`, `refund`)
            let mut bare_words: Option<Vec<String>> = None;
            for (i, term) in self.terms.iter().enumerate() {
                match term.kind {
                    TermKind::Name => {}
                    TermKind::Phrase => continue,
                    // a JS-ben a `route('név')` hívás célja `route::név` (Ziggy)
                    TermKind::Route(_) => {
                        if lower == term.lower {
                            let symbol = Symbol::Call {
                                caller: call.caller.clone(),
                                target: short.to_owned(),
                                line: call.line,
                            };
                            Self::push(hits, i, Kind::Call, symbol);
                        }
                        continue;
                    }
                }
                let exact = bare_lower == term.lower
                    || short_lower == term.lower
                    || lower == term.lower
                    || term.is_member(owner_lower.as_deref(), &bare_lower);
                if exact || term.matches_words(bare_words.get_or_insert_with(|| words(bare))) {
                    let symbol = Symbol::Call {
                        caller: call.caller.clone(),
                        target: short.to_owned(),
                        line: call.line,
                    };
                    Self::push(hits, i, Kind::Call, symbol);
                }
            }
        }
    }
}

/// Azonosító-egyezés-e: egy definíció neve vagy a fájlnév illik a kifejezésre (pontosan vagy név-részletként). A
/// property / mező nem számít: a sok modellben ismétlődő mező (`user_id`) ne emelje a fájlt a pontos szintre.
fn identifier_hit(hit: &Hit) -> bool {
    let kind = matches!(hit.kind, Kind::Definition | Kind::FileName | Kind::NamePart);
    let symbol = match hit.symbol {
        Symbol::File
        | Symbol::Container { .. }
        | Symbol::Method { .. }
        | Symbol::Constant { .. } => true,
        Symbol::Property { .. }
        | Symbol::Key { .. }
        | Symbol::Str { .. }
        | Symbol::Call { .. } => false,
    };
    kind && symbol
}

/// Útvonal-definíciós fájl-e: PHP-fájl a `routes/` mappában (Laravel: `routes/web.php`, `routes/api.php`; a
/// projekt-gyökérhez vagy egy modulhoz relatívan).
fn is_route_file(rel: &str) -> bool {
    let path = rel.replace('\\', "/").to_ascii_lowercase();
    path.ends_with(".php") && (path.starts_with("routes/") || path.contains("/routes/"))
}

/// A szóközök (sortörés, tab, több szóköz) egy szóközre cserélve.
fn collapse_spaces(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
