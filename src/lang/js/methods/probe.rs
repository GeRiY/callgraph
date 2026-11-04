//! Van-e top-level (metódusba nem ágyazott) hívás vagy elágazás a blokkban? — a `setup` / `__module__` feltétele.

use std::collections::HashSet;

use oxc_ast::ast::{
    ArrowFunctionExpression, CallExpression, Function, IfStatement, ImportExpression,
    NewExpression, Program, SwitchStatement,
};
use oxc_ast_visit::{Visit, walk};
use oxc_syntax::scope::ScopeFlags;

struct Probe<'r> {
    registered: &'r HashSet<u32>,
    depth: u32,
    found: bool,
}

impl<'a> Visit<'a> for Probe<'_> {
    fn visit_function(&mut self, it: &Function<'a>, flags: ScopeFlags) {
        let own = self.registered.contains(&it.span.start);
        self.depth += u32::from(own);
        walk::walk_function(self, it, flags);
        self.depth -= u32::from(own);
    }

    fn visit_arrow_function_expression(&mut self, it: &ArrowFunctionExpression<'a>) {
        let own = self.registered.contains(&it.span.start);
        self.depth += u32::from(own);
        walk::walk_arrow_function_expression(self, it);
        self.depth -= u32::from(own);
    }

    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        self.found |= self.depth == 0;
        walk::walk_call_expression(self, it);
    }

    fn visit_new_expression(&mut self, it: &NewExpression<'a>) {
        self.found |= self.depth == 0;
        walk::walk_new_expression(self, it);
    }

    fn visit_import_expression(&mut self, it: &ImportExpression<'a>) {
        self.found |= self.depth == 0;
        walk::walk_import_expression(self, it);
    }

    fn visit_if_statement(&mut self, it: &IfStatement<'a>) {
        self.found |= self.depth == 0;
        walk::walk_if_statement(self, it);
    }

    fn visit_switch_statement(&mut self, it: &SwitchStatement<'a>) {
        self.found |= self.depth == 0;
        walk::walk_switch_statement(self, it);
    }
}

/// `true`, ha a programban van a `registered` függvényeken kívüli hívás / `if` / `switch`.
pub fn has_top_level_activity(program: &Program<'_>, registered: &HashSet<u32>) -> bool {
    let mut probe = Probe {
        registered,
        depth: 0,
        found: false,
    };
    probe.visit_program(program);
    probe.found
}
