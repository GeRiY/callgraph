//! Fájl- és típus-szint: importok, containerek (class / struct / enum / protocol / extension / actor,
//! SwiftUI-nézetnél `component`), a fájl-szintű container és a tagok szétosztása.

use tree_sitter::Node;

use super::super::known::{KNOWN_PROTOCOLS, RAW_TYPES, is_in};
use super::super::parser::{end_line, line, named_children};
use super::{Scope, SwiftVisitor, scan};
use crate::model::{Container, FileGraphBuilder};

impl SwiftVisitor<'_, '_> {
    /// A fájl-szintű függvények / változók / utasítások containere (`<fájlnév>`, `type: module`).
    pub(super) fn module_container(&mut self, root: Node<'_>, b: &mut FileGraphBuilder) {
        let name = self.file_name.clone();
        let end = end_line(root).max(1);
        b.container(Container {
            kind: "module".to_string(),
            name: name.clone(),
            line: 1,
            end_line: end,
            is_abstract: Some(false),
            is_final: Some(false),
            extends: Some(Vec::new()),
            implements: Some(Vec::new()),
            traits: Some(Vec::new()),
            doc: None,
        });
        self.containers.push((name.clone(), 1, end));
        self.module = Some(name);
    }

    /// A gyökér közvetlen gyerekei: import, típus, fájl-szintű tag vagy utasítás.
    pub(super) fn visit_top_level(&mut self, root: Node<'_>, b: &mut FileGraphBuilder) {
        let mut statements: Vec<Node<'_>> = Vec::new();
        for node in named_children(root) {
            match node.kind() {
                "import_declaration" => self.import(node, b),
                "class_declaration" | "protocol_declaration" => self.type_decl(node, None, b),
                "comment" | "multiline_comment" => {}
                _ => {
                    let container = self.module.clone();
                    if !self.member(node, container.as_deref(), false, b) {
                        statements.push(node);
                    }
                }
            }
        }
        self.top_level_statements(&statements, b);
    }

    /// `import SwiftUI` → `SwiftUI: SwiftUI`; `import struct A.B` → `B: A.B`.
    fn import(&mut self, node: Node<'_>, b: &mut FileGraphBuilder) {
        let Some(id) = named_children(node)
            .into_iter()
            .find(|c| c.kind() == "identifier")
        else {
            return;
        };
        let full = self.text(id).to_string();
        let alias = full.rsplit('.').next().unwrap_or(&full).to_string();
        b.import(alias, full);
    }

    /// Egy típus-deklaráció containerként, majd a tagjai (beágyazott típusnál `Külső.Belső`).
    fn type_decl(&mut self, node: Node<'_>, outer: Option<&str>, b: &mut FileGraphBuilder) {
        let keyword = node
            .child_by_field_name("declaration_kind")
            .map(|k| k.kind().to_string())
            .unwrap_or_else(|| "protocol".to_string());
        let Some(name_node) = node.child_by_field_name("name") else {
            return;
        };
        let base = self.text(name_node).to_string();
        let qualified = match outer {
            Some(o) => format!("{o}.{base}"),
            None => base.clone(),
        };
        let inherited = scan::inherited(self.parsed, node);
        let is_view = inherited.iter().any(|i| i == "View");
        let (name, kind) = if keyword == "extension" {
            let taken = self.containers.iter().any(|(n, _, _)| *n == qualified)
                || (outer.is_none() && self.symbols.types.contains_key(&base));
            let name = if taken {
                format!("{qualified}@L{}", line(node))
            } else {
                qualified.clone()
            };
            (name, "extension".to_string())
        } else if is_view && keyword != "protocol" {
            (qualified.clone(), "component".to_string())
        } else {
            (qualified.clone(), keyword.clone())
        };
        let (extends, implements) = self.split_inheritance(&keyword, &name, &qualified, inherited);
        let modifiers = self.modifier_words(node);
        b.container(Container {
            kind,
            name: name.clone(),
            line: line(node),
            end_line: end_line(node),
            is_abstract: Some(keyword == "protocol"),
            is_final: Some(modifiers.iter().any(|m| m == "final")),
            extends: Some(extends),
            implements: Some(implements),
            traits: Some(Vec::new()),
            doc: self.doc_of(node),
        });
        self.containers
            .push((name.clone(), line(node), end_line(node)));
        let Some(body) = node.child_by_field_name("body") else {
            return;
        };
        let protocol = keyword == "protocol";
        self.type_members(body, &name, &qualified, protocol, b);
    }

    /// A típus-törzs tagjai; a hibás (`ERROR`) részek belsejében is keres deklarációt.
    fn type_members(
        &mut self,
        body: Node<'_>,
        name: &str,
        qualified: &str,
        protocol: bool,
        b: &mut FileGraphBuilder,
    ) {
        for member in named_children(body) {
            match member.kind() {
                "ERROR" => self.type_members(member, name, qualified, protocol, b),
                "class_declaration" | "protocol_declaration" => {
                    self.type_decl(member, Some(qualified), b)
                }
                "enum_entry" => self.enum_cases(member, name, b),
                _ => {
                    self.member(member, Some(name), protocol, b);
                }
            }
        }
    }

    /// `extends` / `implements`: osztálynál az első nem-protokoll az ős, enumnál a nyers típus,
    /// protokollnál minden örökölt protokoll; extension-nél a kiterjesztett típus (ha a név egyedivé vált).
    fn split_inheritance(
        &self,
        keyword: &str,
        name: &str,
        qualified: &str,
        inherited: Vec<String>,
    ) -> (Vec<String>, Vec<String>) {
        let is_protocol = |t: &str| is_in(KNOWN_PROTOCOLS, t) || self.symbols.protocols.contains(t);
        match keyword {
            "protocol" => (inherited, Vec::new()),
            "class" if inherited.first().is_some_and(|f| !is_protocol(f)) => {
                let mut rest = inherited;
                let first = rest.remove(0);
                (vec![first], rest)
            }
            "enum" if inherited.first().is_some_and(|f| is_in(RAW_TYPES, f)) => {
                let mut rest = inherited;
                let first = rest.remove(0);
                (vec![first], rest)
            }
            "extension" if name != qualified => (vec![qualified.to_string()], inherited),
            _ => (Vec::new(), inherited),
        }
    }

    /// Fájl-szintű utasítások (pl. `main.swift`): `__module__` pszeudo-metódus a hívásaikkal.
    fn top_level_statements(&mut self, statements: &[Node<'_>], b: &mut FileGraphBuilder) {
        let (Some(first), Some(last)) = (statements.first(), statements.last()) else {
            return;
        };
        let scope = Scope {
            container: self.module.clone(),
            caller: "__module__".to_string(),
            in_view: false,
        };
        let (start, end) = (line(*first), end_line(*last));
        self.add_method_span(&scope.container, "__module__", start, end);
        b.method(crate::model::Method {
            container: scope.container.clone(),
            name: "__module__".to_string(),
            line: start,
            end_line: Some(end),
            visibility: Some("internal".to_string()),
            is_static: false,
            is_abstract: false,
            kind: Some("top-level".to_string()),
            params: Vec::new(),
            return_type: None,
            doc_types: None,
            doc: None,
            body: None,
        });
        for node in statements {
            self.walk_body(*node, &scope, b);
        }
    }
}
