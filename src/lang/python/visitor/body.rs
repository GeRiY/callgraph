//! Törzs-bejárás: egy függvény / utasítás minden node-ja — hívás-élek és string-literálok
//! (a beágyazott függvények, lambdák és osztályok a külső hívóhoz tartoznak).

use tree_sitter::Node;

use super::super::categorize::CallInfo;
use super::super::parser::{end_line, line, named_children};
use super::{PythonVisitor, Scope, clip};
use crate::model::{Call, FileGraphBuilder, StringLit};

/// A hívott kifejezés felbontása.
struct Callee<'t> {
    name_node: Node<'t>,
    name: String,
    member: bool,
    /// A fogadó pontozott útja, ha csak azonosítókból áll (`self.repo`, `os.path`).
    path: Option<String>,
    /// A lánc bal szélső azonosítója.
    root: Option<String>,
    /// A fogadó maga is hívás (`super().f()`, `a().b()`).
    chained: bool,
}

impl PythonVisitor<'_, '_> {
    /// A node és a leszármazottai bejárása a `scope` hívóval.
    pub(super) fn walk_body(&mut self, node: Node<'_>, scope: &Scope, b: &mut FileGraphBuilder) {
        match node.kind() {
            "call" => self.call(node, scope, b),
            "string" => self.string_literal(node, scope, b),
            "comment" => return,
            _ => {}
        }
        for child in named_children(node) {
            self.walk_body(child, scope, b);
        }
    }

    /// Egy `call` felvétele `Call`-ként.
    fn call(&mut self, node: Node<'_>, scope: &Scope, b: &mut FileGraphBuilder) {
        let Some(expr) = node.child_by_field_name("function") else {
            return;
        };
        let Some(callee) = self.callee(expr) else {
            return;
        };
        let on_self = callee
            .path
            .as_deref()
            .is_some_and(|p| p == "self" || p == "cls");
        let info = CallInfo {
            name: callee.name.clone(),
            member: callee.member,
            root: callee.root.clone(),
            on_self,
            chained: callee.chained,
        };
        let category = self.categorizer.categorize(&info, &self.symbols);
        let kind = if callee.member {
            "method_call"
        } else if callee.name.chars().next().is_some_and(char::is_uppercase) {
            "new"
        } else {
            "func_call"
        };
        let target = match (&callee.path, callee.member) {
            (Some(path), true) => format!("{path}.{}", callee.name),
            (None, true) => format!(".{}", callee.name),
            _ => callee.name.clone(),
        };
        let start = line(callee.name_node);
        let end = end_line(node);
        b.call(Call {
            container: scope.container.clone(),
            caller: Some(scope.caller.clone()),
            line: start,
            end_line: (end > start).then_some(end),
            kind: kind.to_string(),
            target,
            resolved: None,
            on: callee
                .root
                .filter(|_| callee.member && !on_self && callee.path.is_some()),
            category: category.to_string(),
            args: self.args(node),
            handler: None,
        });
    }

    fn callee<'t>(&self, expr: Node<'t>) -> Option<Callee<'t>> {
        match expr.kind() {
            "identifier" => Some(Callee {
                name_node: expr,
                name: self.text(expr).to_string(),
                member: false,
                path: None,
                root: None,
                chained: false,
            }),
            "attribute" => {
                let name_node = expr.child_by_field_name("attribute")?;
                let object = expr.child_by_field_name("object")?;
                let (root, chained) = self.root_of(object);
                Some(Callee {
                    name_node,
                    name: self.text(name_node).to_string(),
                    member: true,
                    path: self.path_of(object),
                    root,
                    chained,
                })
            }
            _ => None,
        }
    }

    /// `a.b.c` alakú út (csak azonosítók).
    fn path_of(&self, node: Node<'_>) -> Option<String> {
        match node.kind() {
            "identifier" => Some(self.text(node).to_string()),
            "attribute" => {
                let object = self.path_of(node.child_by_field_name("object")?)?;
                let attr = node.child_by_field_name("attribute")?;
                Some(format!("{object}.{}", self.text(attr)))
            }
            _ => None,
        }
    }

    /// A lánc bal szélső azonosítója, és hogy hívás(oka)n át értük-e el.
    fn root_of(&self, node: Node<'_>) -> (Option<String>, bool) {
        match node.kind() {
            "identifier" => (Some(self.text(node).to_string()), false),
            "attribute" => match node.child_by_field_name("object") {
                Some(o) => self.root_of(o),
                None => (None, false),
            },
            "call" => match node.child_by_field_name("function") {
                Some(f) => (self.root_of(f).0, true),
                None => (None, true),
            },
            _ => (None, false),
        }
    }

    /// Az első ≤4 argumentum forrása (≤60 kar.); `None`, ha nincs argumentum.
    fn args(&self, node: Node<'_>) -> Option<Vec<String>> {
        let arguments = node.child_by_field_name("arguments")?;
        let args: Vec<String> = named_children(arguments)
            .into_iter()
            .filter(|c| c.kind() != "comment")
            .take(4)
            .map(|a| clip(self.text(a), 60))
            .collect();
        (!args.is_empty()).then_some(args)
    }

    /// String-literál: a szövegrészek, az f-string kifejezések `{…}`-ként (üres / csak szóköz kimarad).
    fn string_literal(&self, node: Node<'_>, scope: &Scope, b: &mut FileGraphBuilder) {
        let mut value = String::new();
        for part in named_children(node) {
            match part.kind() {
                "interpolation" => value.push_str("{…}"),
                "string_content" => value.push_str(self.text(part)),
                _ => {}
            }
        }
        if value.trim().is_empty() {
            return;
        }
        let end = end_line(node);
        b.string(StringLit {
            container: scope.container.clone(),
            caller: Some(scope.caller.clone()),
            line: line(node),
            end_line: (end > line(node)).then_some(end),
            value: value.chars().take(300).collect(),
        });
    }
}
