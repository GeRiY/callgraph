//! A bejárás: hívások, `new`, dinamikus import; a kötések a deklarátoroknál töltődnek (forrásbeli sorrendben).

use oxc_ast::ast::{
    Argument, ArrowFunctionExpression, BindingPattern, CallExpression, Expression, Function,
    ImportExpression, NewExpression, Program, VariableDeclarator,
};
use oxc_ast_visit::{Visit, walk};
use oxc_span::GetSpan;
use oxc_syntax::scope::ScopeFlags;

use super::callee::{Binding, Callee, Scope, is_native};
use super::key_name;
use crate::lang::js::analysis::PassContext;
use crate::lang::js::methods::{Docs, MethodIndex};
use crate::model::Call;

pub struct Walker<'s> {
    ctx: &'s PassContext<'s>,
    docs: &'s Docs<'s>,
    index: &'s MethodIndex,
    scope: Scope,
    caller: String,
    container: String,
    pub calls: Vec<Call>,
}

impl<'s> Walker<'s> {
    pub fn new(
        ctx: &'s PassContext<'s>,
        docs: &'s Docs<'s>,
        index: &'s MethodIndex,
        scope: Scope,
        top: &str,
    ) -> Self {
        Self {
            ctx,
            docs,
            index,
            scope,
            caller: top.to_string(),
            container: index.main.clone(),
            calls: Vec::new(),
        }
    }

    pub fn run(&mut self, program: &Program<'_>) {
        self.visit_program(program);
    }

    fn src(&self, a: &Argument<'_>) -> String {
        let s = a
            .as_expression()
            .map_or_else(|| a.span(), |e| e.without_parentheses().span());
        self.docs.src_of(s.start, s.end, 60)
    }

    fn record(&mut self, start: u32, r: Callee, args: &[Argument<'_>]) {
        let mut resolved = r.resolved.clone();
        if let Some(Argument::StringLiteral(s)) = args.first()
            && !s.value.is_empty()
        {
            let prefix = match r.category {
                "vue_emit" => Some("vue::emit:"),
                "i18n_helper" => Some("i18n::"),
                "route_helper" => Some("route::"),
                _ => None,
            };
            if let Some(p) = prefix {
                resolved = Some(format!("{p}{}", s.value.display()));
            }
        }
        let args = (!args.is_empty()).then(|| args.iter().take(4).map(|a| self.src(a)).collect());
        self.calls.push(Call {
            handler: None,
            container: Some(self.container.clone()),
            caller: Some(self.caller.clone()),
            line: self.ctx.lines.line_of(start as usize),
            end_line: None,
            kind: r.kind.to_string(),
            target: r.target,
            resolved,
            on: r.on,
            category: r.category.to_string(),
            args,
        });
    }

    /// Egy metódusként felvett függvény belépése: a `caller` / container átáll.
    fn enter(&mut self, start: u32) -> Option<(String, String)> {
        let (name, cont) = self.index.fns.get(&start)?.clone();
        Some((
            std::mem::replace(&mut self.caller, name),
            std::mem::replace(&mut self.container, cont),
        ))
    }

    fn leave(&mut self, saved: Option<(String, String)>) {
        if let Some((caller, container)) = saved {
            self.caller = caller;
            self.container = container;
        }
    }

    /// `const x = hívás()` / `await hívás()` / destrukturálás -> kötés.
    fn bind_declarator(&mut self, d: &VariableDeclarator<'_>) {
        let Some(mut init) = d.init.as_ref().map(Expression::without_parentheses) else {
            return;
        };
        if let Expression::AwaitExpression(a) = init {
            init = a.argument.without_parentheses();
        }
        let Expression::CallExpression(c) = init else {
            return;
        };
        let r = self.scope.resolve(&c.callee);
        if r.resolved.is_none() && r.category == "external_call" {
            return;
        }
        let rv = format!("{}()", r.resolved.as_deref().unwrap_or(&r.target));
        let cat = match r.target.as_str() {
            "defineEmits" => "vue_emit",
            "defineProps" | "withDefaults" => "vue_props",
            _ => r.category,
        };
        match &d.id {
            BindingPattern::BindingIdentifier(id) => {
                self.set(id.name.as_str(), rv, cat);
            }
            BindingPattern::ObjectPattern(o) => {
                for p in &o.properties {
                    let k = key_name(&p.key);
                    let v = match &p.value {
                        BindingPattern::BindingIdentifier(id) => Some(id.name.to_string()),
                        _ => k.clone(),
                    };
                    if let (Some(k), Some(v)) = (k, v) {
                        let cat = if k == "t" { "i18n_helper" } else { r.category };
                        self.set(&v, format!("{rv}.{k}"), cat);
                    }
                }
            }
            _ => {}
        }
    }

    fn set(&mut self, name: &str, resolved: String, category: &'static str) {
        self.scope
            .bindings
            .insert(name.to_string(), Binding { resolved, category });
    }
}

impl<'a> Visit<'a> for Walker<'_> {
    fn visit_function(&mut self, it: &Function<'a>, flags: ScopeFlags) {
        let saved = self.enter(it.span.start);
        walk::walk_function(self, it, flags);
        self.leave(saved);
    }

    fn visit_arrow_function_expression(&mut self, it: &ArrowFunctionExpression<'a>) {
        let saved = self.enter(it.span.start);
        walk::walk_arrow_function_expression(self, it);
        self.leave(saved);
    }

    fn visit_variable_declarator(&mut self, it: &VariableDeclarator<'a>) {
        self.bind_declarator(it);
        walk::walk_variable_declarator(self, it);
    }

    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        let r = self.scope.resolve(&it.callee);
        self.record(it.span.start, r, &it.arguments);
        walk::walk_call_expression(self, it);
    }

    fn visit_new_expression(&mut self, it: &NewExpression<'a>) {
        let mut r = self.scope.resolve(&it.callee);
        r.kind = "new";
        if r.resolved.is_none() {
            r.category = if is_native(&r.target) {
                "native_js"
            } else {
                "instantiation"
            };
        }
        self.record(it.span.start, r, &it.arguments);
        walk::walk_new_expression(self, it);
    }

    fn visit_import_expression(&mut self, it: &ImportExpression<'a>) {
        let mut args = Vec::new();
        let span = it.source.without_parentheses().span();
        args.push(self.docs.src_of(span.start, span.end, 60));
        for o in it.options.iter() {
            let s = o.without_parentheses().span();
            args.push(self.docs.src_of(s.start, s.end, 60));
        }
        let r = Callee::expr();
        let (container, caller) = (self.container.clone(), self.caller.clone());
        self.calls.push(Call {
            handler: None,
            container: Some(container),
            caller: Some(caller),
            line: self.ctx.lines.line_of(it.span.start as usize),
            end_line: None,
            kind: r.kind.to_string(),
            target: r.target,
            resolved: None,
            on: None,
            category: r.category.to_string(),
            args: Some(args),
        });
        walk::walk_import_expression(self, it);
    }
}
