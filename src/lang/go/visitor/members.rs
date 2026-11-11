//! Tagok: `func` → függvény, receiveres `func` → a receiver-típus metódusa (szignatúrával).

use tree_sitter::Node;

use super::super::parser::{end_line, line, named_children};
use super::{GoVisitor, Scope, clip, visibility};
use crate::model::{FileGraphBuilder, Method};

impl GoVisitor<'_, '_> {
    /// Csomag-szintű `func name(…) …`: a modul-container függvénye.
    pub(super) fn function(&mut self, def: Node<'_>, b: &mut FileGraphBuilder) {
        let Some(name_node) = def.child_by_field_name("name") else {
            return;
        };
        let base = self.text(name_node).to_string();
        let container = self.file_name.clone();
        self.callable(def, &container, &base, "function", None, b);
    }

    /// `func (r *Repo) Find(…) …`: a `Repo` típus metódusa (a pointer és a típusparaméter nem része a névnek).
    pub(super) fn method(&mut self, def: Node<'_>, b: &mut FileGraphBuilder) {
        let Some(name_node) = def.child_by_field_name("name") else {
            return;
        };
        let (Some(owner), receiver) = self.receiver(def) else {
            return;
        };
        let base = self.text(name_node).to_string();
        self.callable(def, &owner, &base, "method", receiver, b);
    }

    /// A receiver típusa (pointer és típusparaméterek nélkül) és a változó neve (`_` / névtelen: `None`).
    pub(super) fn receiver(&self, def: Node<'_>) -> (Option<String>, Option<String>) {
        let Some(list) = def.child_by_field_name("receiver") else {
            return (None, None);
        };
        let Some(decl) = named_children(list)
            .into_iter()
            .find(|c| c.kind() == "parameter_declaration")
        else {
            return (None, None);
        };
        let owner = decl.child_by_field_name("type").and_then(|t| {
            let text = self.text(t).trim().trim_start_matches('*').trim();
            let base = text.split('[').next().unwrap_or(text).trim();
            (!base.is_empty()).then(|| base.to_string())
        });
        let name = decl
            .child_by_field_name("name")
            .map(|n| self.text(n).to_string())
            .filter(|n| n != "_");
        (owner, name)
    }

    fn callable(
        &mut self,
        def: Node<'_>,
        container: &str,
        base: &str,
        kind: &str,
        receiver: Option<String>,
        b: &mut FileGraphBuilder,
    ) {
        let key = (container.to_string(), base.to_string());
        let name = if self.taken.contains(&key) {
            format!("{base}@L{}", line(def))
        } else {
            base.to_string()
        };
        self.taken.insert(key);
        b.method(Method {
            container: Some(container.to_string()),
            name: name.clone(),
            line: line(def),
            end_line: Some(end_line(def)),
            visibility: Some(visibility(base).to_string()),
            is_static: false,
            is_abstract: false,
            kind: Some(kind.to_string()),
            params: self.params(def),
            return_type: self.result(def),
            doc_types: None,
            doc: self.doc_of(def),
            body: None,
        });
        let Some(body) = def.child_by_field_name("body") else {
            return;
        };
        let scope = Scope {
            container: Some(container.to_string()),
            caller: name,
            receiver,
        };
        self.walk_body(body, &scope, b);
    }

    /// A paraméterek forrása (≤60 kar.): `id int`, `a, b string`, `args ...int`; a receiver nem tartozik ide.
    pub(super) fn params(&self, def: Node<'_>) -> Vec<String> {
        def.child_by_field_name("parameters")
            .map(|p| {
                named_children(p)
                    .into_iter()
                    .filter(|c| {
                        matches!(
                            c.kind(),
                            "parameter_declaration" | "variadic_parameter_declaration"
                        )
                    })
                    .map(|c| clip(self.text(c), 60))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// A visszatérési típus(ok) forrása (≤80 kar.): `error`, `(*User, error)`, `(n int, err error)`.
    pub(super) fn result(&self, def: Node<'_>) -> Option<String> {
        def.child_by_field_name("result")
            .map(|r| clip(self.text(r), 80))
    }
}
