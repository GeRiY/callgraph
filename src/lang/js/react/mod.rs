//! React function components: the props a component declares, read from its first parameter (destructuring with
//! defaults and `...rest`, a TS type, `React.FC<Props>`, `forwardRef<El, Props>`, `memo(…)`), `Comp.propTypes` and
//! `Comp.defaultProps`.
//!
//! The facts are `Constant`s of the file's main container:
//! - `react_comp` `Card`: one per component; the value lists `default`, `export:<name>` and `open` (the prop set
//!   may be larger than the listed names);
//! - `react_prop` `Card.title`: one per declared prop; the value is `*` for a required one.
//!
//! The outline prints them under the component, the project-level `props` view checks the JSX uses against them.

mod types;

use std::sync::Arc;

use oxc_ast::ast::*;
use oxc_ast_visit::{Visit, walk};
use oxc_syntax::scope::ScopeFlags;

use self::types::{PropSet, Types};
use super::analysis::{JsAnalysis, JsPass, PassContext};
use super::props::container_name;
use super::resolve::Resolver;
use crate::core::srcscan::{Dialect, matching, split_args};
use crate::model::Constant;

/// The constant kinds of the facts.
pub const COMP_KIND: &str = "react_comp";
pub const PROP_KIND: &str = "react_prop";

/// The `react_comp` flag of a default export / of a named export.
pub const FLAG_DEFAULT: &str = "default";
pub const FLAG_OPEN: &str = "open";
pub const FLAG_EXPORT: &str = "export:";

/// Function wrappers that keep the component's props.
const WRAPPERS: [&str; 3] = ["memo", "forwardRef", "observer"];
/// `FC<Props>`-like annotations: the first type argument is the props type.
const FC_TYPES: [&str; 5] = [
    "FC",
    "FunctionComponent",
    "VFC",
    "VoidFunctionComponent",
    "ComponentType",
];

/// The props collector.
pub struct ReactPass {
    resolver: Arc<Resolver>,
}

impl ReactPass {
    pub fn new(resolver: Arc<Resolver>) -> Self {
        Self { resolver }
    }
}

/// A function that is (or wraps) a component body.
#[derive(Clone, Copy)]
enum Func<'p, 'a> {
    Func(&'p Function<'a>),
    Arrow(&'p ArrowFunctionExpression<'a>),
}

impl<'p, 'a> Func<'p, 'a> {
    fn params(self) -> &'p FormalParameters<'a> {
        match self {
            Self::Func(f) => &f.params,
            Self::Arrow(f) => &f.params,
        }
    }

    fn has_jsx(self) -> bool {
        let mut probe = JsxProbe(false);
        match self {
            Self::Func(f) => probe.visit_function(f, ScopeFlags::Function),
            Self::Arrow(f) => probe.visit_arrow_function_expression(f),
        }
        probe.0
    }

    /// The `props.x` accesses of an untyped, non-destructured first parameter.
    fn accesses(self, param: &str) -> Vec<String> {
        let mut probe = AccessProbe {
            param,
            names: Vec::new(),
        };
        match self {
            Self::Func(f) => probe.visit_function(f, ScopeFlags::Function),
            Self::Arrow(f) => probe.visit_arrow_function_expression(f),
        }
        probe.names
    }
}

struct JsxProbe(bool);

impl<'a> Visit<'a> for JsxProbe {
    fn visit_jsx_element(&mut self, _: &JSXElement<'a>) {
        self.0 = true;
    }

    fn visit_jsx_fragment(&mut self, _: &JSXFragment<'a>) {
        self.0 = true;
    }
}

struct AccessProbe<'n> {
    param: &'n str,
    names: Vec<String>,
}

impl<'a> Visit<'a> for AccessProbe<'_> {
    fn visit_static_member_expression(&mut self, it: &StaticMemberExpression<'a>) {
        if let Expression::Identifier(id) = &it.object
            && id.name == self.param
        {
            let name = it.property.name.to_string();
            if !self.names.contains(&name) {
                self.names.push(name);
            }
        }
        walk::walk_static_member_expression(self, it);
    }
}

/// A component found at the top level of the file.
struct Found<'p, 'a> {
    name: String,
    line: u32,
    func: Func<'p, 'a>,
    /// The props type: a wrapper's type argument, or the variable's `FC<Props>` annotation.
    declared: Option<&'p TSType<'a>>,
    default: bool,
    exports: Vec<String>,
    /// An `FC<Props>` annotation: React 17 typings add an implicit `children`.
    implicit_children: bool,
}

impl JsPass for ReactPass {
    fn run(&self, ctx: &PassContext<'_>, program: &Program<'_>, out: &mut JsAnalysis) {
        if !is_react_source(ctx.path) {
            return;
        }
        let mut found: Vec<Found<'_, '_>> = Vec::new();
        let mut default_alias: Option<String> = None;
        let mut named: Vec<(String, String)> = Vec::new();
        for stmt in &program.body {
            match stmt {
                Statement::ExportDeclaration(e) => {
                    collect_decl(ctx, &e.declaration, true, &mut found)
                }
                Statement::ExportDefaultDeclaration(e) => match &e.declaration {
                    ExportDefaultDeclarationKind::FunctionDeclaration(f) if f.body.is_some() => {
                        let name = f.id.as_ref().map_or("default", |i| i.name.as_str());
                        push_found(ctx, &mut found, name, f.span, Func::Func(f), None, true);
                    }
                    ExportDefaultDeclarationKind::Identifier(id) => {
                        default_alias = Some(id.name.to_string());
                    }
                    other => {
                        if let Some(expr) = other.as_expression()
                            && let Some((func, declared)) = unwrap(expr)
                        {
                            let name = match func {
                                Func::Func(f) => f.id.as_ref().map(|i| i.name.to_string()),
                                Func::Arrow(_) => None,
                            };
                            let name = name.unwrap_or_else(|| "default".to_string());
                            push_found(ctx, &mut found, &name, e.span, func, declared, true);
                        }
                    }
                },
                Statement::ExportNamedDeclaration(e) => {
                    for s in &e.specifiers {
                        named.push((s.local.name().to_string(), s.exported.name().to_string()));
                    }
                }
                other => {
                    if let Some(decl) = other.as_declaration() {
                        collect_decl(ctx, decl, false, &mut found);
                    }
                }
            }
        }
        if let Some(alias) = default_alias
            && let Some(c) = found.iter_mut().find(|c| c.name == alias)
        {
            c.default = true;
        }
        for (local, exported) in named {
            if let Some(c) = found.iter_mut().find(|c| c.name == local) {
                if exported == "default" {
                    c.default = true;
                } else {
                    c.exports.push(exported);
                }
            }
        }
        let container = container_name(ctx);
        let types = Types {
            program,
            path: ctx.path,
            resolver: &self.resolver,
        };
        for comp in &found {
            let set = comp_props(&types, ctx.source, comp);
            let mut flags: Vec<String> = Vec::new();
            if comp.default {
                flags.push(FLAG_DEFAULT.to_string());
            }
            flags.extend(comp.exports.iter().map(|e| format!("{FLAG_EXPORT}{e}")));
            if set.open {
                flags.push(FLAG_OPEN.to_string());
            }
            push(
                out,
                &container,
                COMP_KIND,
                &comp.name,
                comp.line,
                flag_value(&flags),
            );
            for (name, required) in &set.props {
                let value = required.then(|| "*".to_string());
                push(
                    out,
                    &container,
                    PROP_KIND,
                    &format!("{}.{name}", comp.name),
                    comp.line,
                    value,
                );
            }
        }
    }
}

fn flag_value(flags: &[String]) -> Option<String> {
    (!flags.is_empty()).then(|| flags.join(","))
}

fn push(
    out: &mut JsAnalysis,
    container: &str,
    kind: &str,
    name: &str,
    line: u32,
    value: Option<String>,
) {
    out.builder.constant(Constant {
        js: true,
        container: Some(container.to_string()),
        kind: kind.into(),
        name: name.to_string(),
        line,
        value,
        visibility: None,
        caller: None,
        doc: None,
    });
}

/// Only `.jsx` / `.tsx` / `.js` / `.mjs` files hold components (a `.vue` file has its own props).
fn is_react_source(path: &std::path::Path) -> bool {
    matches!(
        path.extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .as_deref(),
        Some("jsx" | "tsx" | "js" | "mjs")
    )
}

fn capitalised(name: &str) -> bool {
    name.starts_with(|c: char| c.is_ascii_uppercase())
}

fn push_found<'p, 'a>(
    ctx: &PassContext<'_>,
    found: &mut Vec<Found<'p, 'a>>,
    name: &str,
    span: oxc_span::Span,
    func: Func<'p, 'a>,
    declared: Option<&'p TSType<'a>>,
    default: bool,
) {
    if found.iter().any(|c| c.name == name) || !func.has_jsx() {
        return;
    }
    found.push(Found {
        name: name.to_string(),
        line: ctx.lines.line_of(span.start as usize),
        func,
        declared,
        default,
        exports: Vec::new(),
        implicit_children: false,
    });
}

fn collect_decl<'p, 'a>(
    ctx: &PassContext<'_>,
    decl: &'p Declaration<'a>,
    exported: bool,
    found: &mut Vec<Found<'p, 'a>>,
) {
    match decl {
        Declaration::FunctionDeclaration(f) if f.body.is_some() => {
            if let Some(id) = f.id.as_ref().filter(|i| capitalised(i.name.as_str())) {
                push_found(ctx, found, &id.name, f.span, Func::Func(f), None, false);
                mark_export(found, &id.name, exported);
            }
        }
        Declaration::VariableDeclaration(v) => {
            for d in &v.declarations {
                let (Some(id), Some(init)) = (d.id.get_binding_identifier(), d.init.as_ref())
                else {
                    continue;
                };
                if !capitalised(id.name.as_str()) {
                    continue;
                }
                let Some((func, wrapped)) = unwrap(init) else {
                    continue;
                };
                let annotated = d
                    .type_annotation
                    .as_ref()
                    .and_then(|a| fc_props_type(&a.type_annotation));
                push_found(
                    ctx,
                    found,
                    &id.name,
                    v.span,
                    func,
                    wrapped.or(annotated),
                    false,
                );
                if let Some(c) = found.iter_mut().find(|c| c.name == id.name.as_str()) {
                    c.implicit_children = wrapped.is_none() && annotated.is_some();
                }
                mark_export(found, &id.name, exported);
            }
        }
        _ => {}
    }
}

fn mark_export(found: &mut [Found<'_, '_>], name: &str, exported: bool) {
    if exported && let Some(c) = found.iter_mut().find(|c| c.name == name) {
        c.exports.push(name.to_string());
    }
}

/// `FC<Props>` / `React.FunctionComponent<Props>`: the props type.
fn fc_props_type<'p, 'a>(ty: &'p TSType<'a>) -> Option<&'p TSType<'a>> {
    let TSType::TSTypeReference(r) = ty else {
        return None;
    };
    let name = match &r.type_name {
        TSTypeName::IdentifierReference(id) => id.name.as_str(),
        TSTypeName::QualifiedName(q) => q.right.name.as_str(),
        TSTypeName::ThisExpression(_) => return None,
    };
    if !FC_TYPES.contains(&name) {
        return None;
    }
    r.type_arguments.as_ref().and_then(|a| a.params.first())
}

/// The component function of an expression: an arrow / function expression, possibly inside `memo(…)` /
/// `forwardRef(…)` (also `React.memo`); the props type argument of the wrapper.
fn unwrap<'p, 'a>(expr: &'p Expression<'a>) -> Option<(Func<'p, 'a>, Option<&'p TSType<'a>>)> {
    let mut e = expr.without_parentheses();
    let mut declared = None;
    loop {
        match e {
            Expression::FunctionExpression(f) => return Some((Func::Func(f), declared)),
            Expression::ArrowFunctionExpression(f) => return Some((Func::Arrow(f), declared)),
            Expression::CallExpression(c) => {
                let name = match &c.callee {
                    Expression::Identifier(id) => id.name.as_str(),
                    Expression::StaticMemberExpression(m) => m.property.name.as_str(),
                    _ => return None,
                };
                if !WRAPPERS.contains(&name) {
                    return None;
                }
                if declared.is_none() {
                    // `forwardRef<El, Props>`: the second argument; `memo<Props>`: the first
                    let at = usize::from(name == "forwardRef");
                    declared = c.type_arguments.as_ref().and_then(|a| a.params.get(at));
                }
                e = c.arguments.first()?.as_expression()?.without_parentheses();
            }
            _ => return None,
        }
    }
}

/// The props of one component.
fn comp_props(types: &Types<'_, '_>, source: &str, comp: &Found<'_, '_>) -> PropSet {
    let mut set = PropSet::default();
    let params = comp.func.params();
    let Some(first) = params.items.first() else {
        // no parameter: nothing to say about the attributes
        set.open = true;
        return set;
    };
    let ty = first
        .type_annotation
        .as_ref()
        .map(|a| &a.type_annotation)
        .or(comp.declared);
    if let Some(ty) = ty {
        types.collect(ty, 0, &mut set);
    }
    // `FC<Props>` carried an implicit `children` before React 18
    if comp.implicit_children && first.type_annotation.is_none() {
        set.add("children", false);
    }
    match &first.pattern {
        BindingPattern::ObjectPattern(o) => {
            for p in &o.properties {
                let Some(name) = super::props::key_name(&p.key).filter(|_| !p.computed) else {
                    set.open = true;
                    continue;
                };
                if matches!(p.value, BindingPattern::AssignmentPattern(_)) {
                    set.make_optional(name);
                } else {
                    set.add(name, false);
                }
            }
            if o.rest.is_some() {
                set.open = true;
            }
        }
        BindingPattern::BindingIdentifier(id) => {
            if ty.is_none() {
                for name in comp.func.accesses(id.name.as_str()) {
                    set.add(&name, false);
                }
                // the whole object is passed around: other props may be read anywhere
                set.open = true;
            }
        }
        _ => set.open = true,
    }
    if ty.is_none() && params.rest.is_some() {
        set.open = true;
    }
    prop_types(source, &comp.name, &mut set);
    default_props(source, &comp.name, &mut set);
    set
}

/// `Comp.propTypes = { a: PropTypes.string.isRequired, b: PropTypes.number }`.
fn prop_types(source: &str, name: &str, set: &mut PropSet) {
    let Some((open, close)) = assigned_object(source, &format!("{name}.propTypes")) else {
        return;
    };
    for (a, b) in split_args(source, open + 1, close, Dialect::Js) {
        let entry = source[a..b].trim();
        if entry.is_empty() {
            continue;
        }
        let Some((key, value)) = entry_key(entry) else {
            set.open = true;
            continue;
        };
        set.add(&key, value.trim_end().ends_with(".isRequired"));
    }
}

/// `Comp.defaultProps = { size: 'md' }`: the listed props are optional.
fn default_props(source: &str, name: &str, set: &mut PropSet) {
    let Some((open, close)) = assigned_object(source, &format!("{name}.defaultProps")) else {
        return;
    };
    for (a, b) in split_args(source, open + 1, close, Dialect::Js) {
        match entry_key(source[a..b].trim()) {
            Some((key, _)) => set.make_optional(&key),
            None => set.open = true,
        }
    }
}

/// The `{ … }` assigned to `<target> = {`: the offsets of the braces.
fn assigned_object(source: &str, target: &str) -> Option<(usize, usize)> {
    let mut from = 0;
    while let Some(at) = source[from..].find(target) {
        let start = from + at;
        from = start + target.len();
        let before_ok = source[..start]
            .chars()
            .next_back()
            .is_none_or(|c| !(c.is_alphanumeric() || c == '_' || c == '$' || c == '.'));
        if !before_ok {
            continue;
        }
        let rest = source[from..].trim_start();
        let Some(rest) = rest.strip_prefix('=') else {
            continue;
        };
        if rest.starts_with('=') {
            continue;
        }
        let rest = rest.trim_start();
        if !rest.starts_with('{') {
            continue;
        }
        let open = source.len() - rest.len();
        let close = matching(source, open, source.len(), Dialect::Js)?;
        return Some((open, close));
    }
    None
}

/// `key: value` / `'key': value` / shorthand `key`: the key and the value text; a spread or computed key: `None`.
fn entry_key(entry: &str) -> Option<(String, &str)> {
    if entry.starts_with("...") || entry.starts_with('[') {
        return None;
    }
    let (key, value) = match entry.split_once(':') {
        Some((key, value)) => (key.trim(), value),
        None => (entry.trim(), ""),
    };
    let key = key.trim_matches(['\'', '"']);
    (!key.is_empty()
        && key
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '$'))
    .then(|| (key.to_string(), value))
}

#[cfg(test)]
mod tests;
