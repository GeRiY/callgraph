//! `LanguageDocument`: dokumentum- / adatformátum (JSON, Markdown, HTML, YAML…), és a dokumentum-nyelvek közös
//! építőkövei (`document` konténer, szakasz, szakasz-vég, link-kategória, relatív hivatkozás).

use std::path::{Path, PathBuf};

use super::Language;
use crate::model::{Container, Method};

/// Dokumentum- / adatformátum: nincs futó kód, de szerkezet, kulcsok és hivatkozások vannak.
pub trait LanguageDocument: Language {
    /// A `from` fájlból hivatkozott cél (pl. `$ref`, relatív link) fájlútja, ha feloldható.
    fn resolve_reference(&self, reference: &str, from: &Path) -> Option<PathBuf>;
}

/// A `<title>` szakaszának `kind`-ja (az outline `<title> …` alakban írja).
pub const TITLE_KIND: &str = "title";

/// A `document` konténer: a fájl neve, `1..=end_line`.
pub(crate) fn container(name: &str, end_line: u32) -> Container {
    Container {
        kind: "document".into(),
        name: name.to_owned(),
        line: 1,
        end_line,
        is_abstract: Some(false),
        is_final: Some(false),
        extends: Some(vec![]),
        implements: Some(vec![]),
        traits: Some(vec![]),
        doc: None,
    }
}

/// A fájl neve (útvonal nélkül) a `document` konténer nevének.
pub(crate) fn file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_owned()
}

/// A link célja szerinti kategória: `anchor` (`#…`), `external_url` (séma / `mailto:`), különben `internal_file`.
pub(crate) fn link_category(target: &str) -> &'static str {
    if target.starts_with('#') {
        "anchor"
    } else if is_external(target) {
        "external_url"
    } else {
        "internal_file"
    }
}

/// Séma-szerű (`https://…`) vagy `mailto:` cél.
fn is_external(target: &str) -> bool {
    target.contains("://") || target.starts_with("mailto:")
}

/// A `from` fájlhoz relatív hivatkozás (a `#fragment` nélkül) kanonikus útja, ha létezik; horgony / külső cél: `None`.
pub(crate) fn resolve_relative(reference: &str, from: &Path) -> Option<PathBuf> {
    if reference.starts_with('#') || is_external(reference) {
        return None;
    }
    let path = from
        .parent()?
        .join(reference.split_once('#').map_or(reference, |(p, _)| p));
    path.exists().then(|| path.canonicalize().unwrap_or(path))
}

/// Egy szakasz (címsor) `(sor, szint)` listájából a szakaszok utolsó sora: a következő azonos vagy magasabb szintű
/// címsor előtti sor, utolsó szakasznál `end`; legalább a szakasz saját sora.
pub(crate) fn section_ends(headings: &[(u32, u8)], end: u32) -> Vec<u32> {
    headings
        .iter()
        .enumerate()
        .map(|(idx, &(first, level))| {
            headings
                .iter()
                .skip(idx + 1)
                .find(|&&(_, next)| next <= level)
                .map_or(end, |&(next, _)| next.saturating_sub(1))
                .max(first)
        })
        .collect()
}

/// Egy szakasz az outline-ba: `level` 1–6 címsor (`visibility`: `h1`…`h6`), 0 a HTML `<title>` (`kind: title`).
pub(crate) fn section_method(
    container: &str,
    name: &str,
    line: u32,
    end: u32,
    level: u8,
) -> Method {
    Method {
        container: Some(container.to_owned()),
        name: name.to_owned(),
        line,
        end_line: Some(end),
        visibility: (level > 0).then(|| format!("h{level}")),
        is_static: false,
        is_abstract: false,
        params: vec![],
        return_type: None,
        doc_types: None,
        doc: None,
        body: None,
        kind: (level == 0).then(|| TITLE_KIND.to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sections_end_before_the_next_heading_of_the_same_or_higher_level() {
        let headings = [(1, 1), (3, 2), (5, 3), (7, 2)];
        assert_eq!(section_ends(&headings, 9), [9, 6, 6, 9]);
    }

    #[test]
    fn link_categories_follow_the_target() {
        assert_eq!(link_category("#top"), "anchor");
        assert_eq!(link_category("https://a.b"), "external_url");
        assert_eq!(link_category("mailto:a@b.c"), "external_url");
        assert_eq!(link_category("docs/a.md"), "internal_file");
    }
}
