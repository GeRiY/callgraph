//! Builder minta: a PHP-fájl összeállítása lépésenként (`strict_types`, névtér, rendezett importok, törzs), a
//! szakaszok között egy-egy üres sorral. A névtér és az importok a projekt-ismeretből jönnek (`Generator`).

/// A PHP-fájl építője.
#[derive(Debug, Default, Clone)]
pub struct PhpFileBuilder {
    strict_types: bool,
    namespace: Option<String>,
    imports: Vec<(String, Option<String>)>,
    body: Vec<String>,
}

impl PhpFileBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// `declare(strict_types=1);` a fejlécben.
    pub fn strict_types(mut self, on: bool) -> Self {
        self.strict_types = on;
        self
    }

    pub fn namespace(mut self, namespace: Option<String>) -> Self {
        self.namespace = namespace.filter(|n| !n.is_empty());
        self
    }

    /// Egy `use` sor; ugyanaz kétszer nem kerül be. `alias`: `use A\B as C;`.
    pub fn import(mut self, fqcn: &str, alias: Option<&str>) -> Self {
        let entry = (fqcn.to_owned(), alias.map(str::to_owned));
        if !self.imports.contains(&entry) {
            self.imports.push(entry);
        }
        self
    }

    /// A deklarációk sorai (az emitter lusta iterátora ide fogyasztódik).
    pub fn body(mut self, lines: impl Iterator<Item = String>) -> Self {
        self.body.extend(lines);
        self
    }

    /// A rendezett `use` sorok (kis-nagybetűtől függetlenül rendezve).
    pub fn import_lines(&self) -> Vec<String> {
        let mut sorted = self.imports.clone();
        sorted.sort_by_key(|(fqcn, _)| fqcn.to_ascii_lowercase());
        sorted
            .into_iter()
            .map(|(fqcn, alias)| match alias {
                Some(a) => format!("use {fqcn} as {a};"),
                None => format!("use {fqcn};"),
            })
            .collect()
    }

    /// A teljes fájl szövege, záró újsorral.
    pub fn build(self) -> String {
        let mut sections: Vec<Vec<String>> = vec![vec!["<?php".to_owned()]];
        if self.strict_types {
            sections.push(vec!["declare(strict_types=1);".to_owned()]);
        }
        if let Some(ns) = &self.namespace {
            sections.push(vec![format!("namespace {ns};")]);
        }
        let uses = self.import_lines();
        if !uses.is_empty() {
            sections.push(uses);
        }
        sections.push(self.body);
        let mut text = sections
            .iter()
            .filter(|s| !s.is_empty())
            .map(|s| s.join("\n"))
            .collect::<Vec<_>>()
            .join("\n\n");
        text.push('\n');
        text
    }
}
