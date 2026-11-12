//! A metódus-gyűjtő: névütközés-kezelés (`@L<sor>`), a regisztrált függvények és a felvett container-ek.

use std::collections::{HashMap, HashSet};

use oxc_ast::ast::{
    ArrowFunctionBody, ArrowFunctionExpression, Expression, FormalParameters, Function, Statement,
};
use oxc_span::{GetSpan, Span};

use super::docs::Docs;
use crate::lang::js::lines::LineIndex;
use crate::model::{Container, Method, Property};

/// Függvény-szerű csomópont (`function` / nyíl).
#[derive(Clone, Copy)]
pub enum FnRef<'a, 'b> {
    Func(&'b Function<'a>),
    Arrow(&'b ArrowFunctionExpression<'a>),
}

impl<'a, 'b> FnRef<'a, 'b> {
    /// A kifejezés, ha függvény vagy nyíl-függvény.
    pub fn of_expr(e: &'b Expression<'a>) -> Option<Self> {
        match e {
            Expression::FunctionExpression(f) => Some(Self::Func(f)),
            Expression::ArrowFunctionExpression(f) => Some(Self::Arrow(f)),
            _ => None,
        }
    }

    pub fn span(&self) -> Span {
        match self {
            Self::Func(f) => f.span,
            Self::Arrow(f) => f.span,
        }
    }

    fn params(&self) -> &'b FormalParameters<'a> {
        match self {
            Self::Func(f) => &f.params,
            Self::Arrow(f) => &f.params,
        }
    }

    /// A TS visszatérési típus (a `:` utáni rész) helye, ha van.
    fn return_span(&self) -> Option<Span> {
        match self {
            Self::Func(f) => f.return_type.as_ref(),
            Self::Arrow(f) => f.return_type.as_ref(),
        }
        .map(|r| r.type_annotation.span())
    }

    /// A blokk-törzs utasításai (nyíl kifejezés-törzsnél és törzs nélkülinél `None`).
    pub fn block(&self) -> Option<&'b [Statement<'a>]> {
        match self {
            Self::Func(f) => f.body.as_ref().map(|b| &b.statements[..]),
            Self::Arrow(f) => match &f.body {
                ArrowFunctionBody::FunctionBody(b) => Some(&b.statements[..]),
                _ => None,
            },
        }
    }
}

/// Egy felvett metódus megjelenési jellemzői.
pub struct Opts<'o> {
    pub container: Option<&'o str>,
    pub visibility: &'o str,
    pub is_static: bool,
    pub kind: &'o str,
}

impl<'o> Opts<'o> {
    pub fn new(visibility: &'o str, kind: &'o str) -> Self {
        Self {
            container: None,
            visibility,
            is_static: false,
            kind,
        }
    }
}

/// A script-blokk metódusainak és container-einek gyűjtője.
pub struct Collector<'s> {
    pub docs: Docs<'s>,
    pub lines: &'s LineIndex,
    /// A fő container (a fájlnév).
    pub main: String,
    /// A felvett metódusnevek (a `methodNames`).
    pub names: HashSet<String>,
    /// Függvény kezdő offset -> (metódusnév, container): a hívások `caller`-éhez.
    pub fns: HashMap<u32, (String, String)>,
    /// A metódusként felvett függvények kezdő offsetjei (a hívó-döntéshez).
    pub registered: HashSet<u32>,
    pub methods: Vec<Method>,
    pub containers: Vec<Container>,
    /// Az osztálymezők (nem függvény-értékű class field, TS constructor parameter property).
    pub properties: Vec<Property>,
}

impl<'s> Collector<'s> {
    pub fn new(docs: Docs<'s>, lines: &'s LineIndex, main: String) -> Self {
        Self {
            docs,
            lines,
            main,
            names: HashSet::new(),
            fns: HashMap::new(),
            registered: HashSet::new(),
            methods: Vec::new(),
            containers: Vec::new(),
            properties: Vec::new(),
        }
    }

    pub fn line(&self, offset: u32) -> u32 {
        self.lines.line_of(offset as usize)
    }

    /// A paraméterek forrása (egyenként ≤60 kar.), a rest-tel együtt.
    fn param_srcs(&self, p: &FormalParameters<'_>) -> Vec<String> {
        let items = p.items.iter().map(|i| i.span);
        let rest = p.rest.iter().map(|r| r.span);
        items
            .chain(rest)
            .map(|s| self.docs.src_of(s.start, s.end, 60))
            .collect()
    }

    /// Metódus felvétele; a `stmt` adja a sorokat és a docblockot. Visszaadja a (végleges) nevet.
    pub fn add(&mut self, name: &str, f: FnRef<'_, '_>, stmt: Span, o: Opts<'_>) -> String {
        let mut n = name.to_string();
        if self.names.contains(&n) {
            n = format!("{name}@L{}", self.line(stmt.start));
        }
        self.names.insert(n.clone());
        self.registered.insert(f.span().start);
        let container = o
            .container
            .map_or_else(|| self.main.clone(), str::to_string);
        self.fns.insert(f.span().start, (n.clone(), container));
        let doc = self
            .docs
            .doc_of(stmt.start)
            .or_else(|| self.docs.doc_of(f.span().start));
        self.methods.push(Method {
            container: Some(
                o.container
                    .map_or_else(|| self.main.clone(), str::to_string),
            ),
            name: n.clone(),
            line: self.line(stmt.start),
            end_line: Some(self.line(stmt.end)),
            visibility: Some(o.visibility.to_string()),
            is_static: o.is_static,
            is_abstract: false,
            params: self.param_srcs(f.params()),
            return_type: f
                .return_span()
                .map(|s| self.docs.src_of(s.start, s.end, 60)),
            doc_types: None,
            doc,
            body: None,
            kind: Some(o.kind.to_string()),
        });
        n
    }

    /// A függvény törzsének közvetlen belső függvényei (`külső.belső`, `inner`; csak egy szint).
    pub fn nested(&mut self, outer: &str, f: FnRef<'_, '_>) {
        let Some(stmts) = f.block() else { return };
        for s in stmts {
            match s {
                Statement::FunctionDeclaration(func) if func.body.is_some() => {
                    if let Some(id) = &func.id {
                        let name = format!("{outer}.{}", id.name);
                        self.add(
                            &name,
                            FnRef::Func(func),
                            func.span,
                            Opts::new("private", "inner"),
                        );
                    }
                }
                Statement::VariableDeclaration(v) => {
                    for d in &v.declarations {
                        let (Some(id), Some(f)) = (
                            d.id.get_binding_identifier(),
                            d.init.as_ref().and_then(FnRef::of_expr),
                        ) else {
                            continue;
                        };
                        let name = format!("{outer}.{}", id.name);
                        self.add(&name, f, v.span(), Opts::new("private", "inner"));
                    }
                }
                _ => {}
            }
        }
    }
}
