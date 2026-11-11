//! Struktúra: névtér, importok, konténerek és metódusok (a `FileGraph` váza).

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use mago_span::HasSpan;
use mago_syntax::cst::cst::*;
use mago_syntax::cst::sequence::Sequence;
use mago_syntax::cst::{Identifier, Statement, TriviaKind};
use mago_syntax::{cst, walker};

use super::FileVisitor;
use crate::lang::php::analysis::docblock::{doc_for, doc_types_for};
use crate::lang::php::analysis::{AnalysisContext, NameResolver};
use crate::lang::php::parser::ParsedFile;
use crate::model::{Container, FileGraphBuilder, Method};

/// A `namespace`, `imports`, `containers` és `methods` mezőket tölti.
#[derive(Debug, Default, Clone, Copy)]
pub struct StructureVisitor;

impl FileVisitor for StructureVisitor {
    fn visit(
        &self,
        parsed: &ParsedFile<'_>,
        _ctx: &mut AnalysisContext,
        builder: &mut FileGraphBuilder,
    ) {
        let mut resolver = NameResolver::new(None);
        walk_statements(parsed, &parsed.program.statements, &mut resolver, builder);
    }
}

struct AnonymousCollector<'a> {
    parsed: &'a ParsedFile<'a>,
    resolver: NameResolver,
    closure_depth: AtomicUsize,
    saw_anonymous: AtomicBool,
}

impl<'a, 'arena> walker::Walker<'a, 'arena, FileGraphBuilder> for AnonymousCollector<'a> {
    fn walk_in_anonymous_class(
        &self,
        node: &cst::AnonymousClass<'arena>,
        builder: &mut FileGraphBuilder,
    ) {
        self.saw_anonymous.store(true, Ordering::Relaxed);
        let (line, end_line) = self.parsed.lines_of(node);
        for member in node.members.iter() {
            if let ClassLikeMember::Method(node_method) = member {
                let mut m = method(self.parsed, "<anonymous>", node_method);
                let depth = self.closure_depth.load(Ordering::Relaxed);
                if depth > 0 {
                    m.name.push_str(&format!(".<closure>{depth}"));
                }
                builder.method(m);
            }
        }
        builder.container(Container {
            kind: "class".into(),
            name: "<anonymous>".into(),
            line,
            end_line,
            is_abstract: Some(false),
            is_final: Some(false),
            extends: node
                .extends
                .as_ref()
                .map(|e| Some(names(e.types.iter(), &self.resolver)))
                .unwrap_or_else(|| Some(Vec::new())),
            implements: node
                .implements
                .as_ref()
                .map(|i| Some(names(i.types.iter(), &self.resolver)))
                .unwrap_or_else(|| Some(Vec::new())),
            traits: Some(Vec::new()),
            doc: doc_for(self.parsed, node.span()),
        });
    }

    fn walk_in_closure(&self, _: &cst::Closure<'arena>, _: &mut FileGraphBuilder) {
        self.closure_depth.fetch_add(1, Ordering::Relaxed);
    }

    fn walk_out_closure(&self, _: &cst::Closure<'arena>, _: &mut FileGraphBuilder) {
        self.closure_depth
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |d| {
                Some(d.saturating_sub(1))
            })
            .ok();
    }

    fn walk_in_arrow_function(&self, _: &cst::ArrowFunction<'arena>, _: &mut FileGraphBuilder) {
        self.closure_depth.fetch_add(1, Ordering::Relaxed);
    }

    fn walk_out_arrow_function(&self, _: &cst::ArrowFunction<'arena>, _: &mut FileGraphBuilder) {
        self.closure_depth
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |d| {
                Some(d.saturating_sub(1))
            })
            .ok();
    }
}

fn text(id: &[u8]) -> String {
    String::from_utf8_lossy(id).into_owned()
}

fn ident(id: &Identifier<'_>) -> String {
    text(id.value()).trim_start_matches('\\').to_string()
}

fn walk_statements(
    parsed: &ParsedFile<'_>,
    stmts: &Sequence<'_, Statement<'_>>,
    resolver: &mut NameResolver,
    b: &mut FileGraphBuilder,
) {
    for st in stmts.iter() {
        match st {
            Statement::Namespace(ns) => {
                let name = ns.name.as_ref().map(ident);
                b.namespace_if_unset(name.clone());
                let mut inner = NameResolver::new(name);
                walk_statements(parsed, ns.statements(), &mut inner, b);
            }
            Statement::Use(u) => collect_use(u, resolver, b),
            Statement::Function(_) => {
                let collector = AnonymousCollector {
                    parsed,
                    resolver: resolver.clone(),
                    closure_depth: AtomicUsize::new(0),
                    saw_anonymous: AtomicBool::new(false),
                };
                walker::walk_statement(&collector, st, b);
            }
            Statement::Class(c) => {
                let (mut is_abstract, mut is_final) = (false, false);
                for m in c.modifiers.iter() {
                    is_abstract |= matches!(m, Modifier::Abstract(_));
                    is_final |= matches!(m, Modifier::Final(_));
                }
                let extends = c
                    .extends
                    .as_ref()
                    .map(|e| names(e.types.iter(), resolver))
                    .unwrap_or_default();
                let implements = c
                    .implements
                    .as_ref()
                    .map(|i| names(i.types.iter(), resolver))
                    .unwrap_or_default();
                container(
                    parsed,
                    "class",
                    c.name.value,
                    c.span(),
                    (is_abstract, is_final),
                    (extends, implements),
                    &c.members,
                    resolver,
                    b,
                );
            }
            Statement::Interface(i) => {
                let extends = i
                    .extends
                    .as_ref()
                    .map(|e| names(e.types.iter(), resolver))
                    .unwrap_or_default();
                container(
                    parsed,
                    "interface",
                    i.name.value,
                    i.span(),
                    (false, false),
                    (extends, vec![]),
                    &i.members,
                    resolver,
                    b,
                );
            }
            Statement::Trait(t) => {
                container(
                    parsed,
                    "trait",
                    t.name.value,
                    t.span(),
                    (false, false),
                    (vec![], vec![]),
                    &t.members,
                    resolver,
                    b,
                );
            }
            Statement::Enum(e) => {
                let implements = e
                    .implements
                    .as_ref()
                    .map(|i| names(i.types.iter(), resolver))
                    .unwrap_or_default();
                container(
                    parsed,
                    "enum",
                    e.name.value,
                    e.span(),
                    (false, false),
                    (vec![], implements),
                    &e.members,
                    resolver,
                    b,
                );
            }
            _ => {}
        }
    }
}

fn names<'a, 'arena: 'a>(
    ids: impl Iterator<Item = &'a Identifier<'arena>>,
    resolver: &NameResolver,
) -> Vec<String> {
    ids.map(|id| resolver.resolve_class(&text(id.value())))
        .collect()
}

fn collect_use(u: &Use<'_>, resolver: &mut NameResolver, b: &mut FileGraphBuilder) {
    let mut add = |prefix: Option<String>, item: &UseItem<'_>| {
        let name = ident(&item.name);
        let full = match &prefix {
            Some(p) => format!("{p}\\{name}"),
            None => name.clone(),
        };
        let alias = match &item.alias {
            Some(a) => text(a.identifier.value),
            None => name.rsplit('\\').next().unwrap_or(&name).to_string(),
        };
        resolver.add_import(&alias, &full);
        b.import(alias, full);
    };
    match &u.items {
        UseItems::Sequence(s) => s.items.iter().for_each(|i| add(None, i)),
        UseItems::TypedSequence(s) => s.items.iter().for_each(|i| add(None, i)),
        UseItems::TypedList(l) => l
            .items
            .iter()
            .for_each(|i| add(Some(ident(&l.namespace)), i)),
        UseItems::MixedList(l) => l
            .items
            .iter()
            .for_each(|i| add(Some(ident(&l.namespace)), &i.item)),
    }
}

#[allow(clippy::too_many_arguments)]
fn container(
    parsed: &ParsedFile<'_>,
    kind: &str,
    name: &[u8],
    span: mago_span::Span,
    (is_abstract, is_final): (bool, bool),
    (extends, implements): (Vec<String>, Vec<String>),
    members: &Sequence<'_, ClassLikeMember<'_>>,
    resolver: &NameResolver,
    b: &mut FileGraphBuilder,
) {
    let name = text(name);
    let (line, end_line) = parsed.lines_of(&span);
    let mut traits = Vec::new();
    let collector = AnonymousCollector {
        parsed,
        resolver: resolver.clone(),
        closure_depth: AtomicUsize::new(0),
        saw_anonymous: AtomicBool::new(false),
    };
    let mut active_container = Some(name.clone());
    for m in members.iter() {
        match m {
            ClassLikeMember::TraitUse(t) => traits.extend(names(t.trait_names.iter(), resolver)),
            ClassLikeMember::Method(me) => {
                let mut current = method(parsed, active_container.as_deref().unwrap_or(""), me);
                if active_container.is_none() {
                    current.container = None;
                }
                b.method(current);
            }
            _ => {}
        }
        walker::walk_class_like_member(&collector, m, b);
        if collector.saw_anonymous.swap(false, Ordering::Relaxed) {
            active_container = None;
        }
    }
    b.container(Container {
        kind: kind.to_string(),
        name,
        line,
        end_line,
        is_abstract: Some(is_abstract),
        is_final: Some(is_final),
        extends: Some(extends),
        implements: Some(implements),
        traits: Some(traits),
        doc: doc_for(parsed, span),
    });
}

/// Típus-szöveg a referencia alakjában: a név eleji `\` elmarad (`?\Foo|\Bar` → `?Foo|Bar`).
fn hint_text(parsed: &ParsedFile<'_>, hint: &impl HasSpan) -> String {
    let raw = parsed.text(hint.span());
    let mut out = String::with_capacity(raw.len());
    let mut prev = '|';
    for ch in raw.chars() {
        let name_start = !(prev.is_alphanumeric() || prev == '_' || prev == '\\');
        if !(ch == '\\' && name_start) {
            out.push(ch);
        }
        prev = ch;
    }
    out
}

/// Paraméter a referencia (pretty-print) alakjában: `Type readonly &...$name = default`;
/// a láthatóság-módosító (promoted ctor) elmarad, a `readonly` megmarad.
fn param(parsed: &ParsedFile<'_>, p: &FunctionLikeParameter<'_>) -> String {
    let mut out = String::new();
    if let Some(h) = &p.hint {
        out.push_str(&hint_text(parsed, h));
        out.push(' ');
    }
    if p.modifiers
        .iter()
        .any(|m| matches!(m, Modifier::Readonly(_)))
    {
        out.push_str("readonly ");
    }
    if p.ampersand.is_some() {
        out.push('&');
    }
    if p.ellipsis.is_some() {
        out.push_str("...");
    }
    out.push_str(&parsed.text(p.variable.span()));
    if let Some(d) = &p.default_value {
        out.push_str(" = ");
        out.push_str(&default_to_string(parsed, d.value));
    }
    out
}

fn default_to_string(parsed: &ParsedFile<'_>, expression: &Expression<'_>) -> String {
    match expression {
        Expression::Literal(Literal::String(string)) => {
            format!(
                "'{}'",
                String::from_utf8_lossy(string.value.unwrap_or_default())
            )
        }
        Expression::Literal(Literal::Integer(integer)) => {
            String::from_utf8_lossy(integer.raw).into_owned()
        }
        Expression::Literal(Literal::Float(float)) => {
            let value = float.value.into_inner();
            if value.fract() == 0.0 {
                value.to_string()
            } else {
                parsed.text(float.span())
            }
        }
        Expression::Literal(Literal::True(_)) => "true".into(),
        Expression::Literal(Literal::False(_)) => "false".into(),
        Expression::Literal(Literal::Null(_)) => "null".into(),
        Expression::ConstantAccess(access) => parsed.text(access.span()),
        Expression::Access(Access::ClassConstant(access)) => parsed.text(access.span()),
        Expression::Array(array) if array.elements.is_empty() => "[]".into(),
        Expression::LegacyArray(array) if array.elements.is_empty() => "[]".into(),
        Expression::Array(_) | Expression::LegacyArray(_) => "[...]".into(),
        _ => "…".into(),
    }
}

fn method(parsed: &ParsedFile<'_>, container: &str, m: &mago_syntax::cst::Method<'_>) -> Method {
    let (line, end_line) = parsed.lines_of(m);
    let mut visibility = "public";
    for md in m.modifiers.iter() {
        match md {
            Modifier::Protected(_) => visibility = "protected",
            Modifier::Private(_) => visibility = "private",
            _ => {}
        }
    }
    Method {
        container: Some(container.to_string()),
        name: text(m.name.value),
        line,
        end_line: Some(end_line),
        visibility: Some(visibility.to_string()),
        is_static: m.is_static(),
        // Mago also reports interface methods as abstract; nikic's reference
        // only exposes the explicit `abstract` modifier.
        is_abstract: m
            .modifiers
            .iter()
            .any(|md| matches!(md, Modifier::Abstract(_))),
        params: m
            .parameter_list
            .parameters
            .iter()
            .map(|p| param(parsed, p))
            .collect(),
        return_type: m
            .return_type_hint
            .as_ref()
            .map(|r| hint_text(parsed, &r.hint)),
        doc_types: doc_types_for(parsed, m.span()),
        doc: doc_for(parsed, m.span()),
        body: body(parsed, m),
        kind: None,
    }
}

/// A metódus törzse: az első és az utolsó statement sorai egészében (`extractMethodBody`);
/// absztrakt vagy üres törzsnél `None`.
fn body(parsed: &ParsedFile<'_>, m: &mago_syntax::cst::Method<'_>) -> Option<String> {
    let MethodBody::Concrete(block) = &m.body else {
        return None;
    };
    if let (Some(first), Some(last)) = (block.statements.first(), block.statements.last()) {
        let mut end = last.span().end.offset;
        for trivia in parsed.program.trivia.iter() {
            if trivia.span.start.offset >= end
                && trivia.span.end.offset <= block.span().end.offset
                && matches!(
                    trivia.kind,
                    TriviaKind::SingleLineComment
                        | TriviaKind::MultiLineComment
                        | TriviaKind::HashComment
                )
            {
                end = trivia.span.end.offset;
            }
        }
        return Some(parsed.full_lines(first.span().start.offset, end));
    }
    // A comment-only body has no CST statements, but nikic still returns the
    // comment line as the method body.
    parsed
        .program
        .trivia
        .iter()
        .find(|t| {
            t.span.start.offset >= block.span().start.offset
                && t.span.end.offset <= block.span().end.offset
                && matches!(
                    t.kind,
                    TriviaKind::SingleLineComment
                        | TriviaKind::MultiLineComment
                        | TriviaKind::HashComment
                )
        })
        .map(|t| parsed.full_lines(t.span.start.offset, t.span.end.offset))
}
