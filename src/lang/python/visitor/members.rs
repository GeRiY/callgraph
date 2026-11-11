//! Tagok: `def` / `async def` → függvény vagy metódus (szignatúrával), értékadás → konstans vagy property.

use tree_sitter::Node;

use super::super::parser::{children, end_line, line, named_children};
use super::{Ctx, PythonVisitor, Scope, clip, visibility};
use crate::model::{Constant, FileGraphBuilder, Method, Property};

/// `MAX_SIZE`: van betű, és nincs kisbetű; az `__all__` kivétel, az is konstans.
fn is_const_name(name: &str) -> bool {
    name == "__all__"
        || (name.chars().any(char::is_alphabetic) && !name.chars().any(char::is_lowercase))
}

impl PythonVisitor<'_, '_> {
    /// Egy `def` / `async def`: `whole` a dekorátorokkal együtt vett node (a sortartomány innen jön).
    pub(super) fn function(
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
        let base = self.text(name_node).to_string();
        let is_async = children(def).iter().any(|c| c.kind() == "async");
        let marks: Vec<String> = decorators
            .iter()
            .map(|d| {
                let t = self.text(*d).trim_start_matches('@').trim();
                t.split('(').next().unwrap_or(t).trim().to_string()
            })
            .collect();
        let has = |word: &str| {
            marks
                .iter()
                .any(|m| m == word || m.ends_with(&format!(".{word}")))
        };
        let (is_static, base_kind) = if !ctx.in_class {
            (false, "function")
        } else if has("staticmethod") {
            (true, "staticmethod")
        } else if has("classmethod") {
            (true, "classmethod")
        } else if has("property") || has("cached_property") || has("setter") || has("getter") {
            (false, "property")
        } else {
            (false, "method")
        };
        let kind = if is_async {
            format!("async {base_kind}")
        } else {
            base_kind.to_string()
        };
        let mut params = self.params(def);
        if ctx.in_class
            && base_kind != "staticmethod"
            && params
                .first()
                .is_some_and(|p| matches!(p.as_str(), "self" | "cls" | "mcs"))
        {
            params.remove(0);
        }
        let return_type = def
            .child_by_field_name("return_type")
            .map(|t| clip(self.text(t), 60));
        let key = (ctx.container.clone(), base.clone());
        let name = if self.taken.contains(&key) {
            format!("{base}@L{}", line(whole))
        } else {
            base.clone()
        };
        self.taken.insert(key);
        let body = def.child_by_field_name("body");
        let doc = body.and_then(|x| self.docstring(x));
        b.method(Method {
            container: Some(ctx.container.clone()),
            name: name.clone(),
            line: line(whole),
            end_line: Some(end_line(whole)),
            visibility: Some(visibility(&base).to_string()),
            is_static,
            is_abstract: has("abstractmethod"),
            kind: Some(kind),
            params,
            return_type,
            doc_types: None,
            doc: doc.as_ref().map(|(_, d)| d.clone()),
            body: None,
        });
        let outer = Scope {
            container: Some(ctx.container.clone()),
            caller: "__top__".to_string(),
        };
        for decorator in decorators {
            self.walk_body(*decorator, &outer, b);
        }
        let Some(body) = body else { return };
        let scope = Scope {
            container: Some(ctx.container.clone()),
            caller: name,
        };
        let skip = doc.map(|(node, _)| node.id());
        for statement in named_children(body) {
            if Some(statement.id()) != skip {
                self.walk_body(statement, &scope, b);
            }
        }
    }

    /// A paraméterek forrása (≤60 kar.): `a: int`, `b: str = "x"`, `*args`, `**kw`, `/`, `*`.
    fn params(&self, def: Node<'_>) -> Vec<String> {
        def.child_by_field_name("parameters")
            .map(|p| {
                named_children(p)
                    .into_iter()
                    .filter(|c| c.kind() != "comment")
                    .map(|c| clip(self.text(c), 60))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// `NAME = …` / `name: T = …`: modulban konstans (NAGYBETŰS vagy típusozott), osztályban konstans vagy property.
    pub(super) fn assignment(&mut self, node: Node<'_>, ctx: &Ctx, b: &mut FileGraphBuilder) {
        let Some(left) = node.child_by_field_name("left") else {
            return;
        };
        if left.kind() != "identifier" {
            return;
        }
        let name = self.text(left).to_string();
        let declared = node
            .child_by_field_name("type")
            .map(|t| clip(self.text(t), 60));
        let value = node.child_by_field_name("right");
        let container = Some(ctx.container.clone());
        let constant = is_const_name(&name) || (!ctx.in_class && declared.is_some());
        if constant {
            b.constant(Constant {
                container,
                kind: "const".to_string(),
                name: name.clone(),
                line: line(node),
                value: value.map(|v| clip(self.text(v), 80)),
                visibility: Some(visibility(&name).to_string()),
                caller: None,
                doc: None,
                js: false,
            });
        } else if ctx.in_class {
            b.property(Property {
                container,
                type_hint: declared.or_else(|| value.and_then(|v| self.infer_type(v))),
                visibility: visibility(&name).to_string(),
                name,
                line: line(node),
                is_static: false,
                readonly: false,
                doc: None,
                js: false,
            });
        }
    }

    /// Egyszerű típus-következtetés az inicializálóból (literálok, `Foo()`).
    fn infer_type(&self, value: Node<'_>) -> Option<String> {
        let literal = match value.kind() {
            "string" | "concatenated_string" => "str",
            "integer" => "int",
            "float" => "float",
            "true" | "false" => "bool",
            "list" => "list",
            "dictionary" => "dict",
            "set" => "set",
            "tuple" => "tuple",
            "call" => {
                let callee = self.text(value.child_by_field_name("function")?);
                return callee
                    .chars()
                    .next()
                    .is_some_and(char::is_uppercase)
                    .then(|| callee.to_string());
            }
            _ => return None,
        };
        Some(literal.to_string())
    }
}
