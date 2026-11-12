//! HTML dokumentum-indexelő (`.html`, `.htm`): a Markdown-dokumentum mintájára — a `<title>` és a `<h1>`–`<h6>`
//! címsorok szakaszként (sortartománnyal), a látható szövegek és a szöveges attribútumok string-literálként (a
//! `--strings`-hez), a linkek (`<a href>`, `<img src>`) hívásként. A `<script>` / `<style>` tartalma kimarad (nincs
//! JS-elemzés). Nem teljes HTML-parser: a hibatűrő tag-szkenner (`scanner`) dolgozik.

mod scanner;

use crate::core::SourceFile;
use crate::error::Result;
use crate::lang::{Analysis, Language, LanguageDocument, LanguageRole};
use crate::model::{Call, Container, FileGraphBuilder, Method, StringLit};
use std::path::{Path, PathBuf};

/// A `<title>` szakaszának `kind`-ja (az outline `<title> …` alakban írja).
pub const TITLE_KIND: &str = "title";

/// HTML-fájlokat a közös dokumentum-gráfba képező nyelv.
#[derive(Debug, Default, Clone, Copy)]
pub struct HtmlDocument;

impl HtmlDocument {
    /// Alapértelmezett összeállítás.
    pub fn with_defaults() -> Self {
        Self
    }
    fn starts(text: &str) -> Vec<usize> {
        std::iter::once(0)
            .chain(text.match_indices('\n').map(|(i, _)| i + 1))
            .collect()
    }
    fn line(offset: usize, starts: &[usize]) -> u32 {
        starts.partition_point(|&x| x <= offset).max(1) as u32
    }
    fn category(target: &str) -> &'static str {
        if target.starts_with('#') {
            "anchor"
        } else if target.contains("://") || target.starts_with("mailto:") {
            "external_url"
        } else {
            "internal_file"
        }
    }
    fn section_method(container: &str, name: &str, line: u32, end: u32, level: u8) -> Method {
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
}

impl Language for HtmlDocument {
    fn id(&self) -> &'static str {
        "html"
    }
    fn role(&self) -> LanguageRole {
        LanguageRole::Document
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["html", "htm"]
    }

    fn analyze(&self, file: &SourceFile) -> Result<Analysis> {
        let text = String::from_utf8_lossy(&file.text);
        let starts = Self::starts(&text);
        let line = |offset: usize| Self::line(offset, &starts);
        let name = file
            .path
            .file_name()
            .and_then(|x| x.to_str())
            .unwrap_or_default()
            .to_owned();
        let end = text.lines().count().max(1) as u32;
        let scan = scanner::scan(&text);
        let mut b = FileGraphBuilder::new(file.path_string());
        b.container(Container {
            kind: "document".into(),
            name: name.clone(),
            line: 1,
            end_line: end,
            is_abstract: Some(false),
            is_final: Some(false),
            extends: Some(vec![]),
            implements: Some(vec![]),
            traits: Some(vec![]),
            doc: None,
        });
        // az a címsor, amelyik alá egy pozíció tartozik (előtte `__top__`)
        let caller_at = |offset: usize| -> String {
            scan.headings
                .iter()
                .rev()
                .find(|h| h.start <= offset)
                .map_or_else(|| "__top__".to_owned(), |h| h.text.clone())
        };
        if let Some(title) = &scan.title {
            let first = line(title.start);
            let last = line(title.end).max(first);
            b.method(Self::section_method(&name, &title.text, first, last, 0));
        }
        let headings: Vec<(u32, u8, &str)> = scan
            .headings
            .iter()
            .map(|h| (line(h.start), h.level, h.text.as_str()))
            .collect();
        for (idx, (first, level, title)) in headings.iter().enumerate() {
            // a szakasz a következő azonos vagy magasabb szintű címsorig tart
            let section_end = headings
                .iter()
                .skip(idx + 1)
                .find(|(_, next, _)| *next <= *level)
                .map_or(end, |(next, _, _)| next.saturating_sub(1))
                .max(*first);
            b.method(Self::section_method(&name, title, *first, section_end, *level));
        }
        for t in &scan.texts {
            let first = line(t.start);
            let last = line(t.end);
            b.string(StringLit {
                container: Some(name.clone()),
                caller: Some(caller_at(t.start)),
                line: first,
                end_line: (last > first).then_some(last),
                value: t.value.clone(),
            });
        }
        for link in &scan.links {
            let category = if link.image {
                "image"
            } else {
                Self::category(&link.target)
            };
            b.call(Call {
                handler: None,
                container: Some(name.clone()),
                caller: Some(caller_at(link.start)),
                line: line(link.start),
                end_line: None,
                kind: if link.image { "image" } else { "link" }.into(),
                target: link.target.clone(),
                resolved: None,
                on: None,
                category: category.into(),
                args: None,
            });
        }
        Ok(Analysis {
            graph: b.build(),
            warnings: Vec::new(),
        })
    }
}

impl LanguageDocument for HtmlDocument {
    fn resolve_reference(&self, reference: &str, from: &Path) -> Option<PathBuf> {
        if reference.starts_with('#')
            || reference.contains("://")
            || reference.starts_with("mailto:")
        {
            return None;
        }
        let path = from
            .parent()?
            .join(reference.split_once('#').map_or(reference, |(p, _)| p));
        path.exists().then(|| path.canonicalize().unwrap_or(path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = "<!DOCTYPE html>
<html>
<head>
  <title>Design doc</title>
  <style>.hidden { content: 'style-needle'; }</style>
</head>
<body>
  <h1>Overview</h1>
  <p>Intro text</p>
  <h2>Goals</h2>
  <p title=\"Tip\">Goal one</p>
  <h3>Details</h3>
  <a href=\"other.html\">next</a>
  <h2>Risks</h2>
  <script>var x = 'script-needle';</script>
</body>
</html>
";

    fn graph() -> crate::model::FileGraph {
        let file = SourceFile::new("docs/design.html", PAGE.as_bytes().to_vec(), None);
        HtmlDocument.analyze(&file).unwrap().graph
    }

    #[test]
    fn title_and_headings_become_sections_with_line_ranges() {
        let graph = graph();
        let sections: Vec<String> = graph
            .methods
            .iter()
            .map(|m| {
                let level = m.visibility.as_deref().or(m.kind.as_deref());
                format!("{} {}-{:?} {level:?}", m.name, m.line, m.end_line)
            })
            .collect();
        assert_eq!(
            sections,
            [
                "Design doc 4-Some(4) Some(\"title\")",
                "Overview 8-Some(17) Some(\"h1\")",
                "Goals 10-Some(13) Some(\"h2\")",
                "Details 12-Some(13) Some(\"h3\")",
                "Risks 14-Some(17) Some(\"h2\")",
            ]
        );
        assert_eq!(graph.containers[0].name, "design.html");
    }

    #[test]
    fn visible_texts_are_strings_but_script_and_style_are_not() {
        let graph = graph();
        let found = |value: &str| graph.strings.iter().find(|s| s.value == value);
        let goal = found("Goal one").unwrap();
        assert_eq!((goal.line, goal.caller.as_deref()), (11, Some("Goals")));
        assert_eq!(found("Tip").unwrap().line, 11);
        assert_eq!(found("Intro text").unwrap().caller.as_deref(), Some("Overview"));
        assert!(found("Design doc").is_some());
        assert!(
            !graph.strings.iter().any(|s| s.value.contains("needle")),
            "{:?}",
            graph.strings
        );
        assert_eq!(graph.calls.len(), 1);
        assert_eq!(graph.calls[0].category, "internal_file");
        assert_eq!(graph.calls[0].caller.as_deref(), Some("Details"));
    }

    #[test]
    fn broken_html_is_analyzed_without_panic() {
        let file = SourceFile::new(
            "broken.htm",
            b"<h1>Open\n<p title=\"x\n<script>\n".to_vec(),
            None,
        );
        let graph = HtmlDocument.analyze(&file).unwrap().graph;
        assert_eq!(graph.methods[0].name, "Open");
        assert_eq!(graph.methods[0].end_line, Some(3));
    }
}
