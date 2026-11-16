//! Hívás-élek: `foo()`, `Type(...)`, `a.b.c()`, `this.foo()`, módosító-lánc `Modifier.padding().fillMaxWidth()`,
//! valamint a metódus-hivatkozások (`::foo`, `Foo::bar`, `this::baz`). A node-ból parser-független `CallInfo`
//! lesz, a kategóriát a `Categorizer` adja; az ismert típusú fogadónál a hívás `Típus::név` alakban feloldott.

use tree_sitter::Node;

use super::super::categorize::CallInfo;
use super::super::parser::{children, end_line, line, named_children};
use super::util::{
    IDENTS, base_type, declared_type_around, is_comment, is_injector, type_argument,
};
use super::{KotlinVisitor, Scope, clip};
use crate::model::{Call, FileGraphBuilder};

/// A hívott kifejezés felbontása.
struct Callee<'t> {
    name_node: Node<'t>,
    name: String,
    member: bool,
    /// A fogadó pontozott útja, ha csak azonosítókból áll (`this.repo`, `Log`, `viewModel`).
    path: Option<String>,
    root: Option<String>,
    chained: bool,
}

/// A nevesített, nem komment gyerekek.
fn operands(node: Node<'_>) -> Vec<Node<'_>> {
    named_children(node)
        .into_iter()
        .filter(|c| !is_comment(*c))
        .collect()
}

/// A navigációs kifejezés (`target.name`) két fele: a fogadó és a név-node.
fn navigation<'t>(expr: Node<'t>) -> Option<(Node<'t>, Node<'t>)> {
    let parts = operands(expr);
    let target = *parts.first()?;
    let last = *parts.last()?;
    if target.id() == last.id() {
        return None;
    }
    let name = if last.kind() == "navigation_suffix" {
        *operands(last).last()?
    } else {
        last
    };
    Some((target, name))
}

impl KotlinVisitor<'_, '_> {
    /// Egy `call_expression` felvétele `Call`-ként.
    pub(super) fn call(&mut self, node: Node<'_>, scope: &Scope, b: &mut FileGraphBuilder) {
        let Some(expr) = operands(node).into_iter().next() else {
            return;
        };
        let Some(callee) = self.callee(expr) else {
            return;
        };
        let on_self = callee
            .path
            .as_deref()
            .is_some_and(|p| p == "this" || p == "super");
        let receiver_type = self.receiver_type_of(callee.path.as_deref(), scope);
        let args = self.arg_nodes(node);
        let info = CallInfo {
            name: callee.name.clone(),
            member: callee.member,
            root: callee.root.clone(),
            on_self,
            chained: callee.chained,
            in_compose: scope.in_compose,
            first_arg_string: args
                .first()
                .is_some_and(|a| self.text(*a).trim_start().starts_with('"')),
            receiver_type: receiver_type.clone(),
        };
        let category = self.categorizer.categorize(&info, &self.symbols);
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
        let mut resolved = self.resolved_of(&callee, receiver_type.as_deref());
        let mut args_text = self.args(&args);
        // `hiltViewModel()` / `by viewModels()`: befecskendezés, a példány típusa a típus-argumentumból vagy a
        // deklarációból (`viewModel: X = hiltViewModel()`)
        if !callee.member
            && is_injector(&callee.name)
            && let Some(ty) =
                type_argument(self.parsed, node).or_else(|| declared_type_around(self.parsed, node))
        {
            kind = "inject";
            target = format!("{}({ty})", callee.name);
            resolved = None;
        }
        // `composable("útvonal") { Képernyő() }`: a képernyő-hívás navigációs él, az argumentuma az útvonal
        if let Some(route) = self.symbols.screen_nodes.get(&node.id()) {
            kind = "nav";
            args_text = Some(vec![route.clone()]);
        }
        let start = line(callee.name_node);
        let end = end_line(node);
        let on = if !callee.member || callee.path.is_none() {
            None
        } else if on_self {
            Some("this".to_string())
        } else {
            callee.root.clone()
        };
        b.call(Call {
            container: scope.container.clone(),
            caller: Some(scope.caller.clone()),
            line: start,
            end_line: (end > start).then_some(end),
            kind: kind.to_string(),
            target,
            resolved,
            on,
            category: category.to_string(),
            args: args_text,
            handler: None,
            from: None,
        });
        if callee.member && callee.name == "navigate" {
            self.navigate_edge(node, scope, b);
        }
    }

    /// `Foo::bar` / `::bar` / `this::bar` metódus-hivatkozás hívás-élként (`type: reference`).
    pub(super) fn reference(&mut self, node: Node<'_>, scope: &Scope, b: &mut FileGraphBuilder) {
        let all = children(node);
        let Some(sep) = all.iter().position(|c| c.kind() == "::") else {
            return;
        };
        let recv = all[..sep]
            .iter()
            .copied()
            .find(|c| c.is_named() && !is_comment(*c));
        let Some(name_node) = all[sep + 1..]
            .iter()
            .copied()
            .find(|c| c.is_named() && !is_comment(*c))
        else {
            return;
        };
        let name = self.text(name_node).to_string();
        if name == "class" || name.is_empty() {
            return;
        }
        let path = recv.and_then(|r| self.path_of(r));
        if recv.is_some() && path.is_none() {
            return;
        }
        let on_self = path.as_deref() == Some("this");
        let receiver_type = self.receiver_type_of(path.as_deref(), scope).or_else(|| {
            path.as_deref()
                .filter(|p| p.chars().next().is_some_and(char::is_uppercase))
                .and_then(base_type)
        });
        let constructor = name.chars().next().is_some_and(char::is_uppercase);
        let member = path.is_some() && !on_self;
        let info = CallInfo {
            name: name.clone(),
            member: path.is_some(),
            root: path
                .as_deref()
                .and_then(|p| p.split('.').next())
                .map(str::to_string),
            on_self,
            chained: false,
            in_compose: scope.in_compose,
            first_arg_string: false,
            receiver_type: receiver_type.clone(),
        };
        let category = self.categorizer.categorize(&info, &self.symbols);
        let target = match &path {
            Some(p) => format!("{p}.{name}"),
            None => name.clone(),
        };
        let resolved = match (&receiver_type, member || on_self) {
            (Some(t), true) if !on_self => Some(format!("{t}::{name}")),
            _ => None,
        };
        let ln = line(node);
        b.call(Call {
            container: scope.container.clone(),
            caller: Some(scope.caller.clone()),
            line: ln,
            end_line: None,
            kind: if constructor && path.is_none() {
                "new".to_string()
            } else {
                "reference".to_string()
            },
            target,
            resolved,
            on: match (&path, on_self) {
                (Some(_), true) => Some("this".to_string()),
                (Some(p), false) => p.split('.').next().map(str::to_string),
                _ => None,
            },
            category: category.to_string(),
            args: None,
            handler: None,
            from: None,
        });
    }

    fn callee<'t>(&self, expr: Node<'t>) -> Option<Callee<'t>> {
        match expr.kind() {
            k if IDENTS.contains(&k) => Some(Callee {
                name_node: expr,
                name: self.text(expr).to_string(),
                member: false,
                path: None,
                root: None,
                chained: false,
            }),
            "this_expression" | "super_expression" => Some(Callee {
                name_node: expr,
                name: self.text(expr).trim().to_string(),
                member: false,
                path: None,
                root: None,
                chained: false,
            }),
            "navigation_expression" => {
                let (target, name_node) = navigation(expr)?;
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
            _ => None,
        }
    }

    /// `a.b.c` alakú út (csak azonosítók / `this` / `super`).
    pub(super) fn path_of(&self, node: Node<'_>) -> Option<String> {
        match node.kind() {
            k if IDENTS.contains(&k) => Some(self.text(node).to_string()),
            "this_expression" => Some("this".to_string()),
            "super_expression" => Some("super".to_string()),
            "navigation_expression" => {
                let (target, name) = navigation(node)?;
                Some(format!("{}.{}", self.path_of(target)?, self.text(name)))
            }
            // `repo.owner!!.save()`: a `!!` nem változtat az úton
            _ => self
                .non_null_inner(node)
                .and_then(|inner| self.path_of(inner)),
        }
    }

    /// A `x!!` kifejezés belső kifejezése (a node szövege `!!`-ra végződik, és van nevesített gyereke).
    fn non_null_inner<'t>(&self, node: Node<'t>) -> Option<Node<'t>> {
        if IDENTS.contains(&node.kind())
            || matches!(
                node.kind(),
                "string_literal" | "multiline_string_literal" | "line_comment" | "block_comment"
            )
            || !self.text(node).trim_end().ends_with("!!")
        {
            return None;
        }
        operands(node).into_iter().next()
    }

    /// A lánc bal szélső azonosítója, és hogy hívás(oka)n át értük-e el.
    fn root_of(&self, node: Node<'_>) -> (Option<String>, bool) {
        match node.kind() {
            k if IDENTS.contains(&k) => (Some(self.text(node).to_string()), false),
            "this_expression" => (Some("this".to_string()), false),
            "navigation_expression" => match navigation(node) {
                Some((t, _)) => self.root_of(t),
                None => (None, false),
            },
            "call_expression" => match operands(node).into_iter().next() {
                Some(callee) => (self.root_of(callee).0, true),
                None => (None, true),
            },
            _ => match self.non_null_inner(node) {
                Some(inner) => self.root_of(inner),
                None => (None, false),
            },
        }
    }

    /// A fogadó ismert típusa: `x` → helyi változó / paraméter / property típusa; `this.x` → property típusa;
    /// nagybetűs `Foo` (statikus / companion hívás) → maga a név.
    fn receiver_type_of(&self, path: Option<&str>, scope: &Scope) -> Option<String> {
        let path = path?;
        let (var, only_member) = match path.strip_prefix("this.") {
            Some(rest) => (rest, true),
            None => (path, false),
        };
        if var.contains('.') || var == "this" || var == "super" {
            return None;
        }
        if !only_member && let Some(t) = self.locals.get(var) {
            return Some(t.clone());
        }
        if let Some(t) = self
            .class_members(scope.container.as_deref())
            .and_then(|m| m.get(var))
        {
            return Some(t.clone());
        }
        (!only_member && var.chars().next().is_some_and(char::is_uppercase))
            .then(|| var.to_string())
    }

    /// A feloldott cél (`Típus::név`): csak egyszerű, ismert típusú fogadónál.
    fn resolved_of(&self, callee: &Callee<'_>, receiver_type: Option<&str>) -> Option<String> {
        let ty = receiver_type?;
        (callee.member
            && callee
                .path
                .as_deref()
                .is_some_and(|p| p != "this" && p != "super"))
        .then(|| format!("{ty}::{}", callee.name))
    }

    /// Az argumentum-node-ok (zárójeles argumentumlista).
    fn arg_nodes<'t>(&self, node: Node<'t>) -> Vec<Node<'t>> {
        let mut holders = named_children(node);
        if let Some(pos) = holders.iter().position(|c| c.kind() == "call_suffix") {
            holders.extend(named_children(holders[pos]));
        }
        holders
            .into_iter()
            .find(|c| c.kind() == "value_arguments")
            .map(|v| {
                named_children(v)
                    .into_iter()
                    .filter(|c| !is_comment(*c))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Az első ≤4 argumentum forrása (≤60 kar.); `None`, ha nincs zárójeles argumentum.
    fn args(&self, nodes: &[Node<'_>]) -> Option<Vec<String>> {
        let args: Vec<String> = nodes
            .iter()
            .take(4)
            .map(|a| clip(self.text(*a), 60))
            .collect();
        (!args.is_empty()).then_some(args)
    }
}
