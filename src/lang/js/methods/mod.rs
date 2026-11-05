//! A `methods` mező passza: top-level függvények, hookok, Options API, osztálymetódusok, pszeudo-metódusok.

mod collector;
mod docs;
mod options;
mod probe;
mod top;

use oxc_ast::ast::{ExportDefaultDeclarationKind, Expression, Program, Statement};

use super::analysis::{JsAnalysis, JsPass, PassContext};
use super::sfc::SfcBlock;
use crate::model::Method;
use collector::{Collector, FnRef, Opts};

pub use docs::{Docs, container_doc};
pub(crate) use options::is_options_api;

/// Metódus-gyűjtő passz.
#[derive(Default)]
pub struct MethodsPass;

impl JsPass for MethodsPass {
    fn run(&self, ctx: &PassContext<'_>, program: &Program<'_>, out: &mut JsAnalysis) {
        let docs = Docs {
            source: ctx.source,
            comments: &program.comments,
        };
        let main = ctx
            .path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut c = Collector::new(docs, ctx.lines, main);
        for stmt in &program.body {
            collect_statement(&mut c, stmt);
        }
        pseudo_method(&mut c, ctx, program);
        c.methods.sort_by_key(|m| m.line);
        // az osztály-containereket a `ClassContainerPass` adja (itt csak a metódusaik container-neve kell)
        for method in std::mem::take(&mut c.methods) {
            out.builder.method(method);
        }
    }
}

/// A blokk metódus-indexe: a nevek és a függvény-offset -> (név, container) térkép.
pub struct MethodIndex {
    pub names: std::collections::HashSet<String>,
    pub fns: std::collections::HashMap<u32, (String, String)>,
    /// A fő container (a fájlnév).
    pub main: String,
}

/// Ugyanazzal a gyűjtéssel, mint a `MethodsPass`, de csak az indexet adja (a hívás-passznak).
pub fn method_index(ctx: &PassContext<'_>, program: &Program<'_>) -> MethodIndex {
    let docs = Docs {
        source: ctx.source,
        comments: &program.comments,
    };
    let main = ctx
        .path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut c = Collector::new(docs, ctx.lines, main.clone());
    for stmt in &program.body {
        collect_statement(&mut c, stmt);
    }
    MethodIndex {
        names: c.names,
        fns: c.fns,
        main,
    }
}

fn collect_statement(c: &mut Collector<'_>, stmt: &Statement<'_>) {
    match stmt {
        Statement::ExportDeclaration(e) => c.top_declaration(&e.declaration, true),
        Statement::ExportDefaultDeclaration(e) => export_default(c, e.span, &e.declaration),
        Statement::ImportDeclaration(_) => {}
        other => c.top_statement(other, false),
    }
}

fn export_default(
    c: &mut Collector<'_>,
    span: oxc_span::Span,
    d: &ExportDefaultDeclarationKind<'_>,
) {
    match d {
        ExportDefaultDeclarationKind::ObjectExpression(o) if options::is_options_api(o) => {
            c.options_api(o)
        }
        // Vue-kulcsok nélküli `export default { … }`: sima objektum-literál (a kulcsok metódusok)
        ExportDefaultDeclarationKind::ObjectExpression(o) => c.object_methods("", o, "public", 0),
        ExportDefaultDeclarationKind::CallExpression(call) => {
            if let Some(Expression::ObjectExpression(o)) =
                call.arguments.first().and_then(|a| a.as_expression())
            {
                c.options_api(o);
            }
        }
        ExportDefaultDeclarationKind::FunctionDeclaration(f) if f.body.is_some() => {
            let name = f.id.as_ref().map_or("default", |i| i.name.as_str());
            let name = c.add(name, FnRef::Func(f), span, Opts::new("public", "function"));
            c.nested(&name, FnRef::Func(f));
        }
        ExportDefaultDeclarationKind::FunctionExpression(f) => {
            let name = c.add(
                "default",
                FnRef::Func(f),
                span,
                Opts::new("public", "function"),
            );
            c.nested(&name, FnRef::Func(f));
        }
        ExportDefaultDeclarationKind::ArrowFunctionExpression(f) => {
            let name = c.add(
                "default",
                FnRef::Arrow(f),
                span,
                Opts::new("public", "function"),
            );
            c.nested(&name, FnRef::Arrow(f));
        }
        ExportDefaultDeclarationKind::ClassDeclaration(cl) => {
            c.class(cl, true);
        }
        _ => {}
    }
}

/// `setup` / `__module__`: a blokk sortartományával, ha van top-level hívás vagy elágazás.
fn pseudo_method(c: &mut Collector<'_>, ctx: &PassContext<'_>, program: &Program<'_>) {
    if !probe::has_top_level_activity(program, &c.registered) {
        return;
    }
    let first = c.line(0);
    let end = c.line(ctx.source.len() as u32);
    c.methods.push(pseudo(
        &c.main.clone(),
        if ctx.setup { "setup" } else { "__module__" },
        "top-level",
        first,
        end,
    ));
}

fn pseudo(container: &str, name: &str, kind: &str, line: u32, end: u32) -> Method {
    Method {
        container: Some(container.to_string()),
        name: name.to_string(),
        line,
        end_line: Some(end),
        visibility: Some("public".into()),
        is_static: false,
        is_abstract: false,
        params: Vec::new(),
        return_type: None,
        doc: None,
        body: None,
        kind: Some(kind.to_string()),
    }
}

/// A `template` pszeudo-metódus a `.vue` template blokkjának sortartományával.
pub fn template_method(container: &str, block: &SfcBlock) -> Method {
    pseudo(
        container,
        "template",
        "template",
        block.start_line,
        block.end_line,
    )
}

#[cfg(test)]
mod tests;
