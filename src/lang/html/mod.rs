//! HTML dokumentum-indexelő (`.html`, `.htm`): a Markdown-dokumentum mintájára — a `<title>` és a `<h1>`–`<h6>`
//! címsorok szakaszként (sortartománnyal), a látható szövegek és a szöveges attribútumok string-literálként (a
//! `--strings`-hez), a linkek (`<a href>`, `<img src>`) hívásként. A `<script>` / `<style>` tartalma kimarad (nincs
//! JS-elemzés), de a `<script src>` és a `<link rel="stylesheet" href>` belépő-import (a JS-importhoz hasonló
//! `import` sor és `module_use` hívás-él; a külső URL nem él). Nem teljes HTML-parser: a hibatűrő tag-szkenner (`scanner`) dolgozik.

mod scanner;

use crate::core::{LineIndex, SourceFile};
use crate::error::Result;
use crate::lang::document::{
    container, file_name, link_category, resolve_relative, section_ends, section_method,
};
use crate::lang::js::Resolver;
use crate::lang::{Analysis, Language, LanguageDocument, LanguageRole};
use crate::model::{Call, FileGraphBuilder, StringLit};
use std::path::{Component, Path, PathBuf};

pub use crate::lang::document::TITLE_KIND;

/// HTML-fájlokat a közös dokumentum-gráfba képező nyelv.
#[derive(Debug, Default, Clone, Copy)]
pub struct HtmlDocument;

impl HtmlDocument {
    /// Alapértelmezett összeállítás.
    pub fn with_defaults() -> Self {
        Self
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
        let lines = LineIndex::new(&text);
        let line = |offset: usize| lines.line_of(offset);
        let name = file_name(&file.path);
        let end = text.lines().count().max(1) as u32;
        let scan = scanner::scan(&text);
        let mut b = FileGraphBuilder::new(file.path_string());
        b.container(container(&name, end));
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
            b.method(section_method(&name, &title.text, first, last, 0));
        }
        let headings: Vec<(u32, u8, &str)> = scan
            .headings
            .iter()
            .map(|h| (line(h.start), h.level, h.text.as_str()))
            .collect();
        // a szakasz a következő azonos vagy magasabb szintű címsorig tart
        let spans: Vec<(u32, u8)> = headings
            .iter()
            .map(|&(first, level, _)| (first, level))
            .collect();
        for (&(first, level, title), section_end) in headings.iter().zip(section_ends(&spans, end))
        {
            b.method(section_method(&name, title, first, section_end, level));
        }
        let resolver = Resolver::new();
        for import in &scan.imports {
            if is_external_ref(&import.target) {
                continue;
            }
            let at = line(import.start);
            // az outline `import <hivatkozás>  L<n>` sora (mint a CSS `@import`)
            let mut method = section_method(&name, &import.target, at, at, 0);
            method.kind = Some(IMPORT_KIND.to_owned());
            b.method(method);
            let resolved = resolve_entry(&resolver, &import.target, &file.path);
            b.call(
                Call::new(IMPORT_KIND, import.target.clone(), "module_use")
                    .at(at)
                    .within(Some(name.clone()), Some(import.target.clone()))
                    .resolved(resolved),
            );
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
                link_category(&link.target)
            };
            b.call(
                Call::new(
                    if link.image { "image" } else { "link" },
                    link.target.clone(),
                    category,
                )
                .at(line(link.start))
                .within(Some(name.clone()), Some(caller_at(link.start))),
            );
        }
        Ok(Analysis {
            graph: b.build(),
            warnings: Vec::new(),
        })
    }
}

/// A `Method.kind` / `Call.kind` a `<script src>` / `<link href>` belépő-importnak.
pub const IMPORT_KIND: &str = "import";

/// Külső (séma-, protokoll-relatív, `data:`) hivatkozás: nem él a projektben.
fn is_external_ref(target: &str) -> bool {
    target.starts_with("//")
        || target.contains("://")
        || target.starts_with("data:")
        || target.starts_with("mailto:")
        || target.starts_with('#')
}

/// A belépő-hivatkozás fájlja a workspace-hez relatív úton, ha létezik: a `/`-előtagú (Vite) a csomag-gyökérhez
/// (majd a HTML mappájához és a workspace-hez), a többi a HTML-fájlhoz képest. A `?query` / `#fragment` elmarad.
fn resolve_entry(resolver: &Resolver, target: &str, from: &Path) -> Option<String> {
    let clean = target.split(['?', '#']).next().unwrap_or_default();
    let cfg = resolver.config(from);
    let dir = from.parent()?;
    let dir = if dir.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        dir.to_path_buf()
    };
    let candidates: Vec<PathBuf> = match clean.strip_prefix('/') {
        Some(rest) => [cfg.root.clone(), dir, cfg.workspace.clone()]
            .into_iter()
            .map(|base| base.join(rest))
            .collect(),
        None => vec![dir.join(clean)],
    };
    let file = candidates
        .into_iter()
        .map(|path| normalize_path(&path))
        .find(|path| path.is_file())?;
    Some(resolver.module_path(&file))
}

/// `.` / `..` lexikális feloldása (nem követi a symlinket).
fn normalize_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

impl LanguageDocument for HtmlDocument {
    fn resolve_reference(&self, reference: &str, from: &Path) -> Option<PathBuf> {
        resolve_relative(reference, from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

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
        assert_eq!(
            found("Intro text").unwrap().caller.as_deref(),
            Some("Overview")
        );
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

    #[test]
    fn script_and_stylesheet_links_become_import_edges() {
        let root = std::env::temp_dir().join(format!("cg_html_entry_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("client/src")).unwrap();
        fs::create_dir_all(root.join("client/styles")).unwrap();
        fs::write(root.join("client/package.json"), "{}").unwrap();
        fs::write(root.join("client/src/main.ts"), "export {}").unwrap();
        fs::write(root.join("client/styles/site.css"), "a{}").unwrap();
        let page = "<html><head><title>App</title>
<link rel=\"stylesheet\" href=\"styles/site.css?v=2\">
<link rel=\"stylesheet\" href=\"https://cdn.example.test/x.css\">
<script src=\"//cdn.example.test/y.js\"></script>
</head><body><script type=\"module\" src=\"/src/main.ts\"></script>
<script src=\"gone.js\"></script></body></html>
";
        let path = root.join("client/index.html");
        let file = SourceFile::new(path, page.as_bytes().to_vec(), None);
        let graph = HtmlDocument.analyze(&file).unwrap().graph;
        let imports: Vec<(&str, u32)> = graph
            .methods
            .iter()
            .filter(|m| m.kind.as_deref() == Some(IMPORT_KIND))
            .map(|m| (m.name.as_str(), m.line))
            .collect();
        assert_eq!(
            imports,
            [
                ("styles/site.css?v=2", 2),
                ("/src/main.ts", 5),
                ("gone.js", 6)
            ]
        );
        let edges: Vec<(&str, Option<&str>, &str)> = graph
            .calls
            .iter()
            .filter(|c| c.kind == IMPORT_KIND)
            .map(|c| {
                (
                    c.target.as_str(),
                    c.resolved.as_deref(),
                    c.category.as_str(),
                )
            })
            .collect();
        assert_eq!(edges.len(), 3, "{edges:?}");
        // the display path is relative to the project root (the package folder, or a parent marker)
        let ends = |at: usize, suffix: &str| edges[at].1.is_some_and(|r| r.ends_with(suffix));
        assert!(ends(0, "styles/site.css"), "{edges:?}");
        assert!(ends(1, "src/main.ts"), "{edges:?}");
        assert_eq!(edges[2].1, None, "a missing file has no resolved target");
        assert!(edges.iter().all(|e| e.2 == "module_use"));
        let _ = fs::remove_dir_all(&root);
    }
}
