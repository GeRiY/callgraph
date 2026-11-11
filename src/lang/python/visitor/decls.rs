//! Modul- és osztály-szint: a modul-container, az osztályok (dekorátorokkal) és a törzs utasításainak szétosztása.

use tree_sitter::Node;

use super::super::parser::{end_line, line, named_children};
use super::{Ctx, PythonVisitor, Scope, clip};
use crate::model::{Container, FileGraphBuilder, Method};

/// Összetett utasítások: a belsejükben is lehet deklaráció (`if TYPE_CHECKING:`, `try:`, `if __name__…`).
const COMPOUND: &[&str] = &[
    "if_statement",
    "elif_clause",
    "else_clause",
    "try_statement",
    "except_clause",
    "except_group_clause",
    "finally_clause",
    "with_statement",
    "for_statement",
    "while_statement",
    "match_statement",
    "case_clause",
    "block",
];

impl<'s> PythonVisitor<'_, 's> {
    /// A modul-container (`<fájlnév>`, `type: module`), a deklarációk és a modul-szintű utasítások.
    pub(super) fn module(&mut self, root: Node<'_>, b: &mut FileGraphBuilder) {
        let name = self.file_name.clone();
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
            doc: self.docstring(root).map(|(_, doc)| doc),
        });
        let ctx = Ctx {
            container: name.clone(),
            in_class: false,
        };
        let mut pending = Vec::new();
        for node in named_children(root) {
            self.statement(node, &ctx, &mut pending, b);
        }
        self.pending_statements(&pending, &ctx, b);
    }

    /// Egy utasítás: deklaráció felvétele, vagy a `pending`-be (hívásokat kereső bejáráshoz).
    pub(super) fn statement<'t>(
        &mut self,
        node: Node<'t>,
        ctx: &Ctx,
        pending: &mut Vec<Node<'t>>,
        b: &mut FileGraphBuilder,
    ) {
        match node.kind() {
            "import_statement"
            | "import_from_statement"
            | "future_import_statement"
            | "comment"
            | "pass_statement" => {}
            "class_definition" => self.class(node, node, &[], ctx, b),
            "function_definition" => self.function(node, node, &[], ctx, b),
            "decorated_definition" => {
                let decorators: Vec<Node<'_>> = named_children(node)
                    .into_iter()
                    .filter(|c| c.kind() == "decorator")
                    .collect();
                match node.child_by_field_name("definition") {
                    Some(def) if def.kind() == "class_definition" => {
                        self.class(def, node, &decorators, ctx, b)
                    }
                    Some(def) if def.kind() == "function_definition" => {
                        self.function(def, node, &decorators, ctx, b)
                    }
                    _ => {}
                }
            }
            "expression_statement" => {
                let inner = named_children(node);
                match inner.first() {
                    Some(a) if a.kind() == "assignment" => {
                        self.assignment(*a, ctx, b);
                        pending.push(node);
                    }
                    Some(s) if matches!(s.kind(), "string" | "concatenated_string") => {}
                    _ => pending.push(node),
                }
            }
            kind if COMPOUND.contains(&kind) => {
                for child in named_children(node) {
                    self.statement(child, ctx, pending, b);
                }
            }
            _ => pending.push(node),
        }
    }

    /// Egy osztály containerként (`class`), a törzs tagjaival; beágyazott osztálynál `Külső.Belső`.
    fn class(
        &mut self,
        def: Node<'_>,
        whole: Node<'_>,
        decorators: &[Node<'_>],
        ctx: &Ctx,
        b: &mut FileGraphBuilder,
    ) {
        let Some(name_node) = def.child_by_field_name("name") else {
            return;
        };
        let base = self.text(name_node);
        let name = if ctx.in_class {
            format!("{}.{base}", ctx.container)
        } else {
            base.to_string()
        };
        let bases: Vec<String> = def
            .child_by_field_name("superclasses")
            .map(|args| {
                named_children(args)
                    .into_iter()
                    .filter(|c| !matches!(c.kind(), "keyword_argument" | "comment"))
                    .map(|c| clip(self.text(c), 60))
                    .collect()
            })
            .unwrap_or_default();
        let body = def.child_by_field_name("body");
        b.container(Container {
            kind: "class".to_string(),
            name: name.clone(),
            line: line(whole),
            end_line: end_line(whole),
            is_abstract: Some(bases.iter().any(|x| x == "ABC" || x == "abc.ABC")),
            is_final: Some(false),
            extends: Some(bases),
            implements: Some(Vec::new()),
            traits: Some(self.decorator_texts(decorators)),
            doc: body.and_then(|x| self.docstring(x)).map(|(_, doc)| doc),
        });
        let Some(body) = body else { return };
        let inner = Ctx {
            container: name.clone(),
            in_class: true,
        };
        let scope = Scope {
            container: Some(name),
            caller: "__top__".to_string(),
        };
        let outer = Scope {
            container: Some(ctx.container.clone()),
            caller: "__top__".to_string(),
        };
        for decorator in decorators {
            self.walk_body(*decorator, &outer, b);
        }
        let mut pending = Vec::new();
        for node in named_children(body) {
            self.statement(node, &inner, &mut pending, b);
        }
        for node in pending {
            self.walk_body(node, &scope, b);
        }
    }

    /// A modul-szintű utasítások hívásai a `__module__` pszeudo-metódus alatt (a tagok osztályszinten `__top__`).
    fn pending_statements(&mut self, pending: &[Node<'_>], ctx: &Ctx, b: &mut FileGraphBuilder) {
        let (Some(first), Some(last)) = (pending.first(), pending.last()) else {
            return;
        };
        let (start, end) = (line(*first), end_line(*last));
        b.method(Method {
            container: Some(ctx.container.clone()),
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
            container: Some(ctx.container.clone()),
            caller: "__module__".to_string(),
        };
        for node in pending {
            self.walk_body(*node, &scope, b);
        }
    }

    /// A dekorátorok forrása (`@dataclass(frozen=True)`), ≤60 karakter.
    pub(super) fn decorator_texts(&self, decorators: &[Node<'_>]) -> Vec<String> {
        decorators.iter().map(|d| clip(self.text(*d), 60)).collect()
    }

    /// A törzs első utasítása, ha önálló string: `(a node, a docstring szövege ≤300 karakter)`.
    pub(super) fn docstring<'t>(&self, body: Node<'t>) -> Option<(Node<'t>, String)> {
        let first = named_children(body)
            .into_iter()
            .find(|c| c.kind() != "comment")?;
        let string = named_children(first)
            .into_iter()
            .next()
            .filter(|s| first.kind() == "expression_statement" && s.kind() == "string")?;
        let text: String = named_children(string)
            .iter()
            .filter(|c| c.kind() == "string_content")
            .map(|c| self.text(*c))
            .collect();
        let lines: Vec<&str> = text.lines().map(str::trim).collect();
        let doc = lines.join("\n").trim().to_string();
        (!doc.is_empty()).then(|| (first, doc.chars().take(300).collect()))
    }
}
