//! Előszkennelés (`scanFns`): a fájlon belül definiált függvénynevek és az `{emit}` paraméter kötése.

use oxc_ast::ast::{
    ArrowFunctionExpression, BindingPattern, Expression, FormalParameters, Function, FunctionType,
    VariableDeclarator,
};
use oxc_ast_visit::{Visit, walk};
use oxc_syntax::scope::ScopeFlags;

use super::callee::{Binding, Scope};
use super::key_name;

struct Scan<'s> {
    scope: &'s mut Scope,
}

impl Scan<'_> {
    /// `function f({ emit }) {}` -> az `emit` helyi név `vue_emit` kötést kap.
    fn params(&mut self, params: &FormalParameters<'_>) {
        for p in &params.items {
            if p.initializer.is_some() {
                continue;
            }
            let BindingPattern::ObjectPattern(o) = &p.pattern else {
                continue;
            };
            for prop in &o.properties {
                if key_name(&prop.key).as_deref() != Some("emit") {
                    continue;
                }
                if let BindingPattern::BindingIdentifier(id) = &prop.value {
                    self.scope.bindings.insert(
                        id.name.to_string(),
                        Binding {
                            resolved: "vue::emit".into(),
                            category: "vue_emit",
                        },
                    );
                }
            }
        }
    }
}

impl<'a> Visit<'a> for Scan<'_> {
    fn visit_function(&mut self, it: &Function<'a>, flags: ScopeFlags) {
        if it.r#type == FunctionType::FunctionDeclaration
            && let Some(id) = &it.id
        {
            self.scope.local_fns.insert(id.name.to_string());
        }
        self.params(&it.params);
        walk::walk_function(self, it, flags);
    }

    fn visit_arrow_function_expression(&mut self, it: &ArrowFunctionExpression<'a>) {
        self.params(&it.params);
        walk::walk_arrow_function_expression(self, it);
    }

    fn visit_variable_declarator(&mut self, it: &VariableDeclarator<'a>) {
        if let (BindingPattern::BindingIdentifier(id), Some(init)) = (&it.id, &it.init)
            && matches!(
                init.without_parentheses(),
                Expression::FunctionExpression(_) | Expression::ArrowFunctionExpression(_)
            )
        {
            self.scope.local_fns.insert(id.name.to_string());
        }
        walk::walk_variable_declarator(self, it);
    }
}

/// Feltölti a `local_fns`-t és az `{emit}` kötéseket.
pub fn scan_functions(program: &oxc_ast::ast::Program<'_>, scope: &mut Scope) {
    Scan { scope }.visit_program(program);
}
