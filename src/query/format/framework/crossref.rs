//! A `i18n` és a `classes` nézet: a fordítási kulcsok ↔ `$t()` / `__()` hívások (hiányzó és nem használt kulcsok
//! nyelvenként, hiányzó kulcs a hívásnál), és a stíluslap-osztályok ↔ sablon-használat (hol használják, lenient
//! „nem használt” jelzés). A sorok a projekt-szintű kapcsolatokból (`crate::packages::CrossRef`) jönnek — a
//! lekérdezett fájlok csak a hatókört adják; kapcsolat nélkül az üres szöveg.

use crate::i18n::Lang;
use crate::packages::crossref::scopes;
use crate::query::QueryResult;
use crate::query::format::Formatter;

pub(super) struct I18nView;
pub(super) struct ClassesView;

fn text(lines: Vec<String>, empty: &str) -> String {
    if lines.is_empty() {
        format!("{empty}\n")
    } else {
        format!("{}\n", lines.join("\n"))
    }
}

impl Formatter for I18nView {
    fn format(&self, rows: &[QueryResult], lang: Lang) -> String {
        let lines = rows
            .iter()
            .find_map(|row| row.crossref.as_ref())
            .map(|crossref| crossref.i18n_view(&scopes(rows), lang))
            .unwrap_or_default();
        text(
            lines,
            lang.pick("(no translation files found)", "(nincs nyelvfájl találat)"),
        )
    }
}

impl Formatter for ClassesView {
    fn format(&self, rows: &[QueryResult], lang: Lang) -> String {
        let lines = rows
            .iter()
            .find_map(|row| row.crossref.as_ref())
            .map(|crossref| crossref.classes_view(&scopes(rows), lang))
            .unwrap_or_default();
        text(
            lines,
            lang.pick(
                "(no stylesheet classes found)",
                "(nincs stíluslap-osztály találat)",
            ),
        )
    }
}
