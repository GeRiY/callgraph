//! `constants`: top-level `UPPER_SNAKE` const, TS enum-tagok és a Vue emitek (`emit:<név>`).

use std::collections::BTreeSet;
use std::sync::LazyLock;

use oxc_ast::ast::*;
use oxc_span::GetSpan;
use regex::Regex;

use super::analysis::{JsAnalysis, JsPass, PassContext};
use super::props::{container_name, key_name, macro_calls, options_object, src_of};
use crate::model::Constant;

static CALL_SIGNATURE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"\(\s*\w+\s*:\s*['"]([\w:.-]+)['"]"#).unwrap());
static NAMED_TUPLE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?m)^\s*['"]?([\w:.-]+)['"]?\s*:\s*\["#).unwrap());
static UPPER_SNAKE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Z][A-Z0-9_]+$").unwrap());

/// A `const` / `enum` / `defineEmits` / Options `emits:` gyűjtője.
pub struct ConstantsPass;

impl JsPass for ConstantsPass {
    fn run(&self, ctx: &PassContext<'_>, program: &Program<'_>, out: &mut JsAnalysis) {
        let container = container_name(ctx);
        for stmt in &program.body {
            let decl = match stmt {
                Statement::ExportDeclaration(e) => Some(&e.declaration),
                other => other.as_declaration(),
            };
            if let Some(decl) = decl {
                declaration(ctx, &container, decl, out);
            }
        }
        if let Some(options) = options_object(program) {
            for prop in &options.properties {
                if let ObjectPropertyKind::ObjectProperty(p) = prop
                    && key_name(&p.key) == Some("emits")
                {
                    let names = emit_names(&p.value);
                    let line = ctx.lines.line_of(p.value.span().start as usize);
                    push_emits(&container, line, names, out);
                }
            }
        }
        for call in macro_calls(program, "defineEmits") {
            let arg = match call.arguments.first() {
                Some(Argument::SpreadElement(_)) | None => None,
                Some(a) => Some(a.to_expression()),
            };
            let type_src = call
                .type_arguments
                .as_ref()
                .map(|t| src_text(ctx.source, t.span));
            let mut names = arg.map(|e| emit_names(e)).unwrap_or_default();
            if let Some(src) = &type_src {
                names.extend(type_emit_names(src));
            }
            let line = arg.map_or(0, |e| ctx.lines.line_of(e.span().start as usize));
            push_emits(&container, line, names, out);
        }
    }
}

fn src_text(source: &str, span: oxc_span::Span) -> String {
    source
        .get(span.start as usize..span.end as usize)
        .unwrap_or("")
        .to_string()
}

/// A tömb / objektum alakú emit-lista nevei.
fn emit_names(expr: &Expression<'_>) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    match expr {
        Expression::ArrayExpression(arr) => {
            for el in &arr.elements {
                if let ArrayExpressionElement::StringLiteral(s) = el
                    && let Some(v) = s.value.as_str()
                {
                    names.insert(v.to_string());
                }
            }
        }
        Expression::ObjectExpression(obj) => {
            for p in &obj.properties {
                if let ObjectPropertyKind::ObjectProperty(p) = p
                    && let Some(k) = key_name(&p.key)
                {
                    names.insert(k.to_string());
                }
            }
        }
        _ => {}
    }
    names
}

/// A TS típus-paraméter forrásából: hívás-szignatúra `(e: 'x'` és `'x': [` alak.
fn type_emit_names(src: &str) -> BTreeSet<String> {
    let calls = CALL_SIGNATURE.captures_iter(src);
    let tuples = NAMED_TUPLE.captures_iter(src);
    calls.chain(tuples).map(|c| c[1].to_string()).collect()
}

fn push_emits(container: &str, line: u32, names: BTreeSet<String>, out: &mut JsAnalysis) {
    for name in names {
        out.builder.constant(Constant {
            js: true,
            container: Some(container.to_string()),
            kind: "emit".into(),
            name: format!("emit:{name}"),
            line,
            value: None,
            visibility: None,
            caller: None,
            doc: None,
        });
    }
}

fn declaration(
    ctx: &PassContext<'_>,
    container: &str,
    decl: &Declaration<'_>,
    out: &mut JsAnalysis,
) {
    match decl {
        Declaration::VariableDeclaration(v) => {
            for d in &v.declarations {
                let BindingPattern::BindingIdentifier(id) = &d.id else {
                    continue;
                };
                let literal = matches!(
                    &d.init,
                    Some(
                        Expression::StringLiteral(_)
                            | Expression::NumericLiteral(_)
                            | Expression::BooleanLiteral(_)
                            | Expression::TemplateLiteral(_)
                            | Expression::ArrayExpression(_)
                            | Expression::ObjectExpression(_)
                    )
                );
                if let (true, Some(init)) = (literal, &d.init)
                    && UPPER_SNAKE.is_match(&id.name)
                {
                    let value = Some(src_of(ctx.source, init.span(), 80));
                    push(ctx, container, "const", &id.name, d.span.start, value, out);
                }
            }
        }
        Declaration::TSEnumDeclaration(e) => {
            for m in &e.body.members {
                let key = match &m.id {
                    TSEnumMemberName::Identifier(i) => i.name.as_str(),
                    TSEnumMemberName::String(s) => match s.value.as_str() {
                        Some(v) => v,
                        None => continue,
                    },
                    _ => continue,
                };
                let value = m
                    .initializer
                    .as_ref()
                    .map(|i| src_of(ctx.source, i.span(), 80));
                let name = format!("{}.{key}", e.id.name);
                push(ctx, container, "enum_case", &name, m.span.start, value, out);
            }
        }
        _ => {}
    }
}

fn push(
    ctx: &PassContext<'_>,
    container: &str,
    kind: &str,
    name: &str,
    offset: u32,
    value: Option<String>,
    out: &mut JsAnalysis,
) {
    out.builder.constant(Constant {
        js: true,
        container: Some(container.to_string()),
        kind: kind.into(),
        name: name.to_string(),
        line: ctx.lines.line_of(offset as usize),
        value,
        visibility: None,
        caller: None,
        doc: None,
    });
}
