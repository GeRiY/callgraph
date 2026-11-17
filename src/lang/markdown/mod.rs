//! Markdown dokumentum-indexelő.

use crate::core::{LineIndex, SourceFile};
use crate::error::Result;
use crate::lang::document::{
    container, file_name, link_category, resolve_relative, section_ends, section_method,
};
use crate::lang::{Analysis, Language, LanguageDocument, LanguageRole};
use crate::model::{Call, FileGraphBuilder, StringLit};
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
    fn level(level: HeadingLevel) -> u8 {
        level as u8
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
        let lines = LineIndex::new(&text);
        let name = file_name(&file.path);
        let end = text.lines().count().max(1) as u32;
        let mut b = FileGraphBuilder::new(file.path_string());
        b.container(container(&name, end));
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
                            lines.line_of(start),
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
                    b.call(
                        Call::new("link", target.clone(), link_category(&target))
                            .at(lines.line_of(range.start))
                            .within(Some(name.clone()), Some(caller)),
                    );
                }
                Event::Start(Tag::Image { dest_url, .. }) => {
                    let caller = headings
                        .last()
                        .map(|h| h.2.clone())
                        .unwrap_or_else(|| "__top__".into());
                    b.call(
                        Call::new("image", dest_url.to_string(), "image")
                            .at(lines.line_of(range.start))
                            .within(Some(name.clone()), Some(caller)),
                    );
                }
                Event::Start(Tag::CodeBlock(kind)) => {
                    let lang = match kind {
                        CodeBlockKind::Fenced(value) => value.to_string(),
                        CodeBlockKind::Indented => "plaintext".into(),
                    };
                    let start = lines.line_of(range.start);
                    let close = lines.line_of(range.end.saturating_sub(1));
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
        let spans: Vec<(u32, u8)> = headings
            .iter()
            .map(|(line, level, _)| (*line, *level))
            .collect();
        for ((line, level, title), section_end) in headings.iter().zip(section_ends(&spans, end)) {
            b.method(section_method(&name, title, *line, section_end, *level));
        }
        Ok(Analysis {
            graph: b.build(),
            warnings: Vec::new(),
        })
    }
}

impl LanguageDocument for MdDocument {
    fn resolve_reference(&self, reference: &str, from: &Path) -> Option<PathBuf> {
        resolve_relative(reference, from)
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

    /// A referencia nincs a repóban, ezért a teszt `#[ignore]`: nem lehet csendben zöld. Referenciával: `--ignored`.
    #[test]
    #[ignore = "golden/ref-md.jsonl reference missing from the repo (owner decision pending)"]
    fn markdown_golden_rows_match() {
        let mut dir = Some(std::path::Path::new(env!("CARGO_MANIFEST_DIR")));
        let Some(reference) = (loop {
            let Some(current) = dir else { break None };
            match std::fs::read_to_string(current.join("golden/ref-md.jsonl")) {
                Ok(value) => break Some(value),
                Err(_) => dir = current.parent(),
            }
        }) else {
            panic!("golden/ref-md.jsonl not found");
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
