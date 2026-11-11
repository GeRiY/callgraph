//! Fájl-szint: a modul-container, a típus-deklarációk (struct, interface, alias, definiált típus) és a
//! csomag-szintű konstansok / változók.

use std::collections::BTreeMap;

use tree_sitter::Node;

use super::super::parser::{end_line, line, named_children};
use super::{GoVisitor, Scope, clip, visibility};
use crate::model::{Constant, Container, FileGraphBuilder, Method, Property};

impl<'s> GoVisitor<'_, 's> {
    /// A modul-container (`<fájlnév>`, `type: module`), a receiver-only típusok, a deklarációk és a
    /// csomag-szintű változó-inicializálók.
    pub(super) fn module(&mut self, root: Node<'_>, b: &mut FileGraphBuilder) {
        let name = self.file_name.clone();
        let package_doc = named_children(root)
            .into_iter()
            .find(|c| c.kind() == "package_clause")
            .and_then(|c| self.doc_of(c));
        b.container(Container {
            kind: "module".to_string(),
            name: name.clone(),
            line: 1,
            end_line: end_line(root).max(1),
            is_abstract: Some(false),
            is_final: Some(false),
            extends: Some(Vec::new()),
            implements: Some(Vec::new()),
            traits: Some(Vec::new()),
            doc: package_doc,
        });
        self.foreign_receivers(root, b);
        let mut pending = Vec::new();
        for node in named_children(root) {
            match node.kind() {
                "function_declaration" => self.function(node, b),
                "method_declaration" => self.method(node, b),
                "type_declaration" => self.type_declaration(node, b),
                "const_declaration" => {
                    self.value_specs(node, "const", "const_spec", b);
                    pending.push(node);
                }
                "var_declaration" => {
                    self.value_specs(node, "static", "var_spec", b);
                    pending.push(node);
                }
                _ => {}
            }
        }
        self.pending_statements(&pending, b);
    }

    /// Azok a receiver-típusok, amelyeket nem ez a fájl deklarál (a típus másik fájlban van): egy
    /// `extension` container fogja össze a metódusaikat, hogy az outline-ban legyen gazdájuk.
    fn foreign_receivers(&mut self, root: Node<'_>, b: &mut FileGraphBuilder) {
        let mut foreign: BTreeMap<String, (u32, u32)> = BTreeMap::new();
        for node in named_children(root) {
            if node.kind() != "method_declaration" {
                continue;
            }
            let (Some(ty), _) = self.receiver(node) else {
                continue;
            };
            if self.symbols.types.contains(&ty) {
                continue;
            }
            let span = foreign.entry(ty).or_insert((line(node), end_line(node)));
            span.0 = span.0.min(line(node));
            span.1 = span.1.max(end_line(node));
        }
        for (name, (start, end)) in foreign {
            b.container(Container {
                kind: "extension".to_string(),
                name,
                line: start,
                end_line: end,
                is_abstract: Some(false),
                is_final: Some(false),
                extends: Some(Vec::new()),
                implements: Some(Vec::new()),
                traits: Some(Vec::new()),
                doc: None,
            });
        }
    }

    /// `type X …` / `type ( X …; Y … )`: minden spec egy container.
    fn type_declaration(&mut self, node: Node<'_>, b: &mut FileGraphBuilder) {
        let specs: Vec<Node<'_>> = named_children(node)
            .into_iter()
            .filter(|c| matches!(c.kind(), "type_spec" | "type_alias"))
            .collect();
        // egyetlen spec: a deklaráció egésze (a `type` kulcsszótól), csoportnál a spec saját sora
        let single = specs.len() == 1;
        for spec in &specs {
            let whole = if single { node } else { *spec };
            self.type_spec(*spec, whole, b);
        }
    }

    fn type_spec(&mut self, spec: Node<'_>, whole: Node<'_>, b: &mut FileGraphBuilder) {
        let Some(name_node) = spec.child_by_field_name("name") else {
            return;
        };
        let name = self.text(name_node).to_string();
        let ty = spec.child_by_field_name("type");
        let generics: Vec<String> = spec
            .child_by_field_name("type_parameters")
            .map(|t| clip(self.text(t), 60))
            .into_iter()
            .collect();
        let doc = self.doc_of(whole).or_else(|| self.doc_of(spec));
        let (start, end) = (line(whole), end_line(whole));
        match ty {
            Some(t) if t.kind() == "struct_type" => {
                self.struct_type(&name, t, generics, doc, (start, end), b)
            }
            Some(t) if t.kind() == "interface_type" => {
                self.interface_type(&name, t, generics, doc, (start, end), b)
            }
            _ => {
                let kind = if spec.kind() == "type_alias" {
                    "alias"
                } else {
                    "type"
                };
                b.container(Container {
                    kind: kind.to_string(),
                    name,
                    line: start,
                    end_line: end,
                    is_abstract: Some(false),
                    is_final: Some(false),
                    extends: Some(ty.map(|t| vec![clip(self.text(t), 60)]).unwrap_or_default()),
                    implements: Some(Vec::new()),
                    traits: Some(generics),
                    doc,
                });
            }
        }
    }

    /// Struct: `class` container; a mezők property-k, a beágyazott típusok (`extends`) az „ősök”.
    fn struct_type(
        &mut self,
        name: &str,
        node: Node<'_>,
        generics: Vec<String>,
        doc: Option<String>,
        (start, end): (u32, u32),
        b: &mut FileGraphBuilder,
    ) {
        let mut embedded = Vec::new();
        let mut properties = Vec::new();
        let fields = named_children(node)
            .into_iter()
            .find(|c| c.kind() == "field_declaration_list");
        for field in fields.map(named_children).unwrap_or_default() {
            if field.kind() != "field_declaration" {
                continue;
            }
            let type_text = field
                .child_by_field_name("type")
                .map(|t| clip(self.text(t), 60));
            let mut cursor = field.walk();
            let names: Vec<Node<'_>> = field.children_by_field_name("name", &mut cursor).collect();
            if names.is_empty() {
                if let Some(t) = type_text {
                    embedded.push(t.trim_start_matches('*').to_string());
                }
                continue;
            }
            let field_doc = self.doc_of(field);
            for n in names {
                let field_name = self.text(n).to_string();
                properties.push(Property {
                    container: Some(name.to_string()),
                    visibility: visibility(&field_name).to_string(),
                    name: field_name,
                    line: line(n),
                    type_hint: type_text.clone(),
                    is_static: false,
                    readonly: false,
                    doc: field_doc.clone(),
                    js: false,
                });
            }
        }
        b.container(Container {
            kind: "class".to_string(),
            name: name.to_string(),
            line: start,
            end_line: end,
            is_abstract: Some(false),
            is_final: Some(false),
            extends: Some(embedded),
            implements: Some(Vec::new()),
            traits: Some(generics),
            doc,
        });
        for property in properties {
            b.property(property);
        }
    }

    /// Interfész: `interface` container; a metódus-elemek absztrakt metódusok, a beágyazott
    /// interfészek / típus-halmazok az `extends`-ben.
    fn interface_type(
        &mut self,
        name: &str,
        node: Node<'_>,
        generics: Vec<String>,
        doc: Option<String>,
        (start, end): (u32, u32),
        b: &mut FileGraphBuilder,
    ) {
        let mut extends = Vec::new();
        let mut methods = Vec::new();
        for elem in named_children(node) {
            match elem.kind() {
                "method_elem" => methods.push(elem),
                "type_elem" => extends.push(clip(self.text(elem), 60)),
                _ => {}
            }
        }
        b.container(Container {
            kind: "interface".to_string(),
            name: name.to_string(),
            line: start,
            end_line: end,
            is_abstract: Some(true),
            is_final: Some(false),
            extends: Some(extends),
            implements: Some(Vec::new()),
            traits: Some(generics),
            doc,
        });
        for elem in methods {
            let Some(method_name) = elem.child_by_field_name("name") else {
                continue;
            };
            let method_name = self.text(method_name).to_string();
            b.method(Method {
                container: Some(name.to_string()),
                visibility: Some(visibility(&method_name).to_string()),
                name: method_name,
                line: line(elem),
                end_line: Some(end_line(elem)),
                is_static: false,
                is_abstract: true,
                kind: Some("method".to_string()),
                params: self.params(elem),
                return_type: self.result(elem),
                doc_types: None,
                doc: self.doc_of(elem),
                body: None,
            });
        }
    }

    /// `const` / `var` blokk: minden megnevezett érték egy konstans (a `_` kimarad).
    fn value_specs(
        &mut self,
        decl: Node<'_>,
        kind: &str,
        spec_kind: &str,
        b: &mut FileGraphBuilder,
    ) {
        let mut specs = Vec::new();
        for child in named_children(decl) {
            if child.kind() == spec_kind {
                specs.push(child);
            } else if child.kind() == "var_spec_list" {
                specs.extend(
                    named_children(child)
                        .into_iter()
                        .filter(|s| s.kind() == spec_kind),
                );
            }
        }
        for spec in specs {
            let mut cursor = spec.walk();
            let names: Vec<Node<'_>> = spec.children_by_field_name("name", &mut cursor).collect();
            let values = spec
                .child_by_field_name("value")
                .map(named_children)
                .unwrap_or_default();
            let doc = self.doc_of(spec);
            for (index, n) in names.into_iter().enumerate() {
                let name = self.text(n).to_string();
                if name == "_" {
                    continue;
                }
                b.constant(Constant {
                    container: Some(self.file_name.clone()),
                    kind: kind.to_string(),
                    visibility: Some(visibility(&name).to_string()),
                    name,
                    line: line(n),
                    value: values.get(index).map(|v| clip(self.text(*v), 80)),
                    caller: None,
                    doc: doc.clone(),
                    js: false,
                });
            }
        }
    }

    /// A csomag-szintű `const` / `var` inicializálók hívásai a `__module__` pszeudo-metódus alatt.
    fn pending_statements(&mut self, pending: &[Node<'_>], b: &mut FileGraphBuilder) {
        let (Some(first), Some(last)) = (pending.first(), pending.last()) else {
            return;
        };
        let (start, end) = (line(*first), end_line(*last));
        b.method(Method {
            container: Some(self.file_name.clone()),
            name: "__module__".to_string(),
            line: start,
            end_line: Some(end),
            visibility: Some("public".to_string()),
            is_static: false,
            is_abstract: false,
            kind: Some("top-level".to_string()),
            params: Vec::new(),
            return_type: None,
            doc_types: None,
            doc: None,
            body: None,
        });
        let scope = Scope {
            container: Some(self.file_name.clone()),
            caller: "__module__".to_string(),
            receiver: None,
        };
        for node in pending {
            self.walk_body(*node, &scope, b);
        }
    }

    /// A deklaráció elé írt, közvetlenül mellette álló `//` / `/* */` megjegyzések szövege (≤300 karakter).
    pub(super) fn doc_of(&self, node: Node<'_>) -> Option<String> {
        let mut parts: Vec<String> = Vec::new();
        let mut next_row = node.start_position().row;
        let mut prev = node.prev_sibling();
        while let Some(comment) = prev {
            if comment.kind() != "comment" || comment.end_position().row + 1 != next_row {
                break;
            }
            let raw = self.text(comment);
            // fordítói direktívák (`//go:generate`, `//nolint`) nem dokumentáció
            if !(raw.starts_with("//go:") || raw.starts_with("//nolint")) {
                parts.push(strip_comment(raw));
            }
            next_row = comment.start_position().row;
            prev = comment.prev_sibling();
        }
        parts.reverse();
        let doc = parts.join("\n").trim().to_string();
        (!doc.is_empty()).then(|| doc.chars().take(300).collect())
    }
}

/// A megjegyzés jelei (`//`, `/* */`, sor eleji `*`) nélküli szöveg.
fn strip_comment(raw: &str) -> String {
    let raw = raw.trim();
    if let Some(rest) = raw.strip_prefix("//") {
        return rest.trim().to_string();
    }
    let inner = raw
        .strip_prefix("/*")
        .and_then(|r| r.strip_suffix("*/"))
        .unwrap_or(raw);
    inner
        .lines()
        .map(|l| l.trim().trim_start_matches('*').trim())
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}
