//! Local receiver typing: the receiver parameter, typed parameters, `:=` / `=` / `var` locals (also from a
//! multi-value call, by position) and method chains, from the declared types of this file. A call whose receiver
//! type is known gets `resolved = pkg\Type::method`, so the `Type::method` caller search finds it.

use std::collections::HashMap;

use tree_sitter::Node;

use super::super::parser::named_children;
use super::GoVisitor;

/// Declared result types and struct-field types of this file (the first-level type name, `None` = not trackable).
#[derive(Default)]
pub(super) struct TypeIndex {
    functions: HashMap<String, Vec<Option<String>>>,
    methods: HashMap<(String, String), Vec<Option<String>>>,
    fields: HashMap<(String, String), String>,
}

/// Predeclared types: a receiver of these is never a project type.
const BUILTIN_TYPES: &[&str] = &[
    "any",
    "bool",
    "byte",
    "comparable",
    "complex64",
    "complex128",
    "error",
    "float32",
    "float64",
    "int",
    "int8",
    "int16",
    "int32",
    "int64",
    "rune",
    "string",
    "uint",
    "uint8",
    "uint16",
    "uint32",
    "uint64",
    "uintptr",
];

impl GoVisitor<'_, '_> {
    /// The project-local type named by a type node: `*Command` → `Command`, `Box[T]` → `Box`; slices, maps,
    /// channels, function types, builtins and package-qualified types (`http.Client`) are `None`.
    fn type_name(&self, node: Node<'_>) -> Option<String> {
        let text = self.text(node).trim();
        let text = text.trim_start_matches('*').trim();
        let base = text.split('[').next().unwrap_or(text).trim();
        let valid = base
            .chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_')
            && base.chars().all(|c| c.is_alphanumeric() || c == '_');
        (valid && !BUILTIN_TYPES.contains(&base)).then(|| base.to_string())
    }

    /// The declared result types of a function / method / interface element, one entry per value.
    fn result_types(&self, def: Node<'_>) -> Vec<Option<String>> {
        let Some(result) = def.child_by_field_name("result") else {
            return Vec::new();
        };
        if result.kind() != "parameter_list" {
            return vec![self.type_name(result)];
        }
        let mut out = Vec::new();
        for decl in named_children(result) {
            if !matches!(
                decl.kind(),
                "parameter_declaration" | "variadic_parameter_declaration"
            ) {
                continue;
            }
            let ty = decl
                .child_by_field_name("type")
                .and_then(|t| self.type_name(t));
            let mut cursor = decl.walk();
            let names = decl.children_by_field_name("name", &mut cursor).count();
            out.extend(std::iter::repeat_n(ty, names.max(1)));
        }
        out
    }

    /// The pre-pass over the file: result types of functions, methods and interface methods, struct-field types.
    pub(super) fn collect_types(&mut self, root: Node<'_>) {
        let mut index = TypeIndex::default();
        for child in named_children(root) {
            match child.kind() {
                "function_declaration" => {
                    if let Some(name) = child.child_by_field_name("name") {
                        index
                            .functions
                            .insert(self.text(name).to_string(), self.result_types(child));
                    }
                }
                "method_declaration" => {
                    if let (Some(owner), _) = self.receiver(child)
                        && let Some(name) = child.child_by_field_name("name")
                    {
                        index.methods.insert(
                            (owner, self.text(name).to_string()),
                            self.result_types(child),
                        );
                    }
                }
                "type_declaration" => {
                    for spec in named_children(child) {
                        self.collect_type_spec(spec, &mut index);
                    }
                }
                _ => {}
            }
        }
        self.types = index;
    }

    fn collect_type_spec(&self, spec: Node<'_>, index: &mut TypeIndex) {
        if spec.kind() != "type_spec" {
            return;
        }
        let (Some(name), Some(ty)) = (
            spec.child_by_field_name("name"),
            spec.child_by_field_name("type"),
        ) else {
            return;
        };
        let owner = self.text(name).to_string();
        match ty.kind() {
            "struct_type" => {
                let list = named_children(ty)
                    .into_iter()
                    .find(|c| c.kind() == "field_declaration_list");
                for field in list.map(named_children).unwrap_or_default() {
                    let Some(field_type) = field
                        .child_by_field_name("type")
                        .and_then(|t| self.type_name(t))
                    else {
                        continue;
                    };
                    let mut cursor = field.walk();
                    for n in field.children_by_field_name("name", &mut cursor) {
                        index.fields.insert(
                            (owner.clone(), self.text(n).to_string()),
                            field_type.clone(),
                        );
                    }
                }
            }
            "interface_type" => {
                for elem in named_children(ty) {
                    if elem.kind() != "method_elem" {
                        continue;
                    }
                    if let Some(method) = elem.child_by_field_name("name") {
                        index.methods.insert(
                            (owner.clone(), self.text(method).to_string()),
                            self.result_types(elem),
                        );
                    }
                }
            }
            _ => {}
        }
    }

    /// Start of a function body: the receiver and the typed parameters are the known locals.
    pub(super) fn bind_signature(&mut self, def: Node<'_>, receiver: Option<(&str, &str)>) {
        self.locals.clear();
        if let Some((name, owner)) = receiver {
            self.locals.insert(name.to_string(), owner.to_string());
        }
        self.bind_params(def);
    }

    /// The typed names of a `parameters` list (function declarations and function literals).
    pub(super) fn bind_params(&mut self, def: Node<'_>) {
        let Some(list) = def.child_by_field_name("parameters") else {
            return;
        };
        for decl in named_children(list) {
            if decl.kind() != "parameter_declaration" {
                continue;
            }
            let ty = decl
                .child_by_field_name("type")
                .and_then(|t| self.type_name(t));
            let mut cursor = decl.walk();
            let names: Vec<Node<'_>> = decl.children_by_field_name("name", &mut cursor).collect();
            for n in names {
                let name = self.text(n).to_string();
                match &ty {
                    Some(ty) => self.locals.insert(name, ty.clone()),
                    None => self.locals.remove(&name),
                };
            }
        }
    }

    /// `a, b := f()` / `a, b = f()` / `a := b`: the types are bound by position.
    pub(super) fn bind_assignment(&mut self, node: Node<'_>) {
        let (Some(left), Some(right)) = (
            node.child_by_field_name("left"),
            node.child_by_field_name("right"),
        ) else {
            return;
        };
        let define = node.kind() == "short_var_declaration";
        if !define
            && node
                .child_by_field_name("operator")
                .is_some_and(|op| self.text(op) != "=")
        {
            return;
        }
        let targets = list_items(left);
        let types = self.value_types(&list_items(right), targets.len());
        for (at, target) in targets.iter().enumerate() {
            if target.kind() != "identifier" {
                continue;
            }
            let name = self.text(*target).to_string();
            if name == "_" {
                continue;
            }
            match types.get(at).cloned().flatten() {
                Some(ty) => {
                    self.locals.insert(name, ty);
                }
                None if define => {
                    self.locals.remove(&name);
                }
                None => {}
            }
        }
    }

    /// `var x T` / `var x = f()` / `var a, b = g()`.
    pub(super) fn bind_var_spec(&mut self, spec: Node<'_>) {
        let mut cursor = spec.walk();
        let names: Vec<Node<'_>> = spec.children_by_field_name("name", &mut cursor).collect();
        let declared = spec
            .child_by_field_name("type")
            .and_then(|t| self.type_name(t));
        let types = match (&declared, spec.child_by_field_name("value")) {
            (Some(ty), _) => vec![Some(ty.clone()); names.len()],
            (None, Some(values)) => self.value_types(&list_items(values), names.len()),
            (None, None) => Vec::new(),
        };
        for (at, n) in names.iter().enumerate() {
            let name = self.text(*n).to_string();
            match types.get(at).cloned().flatten() {
                Some(ty) => self.locals.insert(name, ty),
                None => self.locals.remove(&name),
            };
        }
    }

    /// The types of the right-hand side: one per value, or by position of a single multi-value call.
    fn value_types(&self, values: &[Node<'_>], wanted: usize) -> Vec<Option<String>> {
        match values {
            [single] if wanted > 1 => match single.kind() {
                "call_expression" => self.call_types(*single),
                // `v, ok := x.(T)`
                "type_assertion_expression" => vec![self.type_of(*single)],
                _ => Vec::new(),
            },
            _ => values.iter().map(|v| self.type_of(*v)).collect(),
        }
    }

    /// The project type of an expression, when it can be told from this file's declarations.
    pub(super) fn type_of(&self, node: Node<'_>) -> Option<String> {
        match node.kind() {
            "identifier" => self.locals.get(self.text(node)).cloned(),
            "parenthesized_expression" => self.type_of(*named_children(node).first()?),
            // `&T{}` / `*p`
            "unary_expression" => self.type_of(node.child_by_field_name("operand")?),
            "composite_literal" | "type_assertion_expression" => {
                self.type_name(node.child_by_field_name("type")?)
            }
            "selector_expression" => {
                let owner = self.type_of(node.child_by_field_name("operand")?)?;
                let field = self.text(node.child_by_field_name("field")?).to_string();
                self.types.fields.get(&(owner, field)).cloned()
            }
            "call_expression" => self.call_types(node).into_iter().next().flatten(),
            _ => None,
        }
    }

    /// The declared result types of the called function / method (`new(T)` and `T(x)` conversions give `T`).
    fn call_types(&self, call: Node<'_>) -> Vec<Option<String>> {
        let Some(function) = call.child_by_field_name("function") else {
            return Vec::new();
        };
        match function.kind() {
            "identifier" => {
                let name = self.text(function);
                if let Some(found) = self.types.functions.get(name) {
                    return found.clone();
                }
                if name == "new" {
                    let first = call
                        .child_by_field_name("arguments")
                        .and_then(|a| named_children(a).into_iter().next());
                    return vec![first.and_then(|t| self.type_name(t))];
                }
                if self.symbols.types.contains(name) {
                    return vec![Some(name.to_string())];
                }
                Vec::new()
            }
            "selector_expression" => {
                let (Some(operand), Some(field)) = (
                    function.child_by_field_name("operand"),
                    function.child_by_field_name("field"),
                ) else {
                    return Vec::new();
                };
                let Some(owner) = self.type_of(operand) else {
                    return Vec::new();
                };
                self.types
                    .methods
                    .get(&(owner, self.text(field).to_string()))
                    .cloned()
                    .unwrap_or_default()
            }
            _ => Vec::new(),
        }
    }

    /// `pkg\Type::method`, the key the caller index uses for a method of a same-package type.
    pub(super) fn method_key(&self, owner: &str, method: &str) -> String {
        match self.package.as_deref() {
            Some(package) if !package.is_empty() => format!("{package}\\{owner}::{method}"),
            _ => format!("{owner}::{method}"),
        }
    }
}

/// The items of an `expression_list` (a single expression is a one-item list).
fn list_items(node: Node<'_>) -> Vec<Node<'_>> {
    if node.kind() == "expression_list" {
        named_children(node)
    } else {
        vec![node]
    }
}
