//! A top-level utasítások metódusai: függvény, `const x = fn`, wrapper, hook, osztály.

use oxc_ast::ast::{
    Argument, BindingPattern, ClassElement, Declaration, Expression, FormalParameter,
    MethodDefinitionKind, ObjectExpression, ObjectPropertyKind, PropertyDefinition, PropertyKey,
    Statement, TSAccessibility, TSTypeAnnotation, VariableDeclaration,
};
use oxc_span::{GetSpan, Span};

use super::collector::{Collector, FnRef, Opts};
use crate::model::{Container, Property};

const FN_WRAPPERS: [&str; 6] = [
    "computed",
    "useMemo",
    "debounce",
    "throttle",
    "useDebounceFn",
    "useThrottleFn",
];

/// A hook-név (`onXxx`, `watch*`).
fn is_hook(name: &str) -> bool {
    matches!(
        name,
        "watch" | "watchEffect" | "watchPostEffect" | "watchSyncEffect"
    ) || name
        .strip_prefix("on")
        .and_then(|r| r.chars().next())
        .is_some_and(|c| c.is_ascii_uppercase())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// A kulcs neve (azonosító vagy string); más kulcs nem ad nevet.
pub fn key_name(k: &PropertyKey<'_>) -> Option<String> {
    match k {
        PropertyKey::StaticIdentifier(i) => Some(i.name.to_string()),
        PropertyKey::Identifier(i) => Some(i.name.to_string()),
        PropertyKey::StringLiteral(s) => s.value.as_str().map(str::to_string),
        _ => None,
    }
}

impl Collector<'_> {
    /// Egy top-level utasítás; `exported` esetén a deklaráció `public`.
    pub fn top_statement(&mut self, stmt: &Statement<'_>, exported: bool) {
        match stmt {
            Statement::ExpressionStatement(e) if !exported => self.hook(e.span, &e.expression),
            _ => {
                if let Some(d) = stmt.as_declaration() {
                    self.top_declaration(d, exported);
                }
            }
        }
    }

    pub fn top_declaration(&mut self, decl: &Declaration<'_>, exported: bool) {
        let vis = if exported { "public" } else { "private" };
        match decl {
            Declaration::FunctionDeclaration(f) if f.body.is_some() => {
                if let Some(id) = &f.id {
                    let name =
                        self.add(&id.name, FnRef::Func(f), f.span, Opts::new(vis, "function"));
                    self.nested(&name, FnRef::Func(f));
                }
            }
            Declaration::VariableDeclaration(v) => self.variables(v, vis),
            Declaration::ClassDeclaration(c) => self.class(c, exported),
            _ => {}
        }
    }

    fn variables(&mut self, v: &VariableDeclaration<'_>, vis: &str) {
        let single = v.declarations.len() == 1;
        for d in &v.declarations {
            let (Some(id), Some(init)) = (d.id.get_binding_identifier(), d.init.as_ref()) else {
                continue;
            };
            let span: Span = if single { v.span } else { d.span };
            if let Some(f) = FnRef::of_expr(init) {
                let name = self.add(&id.name, f, span, Opts::new(vis, "function"));
                self.nested(&name, f);
            } else if let Expression::CallExpression(c) = init
                && let Expression::Identifier(callee) = &c.callee
                && FN_WRAPPERS.contains(&callee.name.as_str())
                && let Some(f) = c
                    .arguments
                    .first()
                    .and_then(Argument::as_expression)
                    .and_then(FnRef::of_expr)
            {
                self.add(&id.name, f, span, Opts::new(vis, &callee.name));
            } else if let Expression::ObjectExpression(o) = init.without_parentheses() {
                self.object_methods(&id.name, o, vis, 0);
            }
        }
    }

    /// Objektum-literál függvény-értékű kulcsai (`fetch() {}`, `save: () => …`, `load: function () {}`)
    /// metódusként: `<előtag>.<kulcs>` (üres előtagnál a kulcs); a beágyazott objektum egy szintig.
    pub fn object_methods(&mut self, prefix: &str, o: &ObjectExpression<'_>, vis: &str, depth: u8) {
        for p in &o.properties {
            let ObjectPropertyKind::ObjectProperty(p) = p else {
                continue;
            };
            let Some(k) = key_name(&p.key) else { continue };
            let name = if prefix.is_empty() {
                k
            } else {
                format!("{prefix}.{k}")
            };
            if let Some(f) = FnRef::of_expr(p.value.without_parentheses()) {
                self.add(&name, f, p.span, Opts::new(vis, "method"));
            } else if depth == 0
                && let Expression::ObjectExpression(inner) = p.value.without_parentheses()
            {
                self.object_methods(&name, inner, vis, depth + 1);
            }
        }
    }

    /// `onMounted(fn)` / `watch(src, fn)` kifejezés-utasítás: az utolsó függvény-argumentum a hook.
    fn hook(&mut self, stmt: Span, expr: &Expression<'_>) {
        let Expression::CallExpression(c) = expr else {
            return;
        };
        let Expression::Identifier(callee) = &c.callee else {
            return;
        };
        if !is_hook(&callee.name) {
            return;
        }
        let f = c
            .arguments
            .iter()
            .rev()
            .filter_map(Argument::as_expression)
            .find_map(FnRef::of_expr);
        if let Some(f) = f {
            let name = format!("{}@L{}", callee.name, self.line(stmt.start));
            self.add(&name, f, stmt, Opts::new("private", "hook"));
        }
    }

    /// Osztály: saját container és a függvény-értékű tagok metódusai.
    pub fn class(&mut self, c: &oxc_ast::ast::Class<'_>, exported: bool) {
        let Some(id) = &c.id else { return };
        let vis = if exported { "public" } else { "private" };
        let extends = c.heritage.as_ref().map(|h| {
            self.docs
                .src_of(h.expression.span().start, h.expression.span().end, 60)
        });
        self.containers.push(Container {
            kind: "class".into(),
            name: id.name.to_string(),
            line: self.line(c.span.start),
            end_line: self.line(c.span.end),
            is_abstract: Some(false),
            is_final: Some(false),
            extends: Some(extends.into_iter().collect()),
            implements: Some(Vec::new()),
            traits: Some(Vec::new()),
            doc: self.docs.doc_of(c.span.start),
        });
        for el in &c.body.body {
            match el {
                ClassElement::MethodDefinition(m) if m.value.body.is_some() => {
                    if m.kind == MethodDefinitionKind::Constructor {
                        for param in &m.value.params.items {
                            self.parameter_property(&id.name, param);
                        }
                    }
                    if let Some(k) = key_name(&m.key) {
                        let o = Opts {
                            container: Some(&id.name),
                            visibility: vis,
                            is_static: m.r#static,
                            kind: "method",
                        };
                        self.add(&k, FnRef::Func(&m.value), m.span, o);
                    }
                }
                ClassElement::PropertyDefinition(p) => {
                    let f = p.value.as_ref().and_then(FnRef::of_expr);
                    if let (Some(k), Some(f)) = (key_name(&p.key), f) {
                        let o = Opts {
                            container: Some(&id.name),
                            visibility: vis,
                            is_static: p.r#static,
                            kind: "method",
                        };
                        self.add(&k, f, p.span, o);
                    } else if f.is_none() {
                        self.field(&id.name, p);
                    }
                }
                _ => {}
            }
        }
    }

    /// Egy nem függvény-értékű osztálymező (`count = 0`, `private repo: Repo`, `#secret`) a `properties` közé.
    fn field(&mut self, class: &str, p: &PropertyDefinition<'_>) {
        let name = match &p.key {
            PropertyKey::PrivateIdentifier(i) => format!("#{}", i.name),
            key => match key_name(key) {
                Some(name) => name,
                None => return,
            },
        };
        let visibility = match p.accessibility {
            Some(TSAccessibility::Private) => "private",
            Some(TSAccessibility::Protected) => "protected",
            _ if name.starts_with('#') => "private",
            _ => "public",
        };
        let type_hint = self.type_text(p.type_annotation.as_deref());
        self.properties.push(Property {
            container: Some(class.to_owned()),
            name,
            line: self.line(p.span.start),
            type_hint,
            visibility: visibility.into(),
            is_static: p.r#static,
            readonly: p.readonly,
            doc: None,
            js: false,
        });
    }

    /// A TS constructor parameter property (`constructor(private repo: Repo)`): a láthatósággal vagy `readonly`-val
    /// jelölt paraméter egyben osztálymező.
    fn parameter_property(&mut self, class: &str, param: &FormalParameter<'_>) {
        if param.accessibility.is_none() && !param.readonly {
            return;
        }
        let BindingPattern::BindingIdentifier(id) = &param.pattern else {
            return;
        };
        let visibility = match param.accessibility {
            Some(TSAccessibility::Private) => "private",
            Some(TSAccessibility::Protected) => "protected",
            _ => "public",
        };
        let type_hint = self.type_text(param.type_annotation.as_deref());
        self.properties.push(Property {
            container: Some(class.to_owned()),
            name: id.name.to_string(),
            line: self.line(param.span.start),
            type_hint,
            visibility: visibility.into(),
            is_static: false,
            readonly: param.readonly,
            doc: None,
            js: false,
        });
    }

    /// A TS típus-annotáció szövege (a `:` nélkül, rövidítve).
    fn type_text(&self, annotation: Option<&TSTypeAnnotation<'_>>) -> Option<String> {
        annotation.map(|a| {
            let span = a.type_annotation.span();
            self.docs.src_of(span.start, span.end, 60)
        })
    }
}
