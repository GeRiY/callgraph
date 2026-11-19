//! A `i18n` és `classes` nézet sorai, és az outline rövid sorai (nyelvfájl-fejléc, `$t(…)` hívás figyelmeztetése).
//! Tömör: nincs sor, ha nincs mit mondani; a listák `… +N`-nel zárulnak.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use regex::Regex;

use super::{CrossRef, Scope};
use crate::i18n::Lang;
use crate::model::Call;

/// Egy lista (kulcs / osztály) legfeljebb ennyi elemmel jelenik meg.
const SAMPLE: usize = 5;
/// A használati helyek száma egy sorban.
const PLACES: usize = 4;
/// A figyelmeztető sorok legfeljebb ennyivel (a többi `… +N`).
const WARN_LINES: usize = 25;

/// `a, b, c +N`.
fn list(total: usize, sample: &[String]) -> String {
    let shown = sample
        .iter()
        .map(|item| short(item, 40))
        .collect::<Vec<_>>()
        .join(", ");
    if total > sample.len() {
        format!("{shown} +{}", total - sample.len())
    } else {
        shown
    }
}

/// Egy i18n-kulcs a kimenetben: a pont-kulcs teljes (kereshető), a mondat-kulcs (szóközzel) rövidítve.
fn key_text(key: &str) -> String {
    if key.contains(char::is_whitespace) {
        short(key, 40)
    } else {
        key.to_owned()
    }
}

/// `a.b.{c, d.e} +N`: a kulcsok teljesek; a közös szegmens-előtag (legalább egy szegmens) kapcsos zárójel elé kerül.
fn key_list(total: usize, sample: &[String]) -> String {
    let more = if total > sample.len() {
        format!(" +{}", total - sample.len())
    } else {
        String::new()
    };
    let plain = |sample: &[String]| {
        sample
            .iter()
            .map(|key| key_text(key))
            .collect::<Vec<_>>()
            .join(", ")
    };
    if sample.len() < 2 || sample.iter().any(|key| key.contains(char::is_whitespace)) {
        return format!("{}{more}", plain(sample));
    }
    let segments: Vec<Vec<&str>> = sample.iter().map(|key| key.split('.').collect()).collect();
    // a közös előtag legfeljebb a legrövidebb kulcs utolsó előtti szegmenséig tart
    let limit = segments.iter().map(|s| s.len() - 1).min().unwrap_or(0);
    let common = (0..limit)
        .take_while(|&i| segments.iter().all(|s| s[i] == segments[0][i]))
        .count();
    if common == 0 {
        return format!("{}{more}", plain(sample));
    }
    let prefix = segments[0][..common].join(".");
    let rest: Vec<String> = segments.iter().map(|s| s[common..].join(".")).collect();
    format!("{prefix}.{{{}}}{more}", rest.join(", "))
}

/// Az érték rövidítve, egy sorra.
fn short(text: &str, max: usize) -> String {
    let one: String = text
        .chars()
        .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
        .collect();
    if one.chars().count() > max {
        let cut: String = one.chars().take(max).collect();
        format!("{cut}…")
    } else {
        one
    }
}

/// `A.vue:12, B.vue:8 +N` a helyekből (a 0. sor nélkül a fájl neve).
fn places(items: &[(&str, u32)]) -> String {
    let shown: Vec<String> = items
        .iter()
        .take(PLACES)
        .map(|(file, line)| {
            let name = file.rsplit('/').next().unwrap_or(file);
            if *line == 0 {
                name.to_owned()
            } else {
                format!("{name}:{line}")
            }
        })
        .collect();
    if items.len() > PLACES {
        format!("{} +{}", shown.join(", "), items.len() - PLACES)
    } else {
        shown.join(", ")
    }
}

fn is_sheet(rel: &str) -> bool {
    rel.ends_with(".css") || rel.ends_with(".scss")
}

impl CrossRef {
    /// Az outline fejlécsorai a fájlhoz: nyelvfájl → `i18n: missing in en: 3 · unused: 2`, stíluslap →
    /// `classes: unused .a, .b +N`; ha nincs mit mondani, `None`.
    pub(crate) fn header(&self, file: &str, lang: Lang) -> Option<String> {
        let rel = self.rel(file)?;
        let mut lines = Vec::new();
        if let Some(views) = &self.views
            && rel.ends_with(".blade.php")
        {
            lines.extend(views.lines(&rel, lang));
        }
        if let Some(report) = self
            .i18n
            .as_ref()
            .and_then(|i18n| i18n.file_report(&rel, 0))
            && !report.is_empty()
        {
            let mut parts = Vec::new();
            if !report.missing_in.is_empty() {
                let each: Vec<String> = report
                    .missing_in
                    .iter()
                    .map(|(locale, total, _)| format!("{locale}: {total}"))
                    .collect();
                parts.push(format!(
                    "{} {}",
                    lang.pick("missing in", "hiányzik itt:"),
                    each.join(", ")
                ));
            }
            if !report.lacks.is_empty() {
                let each: Vec<String> = report
                    .lacks
                    .iter()
                    .map(|(locale, total, _)| format!("{total} vs {locale}"))
                    .collect();
                parts.push(format!(
                    "{} {}",
                    lang.pick("lacks", "hiányzó:"),
                    each.join(", ")
                ));
            }
            if report.unused.0 > 0 {
                parts.push(format!(
                    "{}: {}",
                    lang.pick("unused", "nem használt"),
                    report.unused.0
                ));
            }
            lines.push(format!("i18n: {}", parts.join(" · ")));
        }
        if let Some(i18n) = self.i18n.as_ref()
            && super::i18n::is_script_path(&rel)
        {
            lines.extend(tree_lines(&i18n.key_tree(&rel), lang));
        }
        if let Some(classes) = &self.classes
            && is_sheet(&rel)
        {
            if classes.theme_count_in(&rel) > 0
                && let Some(line) = unknown_theme_line(classes, lang)
            {
                lines.push(line);
            }
            let unused = classes.unused_in(&rel);
            if !unused.is_empty() {
                let names: Vec<String> = unused
                    .iter()
                    .take(SAMPLE)
                    .map(|(name, _)| format!(".{name}"))
                    .collect();
                lines.push(format!(
                    "classes: {} {}",
                    lang.pick("unused", "nem használt"),
                    list(unused.len(), &names)
                ));
            }
        }
        (!lines.is_empty()).then(|| lines.join("\n"))
    }

    /// Egy `$t('a.b')` (JS / Vue) hívás figyelmeztetése az outline-ban (` ⚠ missing in hu.json` / ` ⚠ no key`).
    pub(crate) fn i18n_link(&self, call: &Call, lang: Lang) -> Option<String> {
        if call.category != "i18n_helper" {
            return None;
        }
        let key = call.resolved.as_deref()?.strip_prefix("i18n::")?;
        if key.is_empty() || key.contains(char::is_whitespace) {
            return None;
        }
        self.i18n.as_ref()?.warning(key, lang)
    }

    /// Az `i18n` nézet sorai a hatókörre; üres: nincs nyelvfájl.
    pub(crate) fn i18n_view(&self, scope: &[Scope], lang: Lang) -> Vec<String> {
        let Some(i18n) = self.i18n.as_ref().filter(|i18n| i18n.has_sets()) else {
            return Vec::new();
        };
        let mut in_scope: BTreeSet<usize> = BTreeSet::new();
        for item in scope {
            if let Some((index, _)) = i18n.lang_file(&item.rel) {
                in_scope.insert(index);
            }
        }
        if in_scope.is_empty() {
            in_scope.extend(0..i18n.sets.len());
        }
        let mut head = Vec::new();
        for index in &in_scope {
            let set = &i18n.sets[*index];
            let keys = set
                .locales
                .values()
                .map(|d| d.keys.len())
                .max()
                .unwrap_or(0);
            let locales: Vec<&str> = set.locales.keys().map(String::as_str).collect();
            head.push(format!(
                "i18n {}: {} · {} {}",
                set.dir,
                locales.join(", "),
                keys,
                lang.pick("keys", "kulcs")
            ));
        }
        let mut body = Vec::new();
        for item in scope {
            if i18n.lang_file(&item.rel).is_none() {
                continue;
            }
            match &item.names {
                Some(names) => {
                    for name in names {
                        for row in i18n.key_rows(&item.rel, name, 8) {
                            body.push(key_line(i18n, &row, lang));
                        }
                    }
                }
                None => {
                    if let Some(report) = i18n.file_report(&item.rel, SAMPLE)
                        && !report.is_empty()
                    {
                        body.push(file_line(&item.rel, &report, lang));
                    }
                }
            }
        }
        let mut warnings: BTreeMap<(String, String), Vec<(&str, u32)>> = BTreeMap::new();
        let files: BTreeSet<&str> = scope.iter().map(|item| item.rel.as_str()).collect();
        for usage in &i18n.found.usages {
            if !files.contains(usage.file.as_str()) {
                continue;
            }
            let status = match i18n.check(&usage.key) {
                super::i18n::KeyState::Missing(labels) => {
                    format!(
                        "{} {}",
                        lang.pick("missing in", "hiányzik:"),
                        labels.join(", ")
                    )
                }
                super::i18n::KeyState::Unknown => {
                    lang.pick("no key", "nincs ilyen kulcs").to_owned()
                }
                _ => continue,
            };
            warnings
                .entry((usage.key.clone(), status))
                .or_default()
                .push((usage.file.as_str(), usage.line));
        }
        let total = warnings.len();
        for ((key, status), sites) in warnings.into_iter().take(WARN_LINES) {
            body.push(format!(
                "⚠ {} · {status} ← {}",
                key_text(&key),
                places(&sites)
            ));
        }
        if total > WARN_LINES {
            body.push(format!("… +{}", total - WARN_LINES));
        }
        if body.is_empty() {
            head.iter_mut().for_each(|line| line.push_str(" · ok"));
        }
        head.extend(body);
        head
    }

    /// A `classes` nézet sorai a hatókörre; üres: nincs stíluslap-osztály vagy nincs mit mondani.
    pub(crate) fn classes_view(&self, scope: &[Scope], lang: Lang) -> Vec<String> {
        let Some(classes) = self.classes.as_ref().filter(|c| c.has_targets()) else {
            return Vec::new();
        };
        let mut body = Vec::new();
        let mut unused_total = 0;
        let mut sheets = 0;
        for item in scope {
            if classes.by_sheet.contains_key(&item.rel) || classes.theme_count_in(&item.rel) > 0 {
                sheets += 1;
                match &item.names {
                    Some(names) => class_lines(classes, &item.rel, names, lang, &mut body),
                    None => {
                        body.extend(tailwind_lines(classes, &item.rel, lang));
                        let unused = classes.unused_in(&item.rel);
                        unused_total += unused.len();
                        if !unused.is_empty() {
                            let shown: Vec<String> = unused
                                .iter()
                                .take(SAMPLE)
                                .map(|(name, line)| format!(".{name} L{line}"))
                                .collect();
                            body.push(format!(
                                "{}  {} {}: {}",
                                item.rel,
                                lang.pick("unused", "nem használt"),
                                unused.len(),
                                list(unused.len(), &shown)
                            ));
                        }
                    }
                }
            } else if scope.len() <= 3 {
                let styles = classes.styles_of(&item.rel);
                if !styles.is_empty() {
                    body.push(format!("{}  styles: {}", item.rel, styles_text(&styles)));
                }
            }
        }
        if sheets == 0 && body.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        if sheets > 0 {
            body.extend(unknown_theme_line(classes, lang));
            out.push(format!(
                "classes: {} {} · {} {}",
                classes.defined_count(),
                lang.pick("defined", "definiált"),
                unused_total,
                lang.pick("unused", "nem használt")
            ));
        }
        out.extend(body);
        out
    }
}

/// Legfeljebb ennyi felső szintű csoport a szótár-modul kulcs-fájában.
const TREE_GROUPS: usize = 30;
/// Egy csoport sorában legfeljebb ennyi gyerek-név.
const TREE_CHILDREN: usize = 8;

/// A szótár-modul kulcs-fája: egy sor felső szintenként (`bed (3): locked, save`), a `név.` a további ágat jelzi.
fn tree_lines(groups: &[super::i18n::TreeGroup], lang: Lang) -> Vec<String> {
    if groups.is_empty() {
        return Vec::new();
    }
    let total: usize = groups.iter().map(|group| group.leaves).sum();
    let mut out = vec![format!("{} ({total}):", lang.pick("keys", "kulcsok"))];
    for group in groups.iter().take(TREE_GROUPS) {
        let mut line = format!("  {} ({})", group.name, group.leaves);
        if !group.children.is_empty() {
            let shown: Vec<&str> = group
                .children
                .iter()
                .take(TREE_CHILDREN)
                .map(String::as_str)
                .collect();
            line.push_str(&format!(": {}", shown.join(", ")));
            if group.children.len() > TREE_CHILDREN {
                line.push_str(&format!(" +{}", group.children.len() - TREE_CHILDREN));
            }
        }
        out.push(line);
    }
    if groups.len() > TREE_GROUPS {
        out.push(format!("  … +{}", groups.len() - TREE_GROUPS));
    }
    out
}

fn file_line(rel: &str, report: &super::i18n::FileReport, lang: Lang) -> String {
    let mut parts = Vec::new();
    for (locale, total, sample) in &report.missing_in {
        parts.push(format!(
            "{} {locale} {total}: {}",
            lang.pick("missing in", "hiányzik itt:"),
            key_list(*total, sample)
        ));
    }
    for (locale, total, sample) in &report.lacks {
        parts.push(format!(
            "{} {total} vs {locale}: {}",
            lang.pick("lacks", "hiányzó:"),
            key_list(*total, sample)
        ));
    }
    if report.unused.0 > 0 {
        parts.push(format!(
            "{} {}: {}",
            lang.pick("unused", "nem használt"),
            report.unused.0,
            key_list(report.unused.0, &report.unused.1)
        ));
    }
    format!("{rel}  {}", parts.join(" · "))
}

fn key_line(i18n: &super::i18n::I18n, row: &super::i18n::KeyRow, lang: Lang) -> String {
    let values: Vec<String> = row
        .values
        .iter()
        .map(|(label, def)| match def {
            Some(def) => format!("{label}:{} \"{}\"", def.line, short(&def.value, 40)),
            None => format!("{label} ✗"),
        })
        .collect();
    let usages = i18n.usages_of(&row.key);
    let used = if usages.is_empty() {
        if i18n.is_used(&row.key) {
            lang.pick("referenced dynamically", "dinamikusan hivatkozott")
                .to_owned()
        } else {
            lang.pick("unused", "nem használt").to_owned()
        }
    } else {
        let sites: Vec<(&str, u32)> = usages
            .iter()
            .map(|usage| (usage.file.as_str(), usage.line))
            .collect();
        format!("{}: {}", lang.pick("used", "használja"), places(&sites))
    };
    format!("{}  {} · {used}", key_text(&row.key), values.join(" · "))
}

/// A CSS-szabály nevek osztályai (`.btn, .btn-primary`) és soronként a használatuk.
fn class_lines(
    classes: &super::classes::Classes,
    sheet: &str,
    names: &[String],
    lang: Lang,
    out: &mut Vec<String>,
) {
    static CLASS: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\.(-?[_a-zA-Z][\w-]*)").expect("érvényes regex"));
    let mut seen = BTreeSet::new();
    for name in names {
        // `@utility name` is a class definition; `@theme` the token summary of the sheet
        if name.trim_start().starts_with("@theme") {
            out.extend(theme_line(classes, sheet, lang));
            continue;
        }
        let utility = name
            .trim()
            .strip_prefix("@utility ")
            .map(|class| format!(".{}", class.trim()));
        let text = utility.as_deref().unwrap_or(name);
        for caps in CLASS.captures_iter(text) {
            let Some(class) = caps.get(1).map(|m| m.as_str()) else {
                continue;
            };
            if !seen.insert(class.to_owned()) {
                continue;
            }
            let Some(defs) = classes.defs.get(class) else {
                continue;
            };
            let at = defs
                .iter()
                .find(|(file, _)| file == sheet)
                .or_else(|| defs.first());
            let site = at.map_or(String::new(), |(file, line)| {
                format!("{}:{line}", file.rsplit('/').next().unwrap_or(file))
            });
            let used = classes.used_by(class);
            let verdict = if !used.is_empty() {
                format!("{}: {}", lang.pick("used by", "használja"), places(&used))
            } else if let Some(hint) = classes.dynamic_hint(class) {
                format!("{} ({hint})", lang.pick("possibly used", "talán használt"))
            } else if classes.is_unused(class) {
                lang.pick("unused", "nem használt").to_owned()
            } else {
                lang.pick("utility (not checked)", "utility (nem ellenőrzött)")
                    .to_owned()
            };
            out.push(format!(".{class}  {site} · {verdict}"));
        }
    }
}

/// `app.css .a, .b; other.css .c` — a használt osztályok stíluslaponként.
fn styles_text(styles: &[(&str, &str)]) -> String {
    let mut by_sheet: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for &(class, sheet) in styles {
        by_sheet.entry(sheet).or_default().push(format!(".{class}"));
    }
    by_sheet
        .into_iter()
        .map(|(sheet, names)| {
            let name = sheet.rsplit('/').next().unwrap_or(sheet);
            format!(
                "{name} {}",
                list(
                    names.len(),
                    &names.iter().take(SAMPLE + 3).cloned().collect::<Vec<_>>()
                )
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// The Tailwind 4 definitions of a stylesheet and where templates use them: the `@utility` and the `@theme` line.
fn tailwind_lines(classes: &super::classes::Classes, sheet: &str, lang: Lang) -> Vec<String> {
    let mut out = Vec::new();
    let utilities = classes.utilities_used_in(sheet);
    if !utilities.is_empty() {
        let shown: Vec<String> = utilities
            .iter()
            .take(SAMPLE)
            .map(|(name, files)| format!("{name} ×{files}"))
            .collect();
        out.push(format!(
            "{sheet}  @utility {} {}: {}",
            utilities.len(),
            lang.pick("used", "használt"),
            list(utilities.len(), &shown)
        ));
    }
    out.extend(theme_line(classes, sheet, lang));
    out
}

/// `app.css  @theme 6 tokens · used 2: bg-brand-500 ×3, …` (`×N`: files using it); no tokens: `None`.
fn theme_line(classes: &super::classes::Classes, sheet: &str, lang: Lang) -> Option<String> {
    let tokens = classes.theme_count_in(sheet);
    if tokens == 0 {
        return None;
    }
    let used = classes.theme_used_in(sheet);
    let shown: Vec<String> = used
        .iter()
        .take(SAMPLE)
        .map(|(name, files)| format!("{name} ×{files}"))
        .collect();
    let tail = if used.is_empty() {
        lang.pick("none used", "egyik sem használt").to_owned()
    } else {
        format!(
            "{} {}: {}",
            lang.pick("used", "használt"),
            used.len(),
            list(used.len(), &shown)
        )
    };
    Some(format!(
        "{sheet}  @theme {tokens} {} · {tail}",
        lang.pick("tokens", "token")
    ))
}

/// `theme: unknown token in use: text-brand-450 A.vue:9 (like text-brand-500) +N`; no unknown use: `None`.
fn unknown_theme_line(classes: &super::classes::Classes, lang: Lang) -> Option<String> {
    let unknown = classes.unknown_theme_uses();
    if unknown.is_empty() {
        return None;
    }
    // never cut inside a class name: whole items, the rest counted
    let shown: Vec<String> = unknown
        .iter()
        .take(3)
        .map(|(class, at, siblings)| {
            format!(
                "{class} {} ({} {})",
                places(at),
                lang.pick("like", "mint"),
                siblings.join(", ")
            )
        })
        .collect();
    let more = unknown.len().saturating_sub(shown.len());
    let tail = if more > 0 {
        format!(" +{more}")
    } else {
        String::new()
    };
    Some(format!(
        "theme: {} {}{tail}",
        lang.pick("no such token", "nincs ilyen token"),
        shown.join("; ")
    ))
}
