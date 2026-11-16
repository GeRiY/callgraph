//! Törzs-bejárás: egy függvény / deklaráció minden node-ja — hívás-élek, struct-literálok és string-literálok
//! (a függvény-literálok, a `go` / `defer` hívások a külső hívóhoz tartoznak).

use tree_sitter::Node;

use super::super::categorize::{CallInfo, is_stdlib};
use super::super::parser::{end_line, line, named_children};
use super::{GoVisitor, Scope, clip};
use crate::model::{Call, FileGraphBuilder, StringLit};

/// A hívott kifejezés felbontása.
struct Callee<'t> {
    name_node: Node<'t>,
    name: String,
    member: bool,
    /// A fogadó pontozott útja, ha csak azonosítókból áll (`r.repo`, `http`).
    path: Option<String>,
    /// A lánc bal szélső azonosítója.
    root: Option<String>,
    /// A fogadó maga is hívás (`a().b()`).
    chained: bool,
}

impl GoVisitor<'_, '_> {
    /// A node és a leszármazottai bejárása a `scope` hívóval.
    pub(super) fn walk_body(&mut self, node: Node<'_>, scope: &Scope, b: &mut FileGraphBuilder) {
        match node.kind() {
            "call_expression" => {
                if let Some(expr) = node.child_by_field_name("function") {
                    let args = self.args(node);
                    self.emit_call(expr, node, args, scope, b);
                }
            }
            // `Map[int](xs)`: a grammar generikus típus-konverziónak olvassa — valójában függvényhívás
            "type_conversion_expression" => self.generic_conversion(node, scope, b),
            "composite_literal" => self.literal(node, scope, b),
            "interpreted_string_literal" | "raw_string_literal" => {
                self.string_literal(node, scope, b)
            }
            "comment" => return,
            _ => {}
        }
        for child in named_children(node) {
            self.walk_body(child, scope, b);
        }
    }

    /// Egy `call_expression` felvétele `Call`-ként (a `go f()` és a `defer f()` is ide tartozik).
    fn emit_call(
        &mut self,
        expr: Node<'_>,
        node: Node<'_>,
        args: Option<Vec<String>>,
        scope: &Scope,
        b: &mut FileGraphBuilder,
    ) {
        let Some(callee) = self.callee(expr) else {
            return;
        };
        let on_self = callee.path.is_some()
            && callee.path.as_deref() == scope.receiver.as_deref()
            && callee.member;
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
        } else if self.symbols.types.contains(&callee.name) {
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
            args,
            handler: None,
            from: None,
        });
    }

    /// `Map[int](xs)` mint `type_conversion_expression`, ha a típus generikus: hívásként vesszük fel.
    fn generic_conversion(&mut self, node: Node<'_>, scope: &Scope, b: &mut FileGraphBuilder) {
        let Some(ty) = node.child_by_field_name("type") else {
            return;
        };
        if ty.kind() != "generic_type" {
            return;
        }
        let Some(inner) = ty.child_by_field_name("type") else {
            return;
        };
        let args = node
            .child_by_field_name("operand")
            .map(|o| vec![clip(self.text(o), 60)]);
        self.emit_call(inner, node, args, scope, b);
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
            "selector_expression" => {
                let name_node = expr.child_by_field_name("field")?;
                let operand = expr.child_by_field_name("operand")?;
                let (root, chained) = self.root_of(operand);
                Some(Callee {
                    name_node,
                    name: self.text(name_node).to_string(),
                    member: true,
                    path: self.path_of(operand),
                    root,
                    chained,
                })
            }
            // generikus példányosítás: `Map[int, string](xs)`
            "index_expression" => self.callee(expr.child_by_field_name("operand")?),
            // a tree-sitter-go `Map[int](xs)`-et típus-példányosításnak olvassa: `Map` típus-azonosító
            "type_instantiation_expression" => self.callee(expr.child_by_field_name("type")?),
            "type_identifier" => Some(Callee {
                name_node: expr,
                name: self.text(expr).to_string(),
                member: false,
                path: None,
                root: None,
                chained: false,
            }),
            // `pkg.Map[int](xs)`
            "qualified_type" => {
                let name_node = expr.child_by_field_name("name")?;
                let package = expr.child_by_field_name("package")?;
                let pkg = self.text(package).to_string();
                Some(Callee {
                    name_node,
                    name: self.text(name_node).to_string(),
                    member: true,
                    path: Some(pkg.clone()),
                    root: Some(pkg),
                    chained: false,
                })
            }
            _ => None,
        }
    }

    /// `a.b.c` alakú út (csak azonosítók).
    fn path_of(&self, node: Node<'_>) -> Option<String> {
        match node.kind() {
            "identifier" => Some(self.text(node).to_string()),
            "selector_expression" => {
                let operand = self.path_of(node.child_by_field_name("operand")?)?;
                let field = node.child_by_field_name("field")?;
                Some(format!("{operand}.{}", self.text(field)))
            }
            _ => None,
        }
    }

    /// A lánc bal szélső azonosítója, és hogy hívás(oka)n át értük-e el.
    fn root_of(&self, node: Node<'_>) -> (Option<String>, bool) {
        match node.kind() {
            "identifier" => (Some(self.text(node).to_string()), false),
            "selector_expression" | "index_expression" => {
                match node.child_by_field_name("operand") {
                    Some(o) => self.root_of(o),
                    None => (None, false),
                }
            }
            "call_expression" => match node.child_by_field_name("function") {
                Some(f) => (self.root_of(f).0, true),
                None => (None, true),
            },
            "parenthesized_expression" => named_children(node)
                .first()
                .map_or((None, false), |inner| self.root_of(*inner)),
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

    /// Struct-literál (`Repo{…}`, `&pkg.Config{…}`): példányosítás `new` hívásként; a slice / map / tömb
    /// literál nem hívás.
    fn literal(&mut self, node: Node<'_>, scope: &Scope, b: &mut FileGraphBuilder) {
        let Some(mut ty) = node.child_by_field_name("type") else {
            return;
        };
        if ty.kind() == "generic_type" {
            let Some(inner) = ty.child_by_field_name("type") else {
                return;
            };
            ty = inner;
        }
        let (name_node, name, package) = match ty.kind() {
            "type_identifier" => (ty, self.text(ty).to_string(), None),
            "qualified_type" => {
                let (Some(name), Some(package)) = (
                    ty.child_by_field_name("name"),
                    ty.child_by_field_name("package"),
                ) else {
                    return;
                };
                (name, self.text(name).to_string(), Some(self.text(package)))
            }
            _ => return,
        };
        let (target, category, on) = match package {
            Some(package) => {
                let stdlib = self
                    .symbols
                    .imports
                    .get(package)
                    .is_some_and(|p| is_stdlib(p));
                (
                    format!("{package}.{name}"),
                    if stdlib { "native_go" } else { "instantiation" },
                    Some(package.to_string()),
                )
            }
            None => (name, "instantiation", None),
        };
        let start = line(name_node);
        let end = end_line(node);
        b.call(Call {
            container: scope.container.clone(),
            caller: Some(scope.caller.clone()),
            line: start,
            end_line: (end > start).then_some(end),
            kind: "new".to_string(),
            target,
            resolved: None,
            on,
            category: category.to_string(),
            args: None,
            handler: None,
            from: None,
        });
    }

    /// String-literál: az idézőjelek közötti szöveg (üres / csak szóköz kimarad).
    fn string_literal(&self, node: Node<'_>, scope: &Scope, b: &mut FileGraphBuilder) {
        let raw = self.text(node);
        let inner = raw.get(1..raw.len().saturating_sub(1)).unwrap_or_default();
        if inner.trim().is_empty() {
            return;
        }
        let end = end_line(node);
        b.string(StringLit {
            container: scope.container.clone(),
            caller: Some(scope.caller.clone()),
            line: line(node),
            end_line: (end > line(node)).then_some(end),
            value: inner.chars().take(300).collect(),
        });
    }
}
