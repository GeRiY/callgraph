//! Hívás-élek: `foo()`, `Type(...)`, `a.b.c()`, módosító-lánc `.padding()`, implicit tag `.foo()`.
//! A node-ból parser-független `CallInfo` lesz, a kategóriát a `Categorizer` adja.

use tree_sitter::Node;

use super::super::categorize::CallInfo;
use super::super::parser::{end_line, line, named_children};
use super::links::Receiver;
use super::{Scope, SwiftVisitor, clip};
use crate::model::{Call, FileGraphBuilder};

/// A hívott kifejezés felbontása.
pub(super) struct Callee<'t> {
    pub(super) name_node: Node<'t>,
    pub(super) name: String,
    pub(super) member: bool,
    /// A fogadó pontozott útja, ha csak azonosítókból áll (`self.store`, `DispatchQueue.main`).
    pub(super) path: Option<String>,
    pub(super) root: Option<String>,
    pub(super) chained: bool,
}

impl SwiftVisitor<'_, '_> {
    /// Egy `call_expression` felvétele `Call`-ként.
    pub(super) fn call(&mut self, node: Node<'_>, scope: &Scope, b: &mut FileGraphBuilder) {
        let Some(expr) = named_children(node).into_iter().next() else {
            return;
        };
        let Some(callee) = self.callee(expr) else {
            return;
        };
        let on_self = callee
            .path
            .as_deref()
            .is_some_and(|p| p == "self" || p == "Self");
        let chain = callee
            .path
            .as_deref()
            .filter(|_| callee.member)
            .and_then(|path| self.receiver_chain(path, scope));
        let info = CallInfo {
            name: callee.name.clone(),
            member: callee.member,
            root: callee.root.clone(),
            on_self,
            chained: callee.chained,
            in_view: scope.in_view,
            recv_type: match &chain {
                Some(Receiver::Typed(ty)) => Some(ty.clone()),
                _ => None,
            },
        };
        let mut category = self.categorizer.categorize(&info, &self.symbols);
        let mut kind = if callee.member {
            "method_call"
        } else if callee.name.chars().next().is_some_and(char::is_uppercase) {
            "new"
        } else {
            "func_call"
        };
        let mut target = match (&callee.path, callee.member) {
            (Some(path), true) => format!("{path}.{}", callee.name),
            (None, true) => format!(".{}", callee.name),
            _ => callee.name.clone(),
        };
        let mut resolved = None;
        if callee.member {
            resolved = self.resolve_member(callee.path.as_deref(), &callee.name, scope);
            if let Some(ty) = self.injected_type(node, &callee.name, scope) {
                target = format!(".{}({ty})", callee.name);
                kind = "inject";
                resolved = None;
            }
        } else if let Some(own) = self.stored_closure(&callee.name, scope) {
            // a típus tárolt closure-property-jének hívása (`onFrame?(frame)`): a bekötés párja
            resolved = Some(own);
            category = "internal_helper";
        }
        if self.nav_depth > 0
            && kind == "new"
            && matches!(category, "component_use" | "instantiation")
        {
            kind = "nav";
        }
        // a másik fájlban megakadó lánc: `on` = `Típus::property.property` (az index folytatja a feloldást)
        let pending_on = match &chain {
            Some(Receiver::Pending { ty, rest }) if resolved.is_none() => {
                Some(format!("{ty}::{}", rest.join(".")))
            }
            _ => None,
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
            resolved,
            on: pending_on.or(callee
                .root
                .filter(|_| callee.member && !on_self && callee.path.is_some())),
            category: category.to_string(),
            args: self.args(node),
            handler: None,
            from: None,
        });
    }

    /// Generikus típus példányosítása (`ItemList<Item>(items: items)`, `FetchDescriptor<Item>()`): `new` hívás
    /// a típus nevével; tömb- / szótár-konstruktor (`[Item]()`) nem hívás-él.
    pub(super) fn constructor(&mut self, node: Node<'_>, scope: &Scope, b: &mut FileGraphBuilder) {
        let Some(name) = self.constructed_name(node) else {
            return;
        };
        let info = CallInfo {
            name: name.clone(),
            in_view: scope.in_view,
            ..CallInfo::default()
        };
        let category = self.categorizer.categorize(&info, &self.symbols);
        let kind = if self.nav_depth > 0 && matches!(category, "component_use" | "instantiation") {
            "nav"
        } else {
            "new"
        };
        let start = line(node);
        let end = end_line(node);
        b.call(Call {
            container: scope.container.clone(),
            caller: Some(scope.caller.clone()),
            line: start,
            end_line: (end > start).then_some(end),
            kind: kind.to_string(),
            target: name,
            resolved: None,
            on: None,
            category: category.to_string(),
            args: self.args(node),
            handler: None,
            from: None,
        });
    }

    pub(super) fn callee<'t>(&self, expr: Node<'t>) -> Option<Callee<'t>> {
        match expr.kind() {
            "simple_identifier" => Some(Callee {
                name_node: expr,
                name: self.text(expr).to_string(),
                member: false,
                path: None,
                root: None,
                chained: false,
            }),
            "navigation_expression" => {
                let name_node = expr
                    .child_by_field_name("suffix")?
                    .child_by_field_name("suffix")?;
                let target = expr.child_by_field_name("target")?;
                let (root, through_call) = self.root_of(target);
                Some(Callee {
                    name_node,
                    name: self.text(name_node).to_string(),
                    member: true,
                    path: self.path_of(target),
                    root,
                    chained: through_call,
                })
            }
            // `Box<Item>(…)` hívásként értelmezve: a generikus típus neve
            "user_type" => {
                let name_node = named_children(expr)
                    .into_iter()
                    .rfind(|c| c.kind() == "type_identifier")?;
                Some(Callee {
                    name_node,
                    name: self.text(name_node).to_string(),
                    member: false,
                    path: None,
                    root: None,
                    chained: false,
                })
            }
            "prefix_expression" => {
                let target = expr.child_by_field_name("target")?;
                if target.kind() == "navigation_expression" {
                    return self.callee(target);
                }
                (target.kind() == "simple_identifier").then(|| Callee {
                    name_node: target,
                    name: self.text(target).to_string(),
                    member: true,
                    path: None,
                    root: None,
                    chained: true,
                })
            }
            _ => None,
        }
    }

    /// `a.b.c` alakú út (csak azonosítók / `self`).
    pub(super) fn path_of(&self, node: Node<'_>) -> Option<String> {
        match node.kind() {
            "simple_identifier" | "self_expression" => Some(self.text(node).to_string()),
            "navigation_expression" => {
                let target = self.path_of(node.child_by_field_name("target")?)?;
                let suffix = node
                    .child_by_field_name("suffix")?
                    .child_by_field_name("suffix")?;
                Some(format!("{target}.{}", self.text(suffix)))
            }
            _ => None,
        }
    }

    /// A lánc bal szélső azonosítója, és hogy hívás(oka)n át értük-e el.
    fn root_of(&self, node: Node<'_>) -> (Option<String>, bool) {
        match node.kind() {
            "simple_identifier" | "self_expression" => (Some(self.text(node).to_string()), false),
            "navigation_expression" => match node.child_by_field_name("target") {
                Some(t) => self.root_of(t),
                None => (None, false),
            },
            "call_expression" => match named_children(node).into_iter().next() {
                Some(callee) => (self.root_of(callee).0, true),
                None => (None, true),
            },
            "prefix_expression" => (None, true),
            _ => (None, false),
        }
    }

    /// Az első ≤4 argumentum forrása (≤60 kar.); `None`, ha nincs zárójeles argumentum.
    fn args(&self, node: Node<'_>) -> Option<Vec<String>> {
        let suffix = named_children(node)
            .into_iter()
            .find(|c| matches!(c.kind(), "call_suffix" | "constructor_suffix"))?;
        let arguments = named_children(suffix)
            .into_iter()
            .find(|c| c.kind() == "value_arguments")?;
        let args: Vec<String> = named_children(arguments)
            .into_iter()
            .filter(|c| c.kind() == "value_argument")
            .take(4)
            .map(|a| clip(self.text(a), 60))
            .collect();
        (!args.is_empty()).then_some(args)
    }
}
