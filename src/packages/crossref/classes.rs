//! CSS-osztály ↔ sablon-használat: a projekt saját stíluslapjainak egyszerű osztály-szelektorai, és hogy melyik
//! sablon / komponens használja őket. A „nem használt osztály” jelzés szándékosan engedékeny: csak a saját
//! stíluslap egyszerű osztálya lehet jelölt, a dinamikus kötés (összefűzés, sablon-literál, `{{ }}`) előtagja /
//! utótagja alá eső osztály „talán használt”, és Tailwind-projektben az utility-nevek kimaradnak. Bizonytalannál
//! nincs jelzés.

use std::collections::{BTreeMap, HashSet};
use std::sync::LazyLock;

use regex::Regex;

use super::classes_scan::{self, Uses};
use super::theme::Theme;

/// (class, places `(file, line)`, defined sibling classes) — see `Classes::unknown_theme_uses`.
pub(crate) type UnknownThemeUse<'a> = (&'a str, Vec<(&'a str, u32)>, Vec<String>);

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
    /// Tailwind 4 `@theme` tokens (their utility classes are not in `defs`: they are counted separately).
    pub(crate) theme: Theme,
    /// The classes defined by Tailwind 4 `@utility` (project-defined, so never treated as core utility names).
    pub(crate) utilities: HashSet<String>,
    applied: Vec<(u32, String)>,
}

impl Classes {
    /// Egy stíluslap felvétele; a harmadik féltől származó / minifikált kimarad.
    pub(super) fn add_stylesheet(&mut self, rel: &str, text: &str) {
        if text.contains("tailwindcss")
            || text.contains("@tailwind")
            || text.contains("@apply")
            || text.contains("@theme")
            || text.contains("@utility")
        {
            self.tailwind = true;
        }
        if classes_scan::looks_vendor(rel, text) {
            return;
        }
        let sheet = self.file_index(rel);
        self.theme.add_sheet(rel, text);
        self.utilities.extend(classes_scan::utility_names(text));
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
        self.defs
            .keys()
            .map(String::clone)
            .chain(self.theme.class_names().map(str::to_owned))
            .collect()
    }

    /// Whether there is anything to look for in the sources (a defined class or a theme token).
    pub(super) fn has_targets(&self) -> bool {
        self.defined_count() > 0 || !self.theme.is_empty()
    }

    /// A class a stylesheet defines: a selector, an `@utility`, or a `@theme` token utility.
    fn is_defined(&self, class: &str) -> bool {
        self.defs.contains_key(class) || self.theme.defines(class)
    }

    /// Egy sablon / szkript használatai.
    pub(super) fn add_source(&mut self, rel: &str, text: &str, defined: &HashSet<String>) {
        let file = self.file_index(rel);
        if self.uses.heads.is_empty() && !self.theme.is_empty() {
            self.uses.heads = self.theme.heads();
        }
        classes_scan::scan_uses(text, file, defined, &mut self.uses);
    }

    /// A gyűjtés vége: a stíluslapok `@apply` / `@extend` szavai használatnak számítanak.
    pub(super) fn finish(&mut self) {
        let applied = std::mem::take(&mut self.applied);
        let heads = self.theme.heads();
        for (sheet, word) in applied {
            if self.is_defined(&word) {
                let places = self.uses.words.entry(word).or_default();
                if !places.contains(&(sheet, 0)) {
                    places.push((sheet, 0));
                }
            } else if classes_scan::probe_head(&word, &heads) {
                classes_scan::record_probe(&mut self.uses, &word, sheet, 0);
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
        self.tailwind && !self.utilities.contains(class) && utility_re().is_match(class)
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

    /// The utility classes of a stylesheet's `@theme` tokens that templates use: (class, number of files).
    pub(crate) fn theme_used_in(&self, sheet: &str) -> Vec<(String, usize)> {
        let mut out: Vec<(String, usize)> = self
            .theme
            .in_sheet(sheet)
            .flat_map(|token| token.classes.iter())
            .filter_map(|class| {
                let files: HashSet<&str> =
                    self.used_by(class).iter().map(|(file, _)| *file).collect();
                (!files.is_empty()).then(|| (class.clone(), files.len()))
            })
            .collect();
        out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        out
    }

    /// The number of `@theme` tokens in a stylesheet.
    pub(crate) fn theme_count_in(&self, sheet: &str) -> usize {
        self.theme.in_sheet(sheet).count()
    }

    /// `@utility` classes of a stylesheet that templates use: (class, number of files), most used first.
    pub(crate) fn utilities_used_in(&self, sheet: &str) -> Vec<(String, usize)> {
        let mut out: Vec<(String, usize)> = self
            .by_sheet
            .get(sheet)
            .into_iter()
            .flatten()
            .filter(|(name, _)| self.utilities.contains(name))
            .filter_map(|(name, _)| {
                let files: HashSet<&str> =
                    self.used_by(name).iter().map(|(file, _)| *file).collect();
                (!files.is_empty()).then(|| (name.clone(), files.len()))
            })
            .collect();
        out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        out
    }

    /// Used classes that look like a theme-token utility whose token does not exist (`text-brand-450` when only
    /// `brand-500` is defined): (class, places, defined siblings). Conservative: needs `@theme` tokens with the
    /// same prefix and first name segment; core Tailwind names never match.
    pub(crate) fn unknown_theme_uses(&self) -> Vec<UnknownThemeUse<'_>> {
        if self.theme.is_empty() {
            return Vec::new();
        }
        let heads = self.theme.heads();
        let mut out: Vec<UnknownThemeUse<'_>> = self
            .uses
            .probes
            .iter()
            .filter(|(word, _)| !self.is_defined(word))
            .filter_map(|(word, places)| {
                let siblings = self.theme.unknown(&heads, word)?;
                let at = places
                    .iter()
                    .filter_map(|(file, line)| {
                        Some((self.files.get(*file as usize)?.as_str(), *line))
                    })
                    .collect();
                Some((word.as_str(), at, siblings))
            })
            .collect();
        out.sort_by(|a, b| a.0.cmp(b.0));
        out
    }
}
