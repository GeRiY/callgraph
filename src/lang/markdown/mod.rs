//! Markdown dokumentum-indexelő.

use crate::core::SourceFile;
use crate::error::Result;
use crate::lang::{Analysis, Language, LanguageDocument, LanguageRole};
use crate::model::{Call, Container, FileGraphBuilder, Method, StringLit};
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use std::path::{Path, PathBuf};

/// Markdown-fájlokat a közös dokumentum-gráfba képező nyelv.
#[derive(Debug, Default, Clone, Copy)]
pub struct MdDocument;

impl MdDocument {
    /// Alapértelmezett Markdown parser-összeállítás.
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
    fn level(level: HeadingLevel) -> u8 {
        level as u8
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
}

impl Language for MdDocument {
    fn id(&self) -> &'static str {
        "markdown"
    }
    fn role(&self) -> LanguageRole {
        LanguageRole::Document
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["md"]
    }

    fn analyze(&self, file: &SourceFile) -> Result<Analysis> {
        let text = String::from_utf8_lossy(&file.text);
        let starts = Self::starts(&text);
        let name = file
            .path
            .file_name()
            .and_then(|x| x.to_str())
            .unwrap_or_default()
            .to_owned();
        let end = text.lines().count().max(1) as u32;
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
        let mut current_heading: Option<(HeadingLevel, usize, String)> = None;
        let mut headings: Vec<(u32, u8, String)> = Vec::new();
        let events = Parser::new_ext(&text, Options::all()).into_offset_iter();
        for (event, range) in events {
            match event {
                Event::Start(Tag::Heading { level, .. }) => {
                    current_heading = Some((level, range.start, String::new()))
                }
                Event::Text(value) | Event::Code(value) => {
                    if let Some((_, _, title)) = current_heading.as_mut() {
                        title.push_str(&value);
                    }
                }
                Event::End(TagEnd::Heading(_)) => {
                    if let Some((level, start, title)) = current_heading.take() {
                        headings.push((
                            Self::line(start, &starts),
                            Self::level(level),
                            title.trim().to_owned(),
                        ));
                    }
                }
                Event::Start(Tag::Link { dest_url, .. }) => {
                    let caller = headings
                        .last()
                        .map(|h| h.2.clone())
                        .unwrap_or_else(|| "__top__".into());
                    let target = dest_url.to_string();
                    b.call(Call {
                        handler: None,
                        from: None,
                        container: Some(name.clone()),
                        caller: Some(caller),
                        line: Self::line(range.start, &starts),
                        end_line: None,
                        kind: "link".into(),
                        target: target.clone(),
                        resolved: None,
                        on: None,
                        category: Self::category(&target).into(),
                        args: None,
                    });
                }
                Event::Start(Tag::Image { dest_url, .. }) => {
                    let caller = headings
                        .last()
                        .map(|h| h.2.clone())
                        .unwrap_or_else(|| "__top__".into());
                    b.call(Call {
                        handler: None,
                        from: None,
                        container: Some(name.clone()),
                        caller: Some(caller),
                        line: Self::line(range.start, &starts),
                        end_line: None,
                        kind: "image".into(),
                        target: dest_url.to_string(),
                        resolved: None,
                        on: None,
                        category: "image".into(),
                        args: None,
                    });
                }
                Event::Start(Tag::CodeBlock(kind)) => {
                    let lang = match kind {
                        CodeBlockKind::Fenced(value) => value.to_string(),
                        CodeBlockKind::Indented => "plaintext".into(),
                    };
                    let start = Self::line(range.start, &starts);
                    let close = Self::line(range.end.saturating_sub(1), &starts);
                    b.string(StringLit {
                        container: Some(name.clone()),
                        caller: Some("__top__".into()),
                        line: start,
                        end_line: Some(close),
                        value: format!(
                            "code:{}",
                            if lang.is_empty() { "plaintext" } else { &lang }
                        ),
                    });
                }
                _ => {}
            }
        }
        for (idx, (line, level, title)) in headings.iter().enumerate() {
            let section_end = headings
                .iter()
                .skip(idx + 1)
                .find(|(_, next, _)| *next <= *level)
                .map_or(end, |(next, _, _)| next - 1);
            b.method(Method {
                container: Some(name.clone()),
                name: title.clone(),
                line: *line,
                end_line: Some(section_end),
                visibility: Some(format!("h{level}")),
                is_static: false,
                is_abstract: false,
                params: vec![],
                return_type: None,
                doc_types: None,
                doc: None,
                body: None,
                kind: None,
            });
        }
        Ok(Analysis {
            graph: b.build(),
            warnings: Vec::new(),
        })
    }
}

impl LanguageDocument for MdDocument {
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
    #[test]
    fn indexes_sections_links_and_code_ranges() {
        let file = SourceFile::new(
            "README.md",
            b"# H\nsee [x](a.md)\n```rust\nlet x = 1;\n```\n## S\n".to_vec(),
            None,
        );
        let graph = MdDocument.analyze(&file).unwrap().graph;
        assert_eq!(graph.methods[0].end_line, Some(6));
        assert_eq!(graph.calls[0].category, "internal_file");
        assert_eq!(graph.strings[0].end_line, Some(5));
    }

    #[test]
    fn markdown_golden_rows_match() {
        let mut dir = Some(std::path::Path::new(env!("CARGO_MANIFEST_DIR")));
        let Some(reference) = (loop {
            let Some(current) = dir else { break None };
            match std::fs::read_to_string(current.join("golden/ref-md.jsonl")) {
                Ok(value) => break Some(value),
                Err(_) => dir = current.parent(),
            }
        }) else {
            return;
        };
        for line in reference.lines().filter(|line| !line.trim().is_empty()) {
            let row: serde_json::Value = serde_json::from_str(line).unwrap();
            let path = std::path::Path::new(row["path"].as_str().unwrap());
            let graph = MdDocument
                .analyze(&SourceFile::read(path).unwrap())
                .unwrap()
                .graph;
            assert_eq!(
                serde_json::to_value(graph).unwrap(),
                row["graph"],
                "{}",
                path.display()
            );
        }
    }
}
