//! `properties`: a Vue `props` (defineProps / Options `props:`), objektum-, tömb- és TS típus-literál alakkal.

use oxc_ast::ast::*;
use oxc_span::{GetSpan, Span};

use super::analysis::{JsAnalysis, JsPass, PassContext};
use crate::model::Property;

/// A `defineProps(...)` / `defineProps<T>()` és az Options API `props:` gyűjtője.
pub struct PropsPass;

impl JsPass for PropsPass {
    fn run(&self, ctx: &PassContext<'_>, program: &Program<'_>, out: &mut JsAnalysis) {
        let container = container_name(ctx);
        if let Some(options) = options_object(program) {
            for prop in &options.properties {
                if let ObjectPropertyKind::ObjectProperty(p) = prop
                    && key_name(&p.key) == Some("props")
                {
                    props_from(ctx, &container, &p.value, out);
                }
            }
        }
        for call in macro_calls(program, "defineProps") {
            match call.arguments.first() {
                Some(Argument::SpreadElement(_)) => {}
                Some(arg) => props_from(ctx, &container, arg.to_expression(), out),
                None => {
                    let ty = call.type_arguments.as_ref().and_then(|t| t.params.first());
                    if let Some(TSType::TSTypeLiteral(lit)) = ty {
                        type_members(ctx, &container, &lit.members, out);
                    }
                }
            }
        }
    }
}

/// A fő container neve: a fájlnév kiterjesztés nélkül.
pub(super) fn container_name(ctx: &PassContext<'_>) -> String {
    ctx.path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// A statikus kulcs neve (azonosító vagy string); más kulcsra `None`.
pub(super) fn key_name<'a>(key: &'a PropertyKey<'_>) -> Option<&'a str> {
    match key {
        PropertyKey::StaticIdentifier(id) => Some(id.name.as_str()),
        PropertyKey::StringLiteral(s) => s.value.as_str(),
        _ => None,
    }
}

/// Forrásrészlet egy sorra vonva, `max` karakterre vágva (`…` jelzéssel).
pub(super) fn src_of(source: &str, span: Span, max: usize) -> String {
    let raw = source
        .get(span.start as usize..span.end as usize)
        .unwrap_or("");
    let flat = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    let units: Vec<u16> = flat.encode_utf16().collect();
    if units.len() > max {
        let head = String::from_utf16_lossy(&units[..max - 1]);
        format!("{head}…")
    } else {
        flat
    }
}

/// Az `export default {…}` / `export default defineComponent({…})` opcióobjektuma.
pub(super) fn options_object<'a, 'b>(program: &'b Program<'a>) -> Option<&'b ObjectExpression<'a>> {
    program.body.iter().find_map(|stmt| {
        let Statement::ExportDefaultDeclaration(d) = stmt else {
            return None;
        };
        match &d.declaration {
            ExportDefaultDeclarationKind::ObjectExpression(o) => Some(&**o),
            ExportDefaultDeclarationKind::CallExpression(c) => match c.arguments.first() {
                Some(Argument::ObjectExpression(o)) => Some(&**o),
                _ => None,
            },
            _ => None,
        }
    })
}

/// A top-level utasításokból elérhető `name(...)` hívások (a `withDefaults(...)` argumentumaiba is belép).
pub(super) fn macro_calls<'a, 'b>(
    program: &'b Program<'a>,
    name: &str,
) -> Vec<&'b CallExpression<'a>> {
    let mut found = Vec::new();
    for stmt in &program.body {
        match stmt {
            Statement::ExpressionStatement(s) => collect_calls(&s.expression, name, &mut found),
            Statement::VariableDeclaration(v) => collect_decl(v, name, &mut found),
            Statement::ExportDeclaration(e) => {
                if let Declaration::VariableDeclaration(v) = &e.declaration {
                    collect_decl(v, name, &mut found);
                }
            }
            _ => {}
        }
    }
    found
}

fn collect_decl<'a, 'b>(
    decl: &'b VariableDeclaration<'a>,
    name: &str,
    found: &mut Vec<&'b CallExpression<'a>>,
) {
    for d in &decl.declarations {
        if let Some(init) = &d.init {
            collect_calls(init, name, found);
        }
    }
}

fn collect_calls<'a, 'b>(
    expr: &'b Expression<'a>,
    name: &str,
    found: &mut Vec<&'b CallExpression<'a>>,
) {
    match expr {
        Expression::CallExpression(call) => {
            if matches!(&call.callee, Expression::Identifier(id) if id.name == name) {
                found.push(call);
            }
            for arg in &call.arguments {
                if !matches!(arg, Argument::SpreadElement(_)) {
                    collect_calls(arg.to_expression(), name, found);
                }
            }
        }
        Expression::AwaitExpression(a) => collect_calls(&a.argument, name, found),
        Expression::ParenthesizedExpression(p) => collect_calls(&p.expression, name, found),
        Expression::TSAsExpression(t) => collect_calls(&t.expression, name, found),
        Expression::TSNonNullExpression(t) => collect_calls(&t.expression, name, found),
        _ => {}
    }
}

fn props_from(ctx: &PassContext<'_>, container: &str, expr: &Expression<'_>, out: &mut JsAnalysis) {
    match expr {
        Expression::ObjectExpression(obj) => {
            for prop in &obj.properties {
                if let ObjectPropertyKind::ObjectProperty(p) = prop
                    && let Some(name) = key_name(&p.key)
                {
                    let (ty, required) = object_prop_type(ctx.source, p);
                    push(ctx, container, name, p.span.start, ty, required, out);
                }
            }
        }
        Expression::ArrayExpression(arr) => {
            for el in &arr.elements {
                if let ArrayExpressionElement::StringLiteral(s) = el
                    && let Some(name) = s.value.as_str()
                {
                    push(
                        ctx,
                        container,
                        name,
                        s.span.start,
                        "mixed".into(),
                        false,
                        out,
                    );
                }
            }
        }
        _ => {}
    }
}

/// Objektum-alakú prop típusa (a `type` kulcs forrása vagy a rövid alak értéke) és a `required` jelző.
fn object_prop_type(source: &str, p: &ObjectProperty<'_>) -> (String, bool) {
    if p.method {
        return ("mixed".into(), false);
    }
    let (ty, required) = match &p.value {
        Expression::ObjectExpression(o) => {
            let mut ty = String::new();
            let mut required = false;
            for q in &o.properties {
                let ObjectPropertyKind::ObjectProperty(q) = q else {
                    continue;
                };
                match key_name(&q.key) {
                    Some("type") if ty.is_empty() => ty = src_of(source, q.value.span(), 40),
                    Some("required") => {
                        required = matches!(&q.value, Expression::BooleanLiteral(b) if b.value)
                    }
                    _ => {}
                }
            }
            (ty, required)
        }
        other => (src_of(source, other.span(), 40), false),
    };
    (if ty.is_empty() { "mixed".into() } else { ty }, required)
}

/// TS típus-literál tagjai (`readonly` = nem opcionális).
fn type_members(
    ctx: &PassContext<'_>,
    container: &str,
    members: &[TSSignature<'_>],
    out: &mut JsAnalysis,
) {
    for m in members {
        let (key, span, optional, ty) = match m {
            TSSignature::TSPropertySignature(p) => (
                &p.key,
                p.span,
                p.optional,
                p.type_annotation
                    .as_ref()
                    .map(|a| src_of(ctx.source, a.type_annotation.span(), 40)),
            ),
            TSSignature::TSMethodSignature(p) => (
                &p.key,
                p.span,
                p.optional,
                p.return_type
                    .as_ref()
                    .map(|a| src_of(ctx.source, a.type_annotation.span(), 40)),
            ),
            _ => continue,
        };
        if let Some(name) = key_name(key) {
            let ty = ty.unwrap_or_else(|| "mixed".into());
            push(ctx, container, name, span.start, ty, !optional, out);
        }
    }
}

fn push(
    ctx: &PassContext<'_>,
    container: &str,
    name: &str,
    offset: u32,
    ty: String,
    readonly: bool,
    out: &mut JsAnalysis,
) {
    out.builder.property(Property {
        js: true,
        container: Some(container.to_string()),
        name: name.to_string(),
        line: ctx.lines.line_of(offset as usize),
        type_hint: Some(ty),
        visibility: "public".into(),
        is_static: false,
        readonly,
        doc: None,
    });
}
