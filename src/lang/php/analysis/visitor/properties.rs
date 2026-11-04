//! Property-k: deklarált property-k és a promoted constructor paraméterek (`properties`).

use mago_span::HasSpan;
use mago_syntax::cst::Statement;
use mago_syntax::cst::cst::*;
use mago_syntax::cst::sequence::Sequence;
use mago_syntax::walker;

use super::FileVisitor;
use crate::lang::php::analysis::AnalysisContext;
use crate::lang::php::analysis::docblock::doc_for;
use crate::lang::php::parser::ParsedFile;
use crate::model::{FileGraphBuilder, Property as PropertyRow};

/// A `properties` mezőt tölti.
#[derive(Debug, Default, Clone, Copy)]
pub struct PropertiesVisitor;

impl FileVisitor for PropertiesVisitor {
    fn visit(
        &self,
        parsed: &ParsedFile<'_>,
        _ctx: &mut AnalysisContext,
        builder: &mut FileGraphBuilder,
    ) {
        walk(parsed, &parsed.program.statements, builder);
        walker::walk_program(&AnonymousProperties { parsed }, parsed.program, builder);
    }
}

struct AnonymousProperties<'a> {
    parsed: &'a ParsedFile<'a>,
}

impl<'a, 'arena> walker::Walker<'a, 'arena, FileGraphBuilder> for AnonymousProperties<'a> {
    fn walk_in_anonymous_class(
        &self,
        node: &mago_syntax::cst::cst::AnonymousClass<'arena>,
        builder: &mut FileGraphBuilder,
    ) {
        members(self.parsed, b"<anonymous>", &node.members, builder);
    }
}

fn walk(parsed: &ParsedFile<'_>, stmts: &Sequence<'_, Statement<'_>>, b: &mut FileGraphBuilder) {
    for st in stmts.iter() {
        match st {
            Statement::Namespace(ns) => walk(parsed, ns.statements(), b),
            Statement::Class(c) => members(parsed, c.name.value, &c.members, b),
            Statement::Interface(i) => members(parsed, i.name.value, &i.members, b),
            Statement::Trait(t) => members(parsed, t.name.value, &t.members, b),
            Statement::Enum(e) => members(parsed, e.name.value, &e.members, b),
            _ => {}
        }
    }
}

fn members(
    parsed: &ParsedFile<'_>,
    name: &[u8],
    members: &Sequence<'_, ClassLikeMember<'_>>,
    b: &mut FileGraphBuilder,
) {
    let container = String::from_utf8_lossy(name).into_owned();
    for m in members.iter() {
        match m {
            ClassLikeMember::Property(p) => property(parsed, &container, p, b),
            ClassLikeMember::Method(me) => {
                for p in me.parameter_list.parameters.iter() {
                    if p.modifiers.is_empty() {
                        continue;
                    }
                    promoted(parsed, &container, p, b);
                }
            }
            _ => {}
        }
    }
}

/// Típus-szöveg a referencia alakjában: szóköz nélkül, név eleji `\` nélkül.
fn type_text(parsed: &ParsedFile<'_>, hint: &impl HasSpan) -> String {
    let raw = parsed.text(hint.span());
    let mut out = String::with_capacity(raw.len());
    let mut prev = '|';
    for ch in raw.chars().filter(|c| !c.is_whitespace()) {
        let name_start = !(prev.is_alphanumeric() || prev == '_' || prev == '\\');
        if !(ch == '\\' && name_start) {
            out.push(ch);
        }
        prev = ch;
    }
    out
}

fn var_name(raw: &[u8]) -> String {
    String::from_utf8_lossy(raw)
        .trim_start_matches('$')
        .to_string()
}

/// (láthatóság, static, readonly) a módosítókból; a láthatóság alapértéke `public`.
fn flags<'a>(mods: impl Iterator<Item = &'a Modifier<'a>>) -> (&'static str, bool, bool) {
    let (mut vis, mut is_static, mut readonly) = ("public", false, false);
    for m in mods {
        match m {
            Modifier::Protected(_) => vis = "protected",
            Modifier::Private(_) => vis = "private",
            Modifier::Static(_) => is_static = true,
            Modifier::Readonly(_) => readonly = true,
            _ => {}
        }
    }
    (vis, is_static, readonly)
}

fn property(parsed: &ParsedFile<'_>, container: &str, p: &Property<'_>, b: &mut FileGraphBuilder) {
    let (mods, hint, vars, span) = match p {
        Property::Plain(pl) => (
            &pl.modifiers,
            &pl.hint,
            pl.items
                .iter()
                .map(|i| match i {
                    PropertyItem::Abstract(a) => a.variable.name,
                    PropertyItem::Concrete(c) => c.variable.name,
                })
                .collect::<Vec<_>>(),
            pl.span(),
        ),
        Property::Hooked(h) => (
            &h.modifiers,
            &h.hint,
            vec![match &h.item {
                PropertyItem::Abstract(a) => a.variable.name,
                PropertyItem::Concrete(c) => c.variable.name,
            }],
            h.span(),
        ),
    };
    let (visibility, is_static, readonly) = flags(mods.iter());
    let (line, _) = parsed.lines_of(&span);
    let type_hint = hint.as_ref().map(|h| type_text(parsed, h));
    for v in vars {
        b.property(PropertyRow {
            js: false,
            container: Some(container.to_string()),
            name: var_name(v),
            line,
            type_hint: type_hint.clone(),
            visibility: visibility.to_string(),
            is_static,
            readonly,
            doc: doc_for(parsed, span),
        });
    }
}

fn promoted(
    parsed: &ParsedFile<'_>,
    container: &str,
    p: &FunctionLikeParameter<'_>,
    b: &mut FileGraphBuilder,
) {
    let (visibility, _, readonly) = flags(p.modifiers.iter());
    let (line, _) = parsed.lines_of(p);
    b.property(PropertyRow {
        js: false,
        container: Some(container.to_string()),
        name: var_name(p.variable.name),
        line,
        type_hint: p.hint.as_ref().map(|h| type_text(parsed, h)),
        visibility: visibility.to_string(),
        is_static: false,
        readonly,
        doc: doc_for(parsed, p.span()),
    });
}
