//! Osztálykonstansok és enum case-ek gyűjtése.

use mago_span::HasSpan;
use mago_syntax::cst::cst::*;
use mago_syntax::cst::{Expression, Literal, Statement};
use mago_syntax::walker;

use super::FileVisitor;
use crate::lang::php::analysis::AnalysisContext;
use crate::lang::php::analysis::docblock::doc_for;
use crate::lang::php::parser::ParsedFile;
use crate::model::{Constant, FileGraphBuilder};

/// A `constants` mezőt tölti.
#[derive(Debug, Default, Clone, Copy)]
pub struct ConstantsVisitor;

impl FileVisitor for ConstantsVisitor {
    fn visit(
        &self,
        parsed: &ParsedFile<'_>,
        _ctx: &mut AnalysisContext,
        builder: &mut FileGraphBuilder,
    ) {
        for statement in parsed.program.statements.iter() {
            visit_statement(parsed, statement, builder);
        }
        walker::walk_program(&AnonymousConstants { parsed }, parsed.program, builder);
    }
}

struct AnonymousConstants<'a> {
    parsed: &'a ParsedFile<'a>,
}

impl<'a, 'arena> walker::Walker<'a, 'arena, FileGraphBuilder> for AnonymousConstants<'a> {
    fn walk_in_anonymous_class(
        &self,
        node: &mago_syntax::cst::cst::AnonymousClass<'arena>,
        builder: &mut FileGraphBuilder,
    ) {
        visit_members(self.parsed, b"<anonymous>", &node.members, builder);
    }
}

fn visit_statement(
    parsed: &ParsedFile<'_>,
    statement: &Statement<'_>,
    builder: &mut FileGraphBuilder,
) {
    match statement {
        Statement::Namespace(namespace) => {
            for statement in namespace.statements().iter() {
                visit_statement(parsed, statement, builder);
            }
        }
        Statement::Class(class) => visit_members(parsed, class.name.value, &class.members, builder),
        Statement::Interface(interface) => {
            visit_members(parsed, interface.name.value, &interface.members, builder)
        }
        Statement::Trait(trait_) => {
            visit_members(parsed, trait_.name.value, &trait_.members, builder)
        }
        Statement::Enum(enum_) => visit_members(parsed, enum_.name.value, &enum_.members, builder),
        _ => {}
    }
}

fn visit_members(
    parsed: &ParsedFile<'_>,
    container: &[u8],
    members: &mago_syntax::cst::sequence::Sequence<'_, ClassLikeMember<'_>>,
    builder: &mut FileGraphBuilder,
) {
    let container = String::from_utf8_lossy(container).into_owned();
    for member in members.iter() {
        match member {
            ClassLikeMember::Constant(constant) => {
                let visibility = visibility(&constant.modifiers);
                let line = parsed.lines_of(constant).0;
                for item in constant.items.iter() {
                    builder.constant(Constant {
                        js: false,
                        container: Some(container.clone()),
                        kind: "const".into(),
                        name: String::from_utf8_lossy(item.name.value).into_owned(),
                        line,
                        value: Some(default_to_string(parsed, item.value)),
                        visibility: Some(visibility.to_string()),
                        caller: None,
                        doc: doc_for(parsed, constant.span()),
                    });
                }
            }
            ClassLikeMember::EnumCase(case) => {
                let line = parsed.lines_of(case).0;
                let value = match &case.item {
                    EnumCaseItem::Unit(_) => None,
                    EnumCaseItem::Backed(item) => Some(default_to_string(parsed, item.value)),
                };
                builder.constant(Constant {
                    js: false,
                    container: Some(container.clone()),
                    kind: "enum_case".into(),
                    name: String::from_utf8_lossy(case.item.name().value).into_owned(),
                    line,
                    value,
                    visibility: Some("public".into()),
                    caller: None,
                    doc: doc_for(parsed, case.span()),
                });
            }
            _ => {}
        }
    }
}

fn visibility(modifiers: &mago_syntax::cst::sequence::Sequence<'_, Modifier<'_>>) -> &'static str {
    modifiers
        .iter()
        .find_map(|modifier| match modifier {
            Modifier::Protected(_) => Some("protected"),
            Modifier::Private(_) => Some("private"),
            Modifier::Public(_) => Some("public"),
            _ => None,
        })
        .unwrap_or("public")
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
        Expression::Literal(Literal::Null(_)) => "null".into(),
        Expression::Literal(Literal::True(_)) => "true".into(),
        Expression::Literal(Literal::False(_)) => "false".into(),
        Expression::ConstantAccess(access) => parsed.text(access.span()),
        Expression::Array(array) if array.elements.is_empty() => "[]".into(),
        Expression::LegacyArray(array) if array.elements.is_empty() => "[]".into(),
        Expression::Access(Access::ClassConstant(access)) => parsed.text(access.span()),
        Expression::Array(_) | Expression::LegacyArray(_) => "[...]".into(),
        _ => "…".into(),
    }
}
