//! Egy fájl egyezései a kifejezésekre, a szűkített tényekből (definíciók, fájlnév, név-részletek, string-literálok,
//! hívások) — forrásfájl olvasása nélkül. A kifejezések kevesen vannak, ezért fájlonként egyetlen menet a tényeken.

use super::callers::{CallerCounts, Callers};
use super::terms::{Term, split_member, words};
use crate::model::FileGraph;

/// Az egyezés fajtája, a legerősebbtől a leggyengébbig (a sorrend a rangsor és a kiírás sorrendje is).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Kind {
    /// A konténer (osztály, struct, komponens, modul), metódus / függvény vagy konstans neve maga a kifejezés.
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

    /// A fájl összes egyezése. `rel`: a gyökérhez relatív út; `code`: kódfájl-e (dokumentum / adat — md, json — csak
    /// a fájlnevével illeszthető, a címsorai és kulcsai zajt adnának).
    pub(crate) fn file_hits(&self, rel: &str, graph: &FileGraph, code: bool) -> Vec<Hit> {
        let mut hits = Vec::new();
        self.file_name(rel, code, &mut hits);
        if code {
            self.definitions(graph, &mut hits);
            self.strings(graph, &mut hits);
            self.calls(graph, &mut hits);
        }
        hits
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

    fn strings(&self, graph: &FileGraph, hits: &mut Vec<Hit>) {
        for s in &graph.strings {
            let value = s.value.to_lowercase();
            for (i, term) in self.terms.iter().enumerate() {
                if term.lower.chars().count() >= 3 && value.contains(&term.lower) {
                    let symbol = Symbol::Str {
                        value: s.value.clone(),
                        line: s.line,
                    };
                    Self::push(hits, i, Kind::StringLit, symbol);
                }
            }
        }
    }

    fn calls(&self, graph: &FileGraph, hits: &mut Vec<Hit>) {
        for call in &graph.calls {
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
