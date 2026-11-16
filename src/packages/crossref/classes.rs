//! CSS-osztály ↔ sablon-használat: a projekt saját stíluslapjainak egyszerű osztály-szelektorai, és hogy melyik
//! sablon / komponens használja őket. A „nem használt osztály” jelzés szándékosan engedékeny: csak a saját
//! stíluslap egyszerű osztálya lehet jelölt, a dinamikus kötés (összefűzés, sablon-literál, `{{ }}`) előtagja /
//! utótagja alá eső osztály „talán használt”, és Tailwind-projektben az utility-nevek kimaradnak. Bizonytalannál
//! nincs jelzés.

use std::collections::{BTreeMap, HashSet};
use std::sync::LazyLock;

use regex::Regex;

use super::classes_scan::{self, Uses};

/// A Tailwind-utility-szerű osztálynevek (csak Tailwind-projektben számít).
fn utility_re() -> &'static Regex {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"^-?(?:sm|md|lg|xl|2xl|flex|inline|grid|block|hidden|contents|absolute|relative|fixed|sticky|static|text|bg|border|rounded|shadow|ring|outline|p[xytrblse]?|m[xytrblse]?|w|h|size|min|max|gap|space|font|leading|tracking|items|justify|content|self|place|overflow|overscroll|z|top|right|bottom|left|inset|start|end|opacity|transition|duration|ease|delay|animate|transform|translate|rotate|scale|skew|origin|cursor|select|pointer|resize|fill|stroke|object|aspect|columns|break|order|col|row|float|clear|isolate|sr|not|dark|group|peer|container|prose|divide|from|via|to|backdrop|blur|brightness|contrast|grayscale|invert|saturate|sepia|drop|list|decoration|underline|overline|line|truncate|italic|uppercase|lowercase|capitalize|antialiased|visible|invisible|collapse|table|caption|whitespace|align|indent|accent|caret|scroll|snap|touch|will|appearance|mix|filter|basis|grow|shrink|flow|auto|backface|bg-clip|content|decoration|display|forced|hyphens|mask|normal|perspective|placeholder|ordinal|tabular|slashed|diagonal|stacked|no|sr-only)(?:-|$)",
        )
        .expect("érvényes regex")
    });
    &RE
}

/// A projekt osztályai: definíciók és használatok.
#[derive(Debug, Default)]
pub(crate) struct Classes {
    /// A használati helyek fájljai (a `Uses` fájl-indexe ide mutat).
    pub(crate) files: Vec<String>,
    /// Osztály → a definíciók (stíluslap, sor).
    pub(crate) defs: BTreeMap<String, Vec<(String, u32)>>,
    /// Stíluslap → a definiált osztályai (név, sor).
    pub(crate) by_sheet: BTreeMap<String, Vec<(String, u32)>>,
    pub(crate) uses: Uses,
    pub(crate) tailwind: bool,
    applied: Vec<(u32, String)>,
}

impl Classes {
    /// Egy stíluslap felvétele; a harmadik féltől származó / minifikált kimarad.
    pub(super) fn add_stylesheet(&mut self, rel: &str, text: &str) {
        if text.contains("tailwindcss") || text.contains("@tailwind") || text.contains("@apply") {
            self.tailwind = true;
        }
        if classes_scan::looks_vendor(rel, text) {
            return;
        }
        let sheet = self.file_index(rel);
        for word in classes_scan::applied_words(text) {
            self.applied.push((sheet, word));
        }
        let mut classes = classes_scan::definitions(text);
        classes.sort();
        classes.dedup();
        for (name, line) in classes {
            self.defs
                .entry(name.clone())
                .or_default()
                .push((rel.to_owned(), line));
            self.by_sheet
                .entry(rel.to_owned())
                .or_default()
                .push((name, line));
        }
    }

    /// A fájl indexe a használati helyekhez (felveszi, ha új).
    pub(super) fn file_index(&mut self, rel: &str) -> u32 {
        if let Some(at) = self.files.iter().position(|file| file == rel) {
            return at as u32;
        }
        self.files.push(rel.to_owned());
        (self.files.len() - 1) as u32
    }

    /// A definiált osztályok neve (a használat-olvasó csak ezeket jegyzi).
    pub(super) fn defined(&self) -> HashSet<String> {
        self.defs.keys().cloned().collect()
    }

    /// Egy sablon / szkript használatai.
    pub(super) fn add_source(&mut self, rel: &str, text: &str, defined: &HashSet<String>) {
        let file = self.file_index(rel);
        classes_scan::scan_uses(text, file, defined, &mut self.uses);
    }

    /// A gyűjtés vége: a stíluslapok `@apply` / `@extend` szavai használatnak számítanak.
    pub(super) fn finish(&mut self) {
        let applied = std::mem::take(&mut self.applied);
        for (sheet, word) in applied {
            if self.defs.contains_key(&word) {
                let places = self.uses.words.entry(word).or_default();
                if !places.contains(&(sheet, 0)) {
                    places.push((sheet, 0));
                }
            }
        }
    }

    /// A dinamikus kötés előtagja / utótagja, amely alá az osztály esik.
    pub(crate) fn dynamic_hint(&self, class: &str) -> Option<String> {
        self.uses
            .prefixes
            .iter()
            .find(|prefix| class.starts_with(prefix.as_str()))
            .map(|prefix| format!("{prefix}…"))
            .or_else(|| {
                self.uses
                    .suffixes
                    .iter()
                    .find(|suffix| class.ends_with(suffix.as_str()))
                    .map(|suffix| format!("…{suffix}"))
            })
    }

    /// Tailwind-projektben az utility-nevű osztály nem jelölt.
    fn utility_like(&self, class: &str) -> bool {
        self.tailwind && utility_re().is_match(class)
    }

    /// Biztosan nem használt-e az osztály (engedékeny: bizonytalannál `false`).
    pub(crate) fn is_unused(&self, class: &str) -> bool {
        !self.uses.words.contains_key(class)
            && self.dynamic_hint(class).is_none()
            && !self.utility_like(class)
    }

    /// A stíluslap nem használt osztályai (név, sor).
    pub(crate) fn unused_in(&self, sheet: &str) -> Vec<(String, u32)> {
        self.by_sheet
            .get(sheet)
            .map(|list| {
                list.iter()
                    .filter(|(name, _)| self.is_unused(name))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Az osztály használati helyei: (fájl, sor).
    pub(crate) fn used_by(&self, class: &str) -> Vec<(&str, u32)> {
        self.uses
            .words
            .get(class)
            .map(|places| {
                places
                    .iter()
                    .filter_map(|(file, line)| {
                        Some((self.files.get(*file as usize)?.as_str(), *line))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// A fájl által használt, a projekt stíluslapjaiban definiált osztályok: (osztály, első stíluslap).
    pub(crate) fn styles_of(&self, rel: &str) -> Vec<(&str, &str)> {
        let Some(index) = self.files.iter().position(|file| file == rel) else {
            return Vec::new();
        };
        let mut out: Vec<(&str, &str)> = self
            .uses
            .words
            .iter()
            .filter(|(_, places)| places.iter().any(|(file, _)| *file as usize == index))
            .filter_map(|(class, _)| {
                let sheet = self.defs.get(class)?.first()?;
                Some((class.as_str(), sheet.0.as_str()))
            })
            .collect();
        out.sort();
        out
    }

    /// A projekt definiált osztályainak száma.
    pub(crate) fn defined_count(&self) -> usize {
        self.defs.len()
    }
}
