//! A "melyik metódusban vagyunk" szabály (a JS-motor `methodOf`/`caller` logikája): a metódus-index,
//! a bejárás közbeni caller-verem és a top-level kód aktivitás-vizsgálata.

mod methods;
mod probe;
pub(crate) mod text;

use std::collections::HashMap;

use oxc_ast::ast::Program;
use oxc_span::Span;

use super::analysis::PassContext;
pub(crate) use probe::top_active;

/// Egy metódus a sortartományával (a `statement` sorai, mint a JS-motorban).
#[derive(Debug, Clone)]
pub(crate) struct MethodInfo {
    pub name: String,
    pub container: String,
    pub line: u32,
    pub end_line: u32,
}

/// A metódus-index és a bejárás alatti aktuális caller-verem.
pub(crate) struct Scopes {
    methods: Vec<MethodInfo>,
    keys: HashMap<(u32, u32), usize>,
    stack: Vec<usize>,
    top: &'static str,
    base: String,
}

impl Scopes {
    pub fn build(program: &Program<'_>, ctx: &PassContext<'_>) -> Self {
        let base = ctx
            .path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut collector = methods::Collector::new(ctx.lines, &base);
        collector.run(program);
        let mut keys = HashMap::new();
        let mut infos = Vec::new();
        for c in collector.out {
            keys.insert(c.key, infos.len());
            infos.push(c.info);
        }
        Self {
            methods: infos,
            keys,
            stack: Vec::new(),
            top: if ctx.setup { "setup" } else { "__module__" },
            base,
        }
    }

    /// A felvett metódusok (a top-level pszeudo-metódus nélkül).
    pub fn methods(&self) -> &[MethodInfo] {
        &self.methods
    }

    /// A top-level kód pszeudo-metódusának neve (`setup` / `__module__`).
    pub fn top_name(&self) -> &'static str {
        self.top
    }

    /// A fájl (alap) container-neve.
    pub fn base_container(&self) -> &str {
        &self.base
    }

    /// Belépés egy csomópontba; `true`, ha az metódus volt (a hívó `leave`-et hív).
    pub fn enter(&mut self, span: Span) -> bool {
        match self.keys.get(&(span.start, span.end)) {
            Some(&i) => {
                self.stack.push(i);
                true
            }
            None => false,
        }
    }

    pub fn leave(&mut self) {
        self.stack.pop();
    }

    pub fn is_top(&self) -> bool {
        self.stack.is_empty()
    }

    pub fn caller(&self) -> &str {
        self.stack
            .last()
            .map_or(self.top, |&i| &self.methods[i].name)
    }

    pub fn container(&self) -> &str {
        self.stack
            .last()
            .map_or(&self.base, |&i| &self.methods[i].container)
    }
}

/// A függvény-/metódus-csomópontokba lépést kezelő `Visit` metódusok (a visitor `scopes` mezőjével).
macro_rules! scope_hooks {
    () => {
        fn visit_function(
            &mut self,
            it: &oxc_ast::ast::Function<'a>,
            flags: oxc_syntax::scope::ScopeFlags,
        ) {
            let e = self.scopes.enter(it.span);
            oxc_ast_visit::walk::walk_function(self, it, flags);
            if e {
                self.scopes.leave();
            }
        }

        fn visit_arrow_function_expression(
            &mut self,
            it: &oxc_ast::ast::ArrowFunctionExpression<'a>,
        ) {
            let e = self.scopes.enter(it.span);
            oxc_ast_visit::walk::walk_arrow_function_expression(self, it);
            if e {
                self.scopes.leave();
            }
        }

        fn visit_method_definition(&mut self, it: &oxc_ast::ast::MethodDefinition<'a>) {
            let e = self.scopes.enter(it.span);
            oxc_ast_visit::walk::walk_method_definition(self, it);
            if e {
                self.scopes.leave();
            }
        }
    };
}
pub(crate) use scope_hooks;
