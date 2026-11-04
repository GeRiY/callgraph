//! A metódus-index: ugyanaz a név- és sortartomány-szabály, mint a JS-motor `addMethod`-jánál.

use std::collections::HashSet;

use oxc_ast::ast::*;
use oxc_span::Span;

use super::MethodInfo;
use crate::lang::js::LineIndex;

/// Egy felvett metódus és a függvény-csomópont kulcsa (a bejárás ezzel azonosítja).
pub(super) struct Collected {
    pub info: MethodInfo,
    pub key: (u32, u32),
}

#[derive(Clone, Copy)]
enum FnRef<'b, 'a> {
    Func(&'b Function<'a>),
    Arrow(&'b ArrowFunctionExpression<'a>),
}

impl<'b, 'a> FnRef<'b, 'a> {
    fn span(self) -> Span {
        match self {
            Self::Func(f) => f.span,
            Self::Arrow(a) => a.span,
        }
    }

    /// A blokk-törzs utasításai (a kifejezés-törzsű nyíl-függvénynek nincs).
    fn block(self) -> Option<&'b [Statement<'a>]> {
        match self {
            Self::Func(f) => f.body.as_ref().map(|b| &b.statements[..]),
            Self::Arrow(a) => match &a.body {
                ArrowFunctionBody::FunctionBody(b) => Some(&b.statements[..]),
                _ => None,
            },
        }
    }
}

fn unparen<'b, 'a>(mut e: &'b Expression<'a>) -> &'b Expression<'a> {
    while let Expression::ParenthesizedExpression(p) = e {
        e = &p.expression;
    }
    e
}

fn fn_of<'b, 'a>(e: &'b Expression<'a>) -> Option<FnRef<'b, 'a>> {
    match unparen(e) {
        Expression::FunctionExpression(f) => Some(FnRef::Func(f)),
        Expression::ArrowFunctionExpression(a) => Some(FnRef::Arrow(a)),
        _ => None,
    }
}

fn key_name(k: &PropertyKey) -> Option<String> {
    match k {
        PropertyKey::StaticIdentifier(i) => Some(i.name.to_string()),
        PropertyKey::Identifier(i) => Some(i.name.to_string()),
        PropertyKey::StringLiteral(s) => Some(s.value.display().to_string()),
        _ => None,
    }
}

const FN_WRAPPERS: [&str; 6] = [
    "computed",
    "useMemo",
    "debounce",
    "throttle",
    "useDebounceFn",
    "useThrottleFn",
];

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

pub(super) struct Collector<'l> {
    lines: &'l LineIndex,
    container: String,
    names: HashSet<String>,
    pub out: Vec<Collected>,
}

impl<'l> Collector<'l> {
    pub fn new(lines: &'l LineIndex, container: &str) -> Self {
        Self {
            lines,
            container: container.to_string(),
            names: HashSet::new(),
            out: Vec::new(),
        }
    }

    pub fn run(&mut self, program: &Program) {
        for stmt in &program.body {
            match stmt {
                Statement::ExportDeclaration(e) => self.declaration(&e.declaration),
                Statement::ExportDefaultDeclaration(e) => self.export_default(e),
                Statement::ImportDeclaration(_) => {}
                other => self.top_stmt(other),
            }
        }
    }

    fn add(&mut self, name: &str, key: Span, stmt: Span, container: Option<&str>) -> String {
        let line = self.lines.line_of(stmt.start as usize);
        let name = if self.names.contains(name) {
            format!("{name}@L{line}")
        } else {
            name.to_string()
        };
        self.names.insert(name.clone());
        self.out.push(Collected {
            info: MethodInfo {
                name: name.clone(),
                container: container.unwrap_or(&self.container).to_string(),
                line,
                end_line: self.lines.line_of(stmt.end as usize),
            },
            key: (key.start, key.end),
        });
        name
    }

    fn nested(&mut self, outer: &str, f: FnRef) {
        let Some(body) = f.block() else { return };
        for s in body {
            match s {
                Statement::FunctionDeclaration(d) if d.id.is_some() && d.body.is_some() => {
                    let n = format!("{outer}.{}", d.id.as_ref().unwrap().name);
                    self.add(&n, d.span, d.span, None);
                }
                Statement::VariableDeclaration(v) => {
                    for d in &v.declarations {
                        let (Some(id), Some(init)) = (d.id.get_binding_identifier(), &d.init)
                        else {
                            continue;
                        };
                        if let Some(f) = fn_of(init) {
                            self.add(&format!("{outer}.{}", id.name), f.span(), v.span, None);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn export_default(&mut self, e: &ExportDefaultDeclaration) {
        match &e.declaration {
            ExportDefaultDeclarationKind::FunctionDeclaration(f) if f.body.is_some() => {
                let n = f.id.as_ref().map_or("default", |i| i.name.as_str());
                let n = self.add(n, f.span, e.span, None);
                self.nested(&n, FnRef::Func(f));
            }
            ExportDefaultDeclarationKind::ClassDeclaration(c) => self.class(c),
            other => {
                let Some(expr) = other.as_expression() else {
                    return;
                };
                match unparen(expr) {
                    Expression::ObjectExpression(o) => self.options_api(o),
                    Expression::CallExpression(c) => {
                        if let Some(Expression::ObjectExpression(o)) =
                            c.arguments.first().and_then(|a| a.as_expression())
                        {
                            self.options_api(o);
                        }
                    }
                    other => {
                        if let Some(f) = fn_of(other) {
                            let n = self.add("default", f.span(), e.span, None);
                            self.nested(&n, f);
                        }
                    }
                }
            }
        }
    }

    fn declaration(&mut self, d: &Declaration) {
        match d {
            Declaration::FunctionDeclaration(f) => self.function_decl(f),
            Declaration::VariableDeclaration(v) => self.var_decl(v),
            Declaration::ClassDeclaration(c) => self.class(c),
            _ => {}
        }
    }

    fn top_stmt(&mut self, s: &Statement) {
        match s {
            Statement::FunctionDeclaration(f) => self.function_decl(f),
            Statement::VariableDeclaration(v) => self.var_decl(v),
            Statement::ClassDeclaration(c) => self.class(c),
            Statement::ExpressionStatement(e) => self.hook(e),
            _ => {}
        }
    }

    fn function_decl(&mut self, f: &Function) {
        if let (Some(id), Some(_)) = (&f.id, &f.body) {
            let n = self.add(id.name.as_str(), f.span, f.span, None);
            self.nested(&n, FnRef::Func(f));
        }
    }

    fn var_decl(&mut self, v: &VariableDeclaration) {
        for d in &v.declarations {
            let (Some(id), Some(init)) = (d.id.get_binding_identifier(), &d.init) else {
                continue;
            };
            let at = if v.declarations.len() == 1 {
                v.span
            } else {
                d.span
            };
            if let Some(f) = fn_of(init) {
                let n = self.add(id.name.as_str(), f.span(), at, None);
                self.nested(&n, f);
            } else if let Expression::CallExpression(c) = unparen(init)
                && let Expression::Identifier(callee) = &c.callee
                && FN_WRAPPERS.contains(&callee.name.as_str())
                && let Some(f) = c
                    .arguments
                    .first()
                    .and_then(|a| a.as_expression())
                    .and_then(fn_of)
            {
                self.add(id.name.as_str(), f.span(), at, None);
            }
        }
    }

    fn hook(&mut self, s: &ExpressionStatement) {
        let Expression::CallExpression(c) = unparen(&s.expression) else {
            return;
        };
        let Expression::Identifier(callee) = &c.callee else {
            return;
        };
        if !is_hook(callee.name.as_str()) {
            return;
        }
        let f = c
            .arguments
            .iter()
            .rev()
            .filter_map(|a| a.as_expression())
            .find_map(fn_of);
        if let Some(f) = f {
            let line = self.lines.line_of(s.span.start as usize);
            self.add(&format!("{}@L{line}", callee.name), f.span(), s.span, None);
        }
    }

    fn class(&mut self, c: &Class) {
        let Some(id) = &c.id else { return };
        for el in &c.body.body {
            match el {
                ClassElement::MethodDefinition(m) if m.value.body.is_some() => {
                    if let Some(k) = key_name(&m.key) {
                        self.add(&k, m.span, m.span, Some(id.name.as_str()));
                    }
                }
                ClassElement::PropertyDefinition(p) => {
                    let f = p.value.as_ref().and_then(fn_of);
                    if let (Some(k), Some(f)) = (key_name(&p.key), f) {
                        self.add(&k, f.span(), p.span, Some(id.name.as_str()));
                    }
                }
                _ => {}
            }
        }
    }

    fn options_api(&mut self, obj: &ObjectExpression) {
        for p in &obj.properties {
            let ObjectPropertyKind::ObjectProperty(p) = p else {
                continue;
            };
            let Some(k) = key_name(&p.key) else { continue };
            if k == "props" || k == "emits" {
                continue;
            }
            if let Some(f) = fn_of(&p.value) {
                let n = self.add(&k, f.span(), p.span, None);
                if k == "setup" {
                    self.nested(&n, f);
                }
            } else if matches!(k.as_str(), "methods" | "computed" | "watch")
                && let Expression::ObjectExpression(o) = unparen(&p.value)
            {
                for q in &o.properties {
                    let ObjectPropertyKind::ObjectProperty(q) = q else {
                        continue;
                    };
                    let Some(qk) = key_name(&q.key) else { continue };
                    let f = fn_of(&q.value).or_else(|| match unparen(&q.value) {
                        Expression::ObjectExpression(inner) => {
                            inner.properties.iter().find_map(|r| match r {
                                ObjectPropertyKind::ObjectProperty(r) => fn_of(&r.value),
                                _ => None,
                            })
                        }
                        _ => None,
                    });
                    if let Some(f) = f {
                        let name = if k == "methods" {
                            qk
                        } else {
                            format!("{k}.{qk}")
                        };
                        self.add(&name, f.span(), q.span, None);
                    }
                }
            }
        }
    }
}
