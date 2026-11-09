//! JSON/JSONC dokumentum-indexelő.

use std::path::{Path, PathBuf};

use jsonc_parser::{
    CollectOptions, CommentCollectionStrategy, ParseOptions, ast::Value, common::Ranged,
    parse_to_ast,
};

use crate::core::SourceFile;
use crate::error::Result;
use crate::lang::{Analysis, Language, LanguageDocument, LanguageRole};
use crate::model::{Constant, FileGraphBuilder};

/// JSON és JSONC fájlokat a közös dokumentum-gráfba képező nyelv.
#[derive(Debug, Default, Clone, Copy)]
pub struct JsonDocument;

impl JsonDocument {
    /// Alapértelmezett, külső függőség nélküli parser-összeállítás.
    pub fn with_defaults() -> Self {
        Self
    }

    fn line(offset: usize, starts: &[usize]) -> u32 {
        starts.partition_point(|&start| start <= offset).max(1) as u32
    }

    fn starts(text: &str) -> Vec<usize> {
        std::iter::once(0)
            .chain(text.match_indices('\n').map(|(i, _)| i + 1))
            .collect()
    }

    fn value_type(value: &Value<'_>) -> &'static str {
        match value {
            Value::StringLit(_) => "string",
            Value::NumberLit(_) => "number",
            Value::BooleanLit(_) => "boolean",
            Value::Object(_) => "object",
            Value::Array(_) => "array",
            Value::NullKeyword(_) => "null",
        }
    }

    fn scalar(value: &Value<'_>) -> Option<String> {
        match value {
            Value::StringLit(value) => Some(value.value.to_string()),
            _ => None,
        }
    }

    fn walk(
        value: &Value<'_>,
        path: &str,
        source: &str,
        starts: &[usize],
        b: &mut FileGraphBuilder,
    ) {
        if let Value::Object(object) = value {
            for property in &object.properties {
                let key = property.name.as_str();
                let child_path = if path.is_empty() {
                    key.to_owned()
                } else {
                    format!("{path}.{key}")
                };
                b.constant(Constant {
                    js: false,
                    container: Some(source.to_owned()),
                    kind: "key".into(),
                    name: child_path.clone(),
                    line: Self::line(property.name.range().start, starts),
                    value: Some(Self::value_type(&property.value).into()),
                    visibility: Some("public".into()),
                    caller: None,
                    doc: None,
                });
                if (key == "$ref" || key == "extends")
                    && let Some(target) = Self::scalar(&property.value)
                {
                    b.import(key.to_owned(), target);
                }
                if Self::imports_context(path)
                    && let Some(version) = Self::scalar(&property.value)
                {
                    b.import(key.to_owned(), version);
                }
                Self::walk(&property.value, &child_path, source, starts, b);
            }
        } else if let Value::Array(array) = value {
            for child in &array.elements {
                Self::walk(child, path, source, starts, b);
            }
        }
    }

    fn imports_context(path: &str) -> bool {
        matches!(
            path,
            "dependencies" | "devDependencies" | "require" | "require-dev"
        )
    }
}

impl Language for JsonDocument {
    fn id(&self) -> &'static str {
        "json"
    }
    fn role(&self) -> LanguageRole {
        LanguageRole::Document
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["json"]
    }

    fn analyze(&self, file: &SourceFile) -> Result<Analysis> {
        let text = String::from_utf8_lossy(&file.text);
        let starts = Self::starts(&text);
        let mut b = FileGraphBuilder::new(file.path_string());
        let name = file
            .path
            .file_name()
            .and_then(|x| x.to_str())
            .unwrap_or_default()
            .to_owned();
        b.container(crate::model::Container {
            kind: "document".into(),
            name: name.clone(),
            line: 1,
            end_line: text.lines().count().max(1) as u32,
            is_abstract: Some(false),
            is_final: Some(false),
            extends: Some(vec![]),
            implements: Some(vec![]),
            traits: Some(vec![]),
            doc: None,
        });
        let parsed = parse_to_ast(
            &text,
            &CollectOptions {
                comments: CommentCollectionStrategy::Separate,
                tokens: true,
            },
            &ParseOptions::default(),
        );
        let warnings = match parsed {
            Ok(parsed) => {
                if let Some(value) = parsed.value {
                    Self::walk(&value, "", &name, &starts, &mut b);
                }
                Vec::new()
            }
            Err(error) => {
                let line = u32::try_from(error.line_display()).unwrap_or(u32::MAX);
                b.parse_error(crate::model::ParseError::new(
                    line,
                    &error.kind().to_string(),
                ));
                vec![format!("JSONC parse warning: {error}")]
            }
        };
        Ok(Analysis {
            graph: b.build(),
            warnings,
        })
    }
}

impl LanguageDocument for JsonDocument {
    fn resolve_reference(&self, reference: &str, from: &Path) -> Option<PathBuf> {
        let target = reference
            .split_once('#')
            .map_or(reference, |(path, _)| path);
        if target.is_empty() || target.contains("://") {
            return None;
        }
        let path = from.parent()?.join(target);
        path.exists().then(|| path.canonicalize().unwrap_or(path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexes_nested_keys_and_jsonc() {
        let file = SourceFile::new(
            "a.json",
            br#"{ // c
          "scripts": { "build": true }, "items": [{"name":"x"}]
        }"#
            .to_vec(),
            None,
        );
        let graph = JsonDocument.analyze(&file).unwrap().graph;
        assert_eq!(
            graph
                .constants
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>(),
            ["scripts", "scripts.build", "items", "items.name"]
        );
    }

    #[test]
    fn syntax_error_is_recorded_with_its_line() {
        let file = SourceFile::new("b.json", b"{\n  \"a\": 1,\n  \"b\": ]\n}\n".to_vec(), None);
        let graph = JsonDocument.analyze(&file).unwrap().graph;
        assert_eq!(graph.parse_errors.len(), 1);
        assert_eq!(graph.parse_errors[0].line, 3, "{:?}", graph.parse_errors);
        let ok = SourceFile::new("c.json", b"{\"a\": 1}\n".to_vec(), None);
        assert!(JsonDocument.analyze(&ok).unwrap().graph.parse_errors.is_empty());
    }

    #[test]
    fn json_golden_rows_match() {
        let mut dir = Some(std::path::Path::new(env!("CARGO_MANIFEST_DIR")));
        let Some(reference) = (loop {
            let Some(current) = dir else { break None };
            match std::fs::read_to_string(current.join("golden/ref-json.jsonl")) {
                Ok(value) => break Some(value),
                Err(_) => dir = current.parent(),
            }
        }) else {
            return;
        };
        for line in reference.lines().filter(|line| !line.trim().is_empty()) {
            let row: serde_json::Value = serde_json::from_str(line).unwrap();
            let path = std::path::Path::new(row["path"].as_str().unwrap());
            let graph = JsonDocument
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
