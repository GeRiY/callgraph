//! A top-level kód aktivitása: van-e híváshoz / elágazáshoz tartozó `setup` / `__module__` caller
//! (a JS-motor ekkor vesz fel top-level pszeudo-metódust, ami a komment-caller szabályt érinti).

use oxc_ast::ast::*;
use oxc_ast_visit::{Visit, walk};

use super::{Scopes, scope_hooks};
use crate::lang::js::PassContext;

struct Probe {
    scopes: Scopes,
    active: bool,
}

impl Probe {
    fn mark(&mut self) {
        self.active |= self.scopes.is_top();
    }
}

impl<'a> Visit<'a> for Probe {
    scope_hooks!();

    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        self.mark();
        walk::walk_call_expression(self, it);
    }

    fn visit_new_expression(&mut self, it: &NewExpression<'a>) {
        self.mark();
        walk::walk_new_expression(self, it);
    }

    fn visit_import_expression(&mut self, it: &ImportExpression<'a>) {
        self.mark();
        walk::walk_import_expression(self, it);
    }

    fn visit_if_statement(&mut self, it: &IfStatement<'a>) {
        self.mark();
        walk::walk_if_statement(self, it);
    }

    fn visit_switch_statement(&mut self, it: &SwitchStatement<'a>) {
        self.mark();
        walk::walk_switch_statement(self, it);
    }
}

/// `true`, ha a blokk top-level kódjában van hívás vagy elágazás.
pub(crate) fn top_active(program: &Program<'_>, ctx: &PassContext<'_>) -> bool {
    let mut probe = Probe {
        scopes: Scopes::build(program, ctx),
        active: false,
    };
    probe.visit_program(program);
    probe.active
}
