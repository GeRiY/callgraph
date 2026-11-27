//! `properties`: a Vue `props` (defineProps / Options `props:`), objektum-, tömb- és TS típus-literál alakkal.

use std::collections::HashMap;
use std::sync::Arc;

use oxc_ast::ast::*;
use oxc_span::{GetSpan, Span};

use super::analysis::{JsAnalysis, JsPass, PassContext};
use super::lines::LineIndex;
use super::parse::{parse_program, source_type_for};
use super::resolve::Resolver;
use crate::model::Property;

/// A `defineProps(...)` / `defineProps<T>()` és az Options API `props:` gyűjtője.
pub struct PropsPass {
    resolver: Arc<Resolver>,
}

impl PropsPass {
    /// The resolver finds the files of imported prop types (`defineProps<ImportedProps>()`).
    pub fn new(resolver: Arc<Resolver>) -> Self {
        Self { resolver }
    }
}

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
        let mut defaults = with_defaults(ctx, program);
        for (name, value) in destructure_defaults(ctx, program) {
            defaults.entry(name).or_insert(value);
        }
        for call in macro_calls(program, "defineProps") {
            match call.arguments.first() {
                Some(Argument::SpreadElement(_)) => {}
                Some(arg) => props_from(ctx, &container, arg.to_expression(), out),
                None => {
                    if let Some(ty) = call.type_arguments.as_ref().and_then(|t| t.params.first()) {
                        let scope = TypeScope {
                            ctx,
                            program,
                            container: &container,
                            defaults: &defaults,
                            resolver: &self.resolver,
                            line: None,
                            call_line: ctx.lines.line_of(call.span.start as usize),
                        };
                        scope.collect(ty, 0, out);
                    }
                }
            }
        }
    }
}

/// A `withDefaults(defineProps<…>(), { név: érték })` alapértékei: prop-név → az érték rövidített forrása.
fn with_defaults(ctx: &PassContext<'_>, program: &Program<'_>) -> HashMap<String, String> {
    let mut defaults = HashMap::new();
    for call in macro_calls(program, "withDefaults") {
        let Some(Argument::ObjectExpression(obj)) = call.arguments.get(1) else {
            continue;
        };
        for prop in &obj.properties {
            if let ObjectPropertyKind::ObjectProperty(p) = prop
                && let Some(name) = key_name(&p.key)
            {
                let value = if p.method {
                    "fn".to_string()
                } else {
                    src_of(ctx.source, p.value.span(), 30)
                };
                defaults.insert(name.to_string(), value);
            }
        }
    }
    defaults
}

/// Vue 3.5 reactive props destructure: `const { size = 'md' } = defineProps<…>()` -> prop name -> shortened default.
fn destructure_defaults(ctx: &PassContext<'_>, program: &Program<'_>) -> HashMap<String, String> {
    let mut defaults = HashMap::new();
    for stmt in &program.body {
        let Statement::VariableDeclaration(v) = stmt else {
            continue;
        };
        for d in &v.declarations {
            let (BindingPattern::ObjectPattern(o), Some(init)) = (&d.id, &d.init) else {
                continue;
            };
            let mut found = Vec::new();
            collect_calls(init, "defineProps", &mut found);
            if found.is_empty() {
                continue;
            }
            for p in &o.properties {
                if p.computed {
                    continue;
                }
                if let (Some(name), BindingPattern::AssignmentPattern(a)) =
                    (key_name(&p.key), &p.value)
                {
                    defaults.insert(name.to_string(), src_of(ctx.source, a.right.span(), 40));
                }
            }
        }
    }
    defaults
}

/// A `defineProps<T>()` típus-paraméterének feloldása a fájlon belül (típus-literál, helyi `interface` / `type`,
/// `extends`, `A & B`) és az importált típusé (relatív / alias-os import; a csomagból jövőt nem követi).
struct TypeScope<'s, 'a, 'c> {
    ctx: &'s PassContext<'c>,
    program: &'s Program<'a>,
    container: &'s str,
    defaults: &'s HashMap<String, String>,
    resolver: &'s Resolver,
    /// Importált típusnál a `defineProps` sora (a tagok ide kerülnek); helyi típusnál `None`.
    line: Option<u32>,
    /// A `defineProps` hívás sora.
    call_line: u32,
}

/// The biggest file read for an imported prop type.
const MAX_TYPE_FILE: u64 = 1_500_000;

impl TypeScope<'_, '_, '_> {
    fn collect(&self, ty: &TSType<'_>, depth: usize, out: &mut JsAnalysis) {
        // önhivatkozó / végtelen típus-láncok ellen
        if depth > 5 {
            return;
        }
        match ty {
            TSType::TSTypeLiteral(lit) => self.members(&lit.members, out),
            TSType::TSIntersectionType(i) => {
                for part in &i.types {
                    self.collect(part, depth + 1, out);
                }
            }
            TSType::TSParenthesizedType(p) => self.collect(&p.type_annotation, depth + 1, out),
            TSType::TSTypeReference(r) => {
                if let TSTypeName::IdentifierReference(id) = &r.type_name {
                    self.named(id.name.as_str(), depth + 1, out);
                }
            }
            _ => {}
        }
    }

    /// A fájl top-level `interface <név>` / `type <név> = …` deklarációja (exportálva is).
    fn named(&self, name: &str, depth: usize, out: &mut JsAnalysis) {
        for stmt in &self.program.body {
            let decl = match stmt {
                Statement::ExportDeclaration(e) => Some(&e.declaration),
                other => other.as_declaration(),
            };
            match decl {
                Some(Declaration::TSInterfaceDeclaration(i)) if i.id.name == name => {
                    for parent in &i.extends {
                        if let TSTypeName::IdentifierReference(id) = &parent.type_name {
                            self.named(id.name.as_str(), depth + 1, out);
                        }
                    }
                    self.members(&i.body.body, out);
                    return;
                }
                Some(Declaration::TSTypeAliasDeclaration(a)) if a.id.name == name => {
                    self.collect(&a.type_annotation, depth + 1, out);
                    return;
                }
                _ => {}
            }
        }
        self.imported(name, depth, out);
    }

    /// `import type { Props } from './types'`: the interface / type alias is read from the imported file (a local
    /// relative or aliased import; the members are attributed to the `defineProps` line).
    fn imported(&self, name: &str, depth: usize, out: &mut JsAnalysis) {
        // circular imports
        if depth > 5 {
            return;
        }
        for stmt in &self.program.body {
            let Statement::ImportDeclaration(decl) = stmt else {
                continue;
            };
            for spec in decl.specifiers.iter().flatten() {
                let (local, remote) = match spec {
                    ImportDeclarationSpecifier::ImportSpecifier(s) => {
                        (s.local.name.as_str(), s.imported.name().to_string())
                    }
                    ImportDeclarationSpecifier::ImportDefaultSpecifier(s) => {
                        (s.local.name.as_str(), "default".to_string())
                    }
                    ImportDeclarationSpecifier::ImportNamespaceSpecifier(_) => continue,
                };
                if local != name || remote == "default" {
                    continue;
                }
                let source = decl.source.value.display().to_string();
                let Some(file) = self.resolver.resolve_file(&source, self.ctx.path) else {
                    return;
                };
                if file.extension().is_some_and(|ext| ext == "vue")
                    || std::fs::metadata(&file)
                        .ok()
                        .is_none_or(|m| m.len() > MAX_TYPE_FILE)
                {
                    return;
                }
                let Ok(text) = std::fs::read_to_string(&file) else {
                    return;
                };
                let allocator = oxc_allocator::Allocator::default();
                let parsed = parse_program(&allocator, &text, source_type_for(&file, None));
                let lines = LineIndex::new(&text);
                let ctx = PassContext {
                    path: &file,
                    source: &text,
                    lines: &lines,
                    setup: false,
                };
                let scope = TypeScope {
                    ctx: &ctx,
                    program: &parsed.program,
                    container: self.container,
                    defaults: self.defaults,
                    resolver: self.resolver,
                    line: Some(self.call_line),
                    call_line: self.call_line,
                };
                scope.named(&remote, depth + 1, out);
                return;
            }
        }
    }

    fn members(&self, members: &[TSSignature<'_>], out: &mut JsAnalysis) {
        type_members(
            self.ctx,
            self.container,
            members,
            self.defaults,
            self.line,
            out,
        );
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
                    let (ty, required, default) = object_prop_type(ctx.source, p);
                    push(
                        container,
                        name,
                        ctx.lines.line_of(p.span.start as usize),
                        ty,
                        required,
                        default,
                        out,
                    );
                }
            }
        }
        Expression::ArrayExpression(arr) => {
            for el in &arr.elements {
                if let ArrayExpressionElement::StringLiteral(s) = el
                    && let Some(name) = s.value.as_str()
                {
                    push(
                        container,
                        name,
                        ctx.lines.line_of(s.span.start as usize),
                        "mixed".into(),
                        false,
                        None,
                        out,
                    );
                }
            }
        }
        _ => {}
    }
}

/// Objektum-alakú prop típusa (a `type` kulcs forrása vagy a rövid alak értéke) és a `required` jelző.
fn object_prop_type(source: &str, p: &ObjectProperty<'_>) -> (String, bool, Option<String>) {
    if p.method {
        return ("mixed".into(), false, None);
    }
    let (ty, required, default) = match &p.value {
        Expression::ObjectExpression(o) => {
            let mut ty = String::new();
            let mut required = false;
            let mut default = None;
            for q in &o.properties {
                let ObjectPropertyKind::ObjectProperty(q) = q else {
                    continue;
                };
                match key_name(&q.key) {
                    Some("type") if ty.is_empty() => ty = src_of(source, q.value.span(), 40),
                    Some("required") => {
                        required = matches!(&q.value, Expression::BooleanLiteral(b) if b.value)
                    }
                    Some("default") => {
                        default = Some(if q.method {
                            "fn".to_string()
                        } else {
                            src_of(source, q.value.span(), 40)
                        })
                    }
                    _ => {}
                }
            }
            (ty, required, default)
        }
        other => (src_of(source, other.span(), 40), false, None),
    };
    (
        if ty.is_empty() { "mixed".into() } else { ty },
        required,
        default,
    )
}

/// TS típus-literál tagjai (`readonly` = nem opcionális).
fn type_members(
    ctx: &PassContext<'_>,
    container: &str,
    members: &[TSSignature<'_>],
    defaults: &HashMap<String, String>,
    line: Option<u32>,
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
            // `withDefaults` alapérték: külön tárolva (`doc`), és a prop nem kötelező
            let default = defaults.get(name);
            // a type imported from another file: its members sit on the `defineProps` line
            let line = line.unwrap_or_else(|| ctx.lines.line_of(span.start as usize));
            push(
                container,
                name,
                line,
                ty,
                !optional && default.is_none(),
                default.cloned(),
                out,
            );
        }
    }
}

/// A `default` a prop alapértéke (`withDefaults`); a `Property.default` mezőbe kerül.
#[allow(clippy::too_many_arguments)]
fn push(
    container: &str,
    name: &str,
    line: u32,
    ty: String,
    readonly: bool,
    default: Option<String>,
    out: &mut JsAnalysis,
) {
    out.builder.property(Property {
        js: true,
        container: Some(container.to_string()),
        name: name.to_string(),
        line,
        type_hint: Some(ty),
        visibility: "public".into(),
        is_static: false,
        readonly,
        doc: None,
        default,
    });
}
