//! A bejárás: hívások, `new`, dinamikus import; a kötések a deklarátoroknál töltődnek (forrásbeli sorrendben).

use oxc_ast::ast::{
    Argument, ArrowFunctionExpression, BindingPattern, CallExpression, Class, Expression, Function,
    ImportExpression, JSXElementName, JSXMemberExpression, JSXMemberExpressionObject,
    JSXOpeningElement, NewExpression, Program, VariableDeclarator,
};
use oxc_ast_visit::{Visit, walk};
use oxc_span::GetSpan;
use oxc_syntax::scope::ScopeFlags;

use super::callee::{Binding, Callee, Scope, is_native};
use super::key_name;
use super::receivers::{
    ClassFrame, Frame, TypeRef, arrow_frame, class_frame, function_frame, program_frame,
};
use crate::lang::js::analysis::PassContext;
use crate::lang::js::methods::{Docs, MethodIndex};
use crate::model::{Call, DYNAMIC_CALL, DYNAMIC_KIND};

pub struct Walker<'s> {
    ctx: &'s PassContext<'s>,
    docs: &'s Docs<'s>,
    index: &'s MethodIndex,
    scope: Scope,
    caller: String,
    container: String,
    /// A függvény-keretek névtípusai (a legbelső a végén; az alja a program).
    frames: Vec<Frame>,
    /// A körülvevő osztályok; `None`: egy nem-metódus `function`, amelyben a `this` nem az osztály.
    classes: Vec<Option<ClassFrame>>,
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
            frames: Vec::new(),
            classes: Vec::new(),
            calls: Vec::new(),
        }
    }

    pub fn run(&mut self, program: &Program<'_>) {
        self.frames.push(program_frame(&self.scope.types, program));
        self.visit_program(program);
        self.frames.pop();
    }

    /// A helyi név típusa: a legbelső keret dönt, amelyben a név szerepel.
    fn var_type(&self, name: &str) -> Option<&TypeRef> {
        self.frames
            .iter()
            .rev()
            .find_map(|f| f.get(name))
            .and_then(Option::as_ref)
    }

    /// A `this` osztálya (a nem-metódus `function`-ben nincs).
    fn this_class(&self) -> Option<&ClassFrame> {
        self.classes.last()?.as_ref()
    }

    /// `x.m()` / `this.x.m()` / `this.m()` a fogadó ismert típusával: (típus, metódusnév).
    fn receiver(&self, callee: &Expression<'_>) -> Option<(TypeRef, String)> {
        let Expression::StaticMemberExpression(m) = callee.without_parentheses() else {
            return None;
        };
        let method = m.property.name.to_string();
        let ty = match strip_ts(&m.object) {
            Expression::Identifier(i) => self.var_type(i.name.as_str())?.clone(),
            Expression::ThisExpression(_) => {
                let class = self.this_class()?;
                if !class.methods.contains(&method) {
                    return None;
                }
                TypeRef {
                    name: class.name.clone()?,
                    category: "internal_helper",
                    from: None,
                }
            }
            Expression::StaticMemberExpression(inner)
                if matches!(strip_ts(&inner.object), Expression::ThisExpression(_)) =>
            {
                self.this_class()?
                    .props
                    .get(inner.property.name.as_str())?
                    .clone()?
            }
            _ => return None,
        };
        Some((ty, method))
    }

    /// A hívott kifejezés feloldása; ismert fogadó-típusnál `Típus::metódus`.
    fn callee(&self, callee: &Expression<'_>) -> Callee {
        let mut r = self.scope.resolve(callee);
        if let Some((ty, method)) = self.receiver(callee) {
            r.resolved = Some(format!("{}::{method}", ty.name));
            r.category = ty.category;
            r.from = ty.from;
        }
        r
    }

    /// Egy JSX-elem neve mint komponens-használat; a kisbetűs (`<div>`), a névtér-es (`<a:b>`) és a `this` nem az.
    /// Importált névnél a feloldás ugyanaz, mint a függvényhívásnál (`<import>::<név>`), fájlon belüli
    /// komponensnél `internal_helper`; egyébként (globális, ismeretlen) feloldatlan `component_use`.
    fn jsx_callee(&self, name: &JSXElementName<'_>) -> Option<Callee> {
        let (root, rest) = match name {
            JSXElementName::IdentifierReference(i) => {
                let nm = i.name.as_str();
                if !nm.starts_with(|c: char| c.is_ascii_uppercase()) {
                    return None;
                }
                (nm.to_string(), Vec::new())
            }
            JSXElementName::MemberExpression(m) => jsx_member_chain(m)?,
            _ => return None,
        };
        let target = std::iter::once(root.as_str())
            .chain(rest.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(".");
        let (resolved, category, from) = if rest.is_empty() && self.scope.names.contains(&root) {
            (None, "internal_helper", None)
        } else if let Some(b) = self.scope.bindings.get(&root) {
            let resolved = if rest.is_empty() {
                b.resolved.clone()
            } else {
                format!("{}.{}", b.resolved, rest.join("."))
            };
            (Some(resolved), "component_use", b.from.clone())
        } else if self.scope.local_fns.contains(&root) || self.scope.names.contains(&root) {
            (None, "internal_helper", None)
        } else {
            (None, "component_use", None)
        };
        Some(Callee {
            on: (!rest.is_empty()).then(|| root.clone()),
            target,
            resolved,
            category,
            kind: "component",
            from,
        })
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
            from: r.from,
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

    /// Dinamikus hívás-jelölés: a cél a hívott kifejezés szövege (egy sorra vágva, ~60 karakterre rövidítve).
    fn record_dynamic(&mut self, start: u32, expr: &str) {
        self.calls.push(Call {
            handler: None,
            from: None,
            container: Some(self.container.clone()),
            caller: Some(self.caller.clone()),
            line: self.ctx.lines.line_of(start as usize),
            end_line: None,
            kind: DYNAMIC_KIND.to_string(),
            target: Call::dynamic_target(expr),
            resolved: None,
            on: None,
            category: DYNAMIC_CALL.to_string(),
            args: None,
        });
    }

    /// A kifejezés forrásszövege, egy sorba húzva (vágás nélkül).
    fn text(&self, span: oxc_span::Span) -> String {
        self.docs.src_of(span.start, span.end, usize::MAX)
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
                self.set(id.name.as_str(), rv, cat, r.from.clone());
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
                        self.set(&v, format!("{rv}.{k}"), cat, r.from.clone());
                    }
                }
            }
            _ => {}
        }
    }

    fn set(&mut self, name: &str, resolved: String, category: &'static str, from: Option<String>) {
        self.scope.bindings.insert(
            name.to_string(),
            Binding {
                resolved,
                category,
                from,
            },
        );
    }
}

/// Literál kulcs / argumentum: sztring, szám, helyettesítés nélküli sablon.
fn is_literal(e: &Expression<'_>) -> bool {
    let e = e.get_inner_expression();
    matches!(
        e,
        Expression::StringLiteral(_) | Expression::NumericLiteral(_)
    ) || e.is_no_substitution_template()
}

/// A `new` hívott kifejezése futásidőben dől el (`new (expr)()`, `new obj[k]()`); a név- és tag-elérés,
/// az osztály-/függvény-kifejezés és a `this` nem az.
fn is_dynamic_new_callee(callee: &Expression<'_>) -> bool {
    match callee.get_inner_expression() {
        Expression::Identifier(_)
        | Expression::StaticMemberExpression(_)
        | Expression::PrivateFieldExpression(_)
        | Expression::ThisExpression(_)
        | Expression::ClassExpression(_)
        | Expression::FunctionExpression(_) => false,
        Expression::ComputedMemberExpression(m) => !is_literal(&m.expression),
        _ => true,
    }
}

/// `<A.B.C>` -> (`A`, `["B", "C"]`); a `this.X` gyökerű név nem komponens-hivatkozás.
fn jsx_member_chain(m: &JSXMemberExpression<'_>) -> Option<(String, Vec<String>)> {
    let (root, mut parts) = match &m.object {
        JSXMemberExpressionObject::IdentifierReference(i) => (i.name.to_string(), Vec::new()),
        JSXMemberExpressionObject::MemberExpression(inner) => jsx_member_chain(inner)?,
        JSXMemberExpressionObject::ThisExpression(_) => return None,
    };
    parts.push(m.property.name.to_string());
    Some((root, parts))
}

/// Zárójel és TS non-null (`x!`) nélkül.
fn strip_ts<'b, 'a>(mut e: &'b Expression<'a>) -> &'b Expression<'a> {
    loop {
        e = e.without_parentheses();
        match e {
            Expression::TSNonNullExpression(n) => e = &n.expression,
            _ => return e,
        }
    }
}

impl<'a> Visit<'a> for Walker<'_> {
    fn visit_function(&mut self, it: &Function<'a>, flags: ScopeFlags) {
        let saved = self.enter(it.span.start);
        self.frames.push(function_frame(&self.scope.types, it));
        // a nem-metódus `function`-ben a `this` nem az osztály
        let unbinds = self
            .this_class()
            .is_some_and(|c| !c.method_fns.contains(&it.span.start));
        if unbinds {
            self.classes.push(None);
        }
        walk::walk_function(self, it, flags);
        if unbinds {
            self.classes.pop();
        }
        self.frames.pop();
        self.leave(saved);
    }

    fn visit_arrow_function_expression(&mut self, it: &ArrowFunctionExpression<'a>) {
        let saved = self.enter(it.span.start);
        self.frames.push(arrow_frame(&self.scope.types, it));
        walk::walk_arrow_function_expression(self, it);
        self.frames.pop();
        self.leave(saved);
    }

    fn visit_class(&mut self, it: &Class<'a>) {
        self.classes.push(Some(class_frame(&self.scope.types, it)));
        walk::walk_class(self, it);
        self.classes.pop();
    }

    fn visit_variable_declarator(&mut self, it: &VariableDeclarator<'a>) {
        self.bind_declarator(it);
        walk::walk_variable_declarator(self, it);
    }

    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        let callee = it.callee.without_parentheses();
        match callee {
            // `obj[action]()`: a kulcs futásidőben dől el (a literál kulcs, `obj['x']()`, marad a régi)
            Expression::ComputedMemberExpression(m) if !is_literal(&m.expression) => {
                let text = self.text(callee.span());
                self.record_dynamic(it.span.start, &text);
            }
            _ => {
                let r = self.callee(&it.callee);
                self.record(it.span.start, r, &it.arguments);
                // `require(expr)`: nem literál modul-név (a `require` hívás marad, mellé a jelölés)
                if matches!(callee, Expression::Identifier(i) if i.name.as_str() == "require")
                    && it
                        .arguments
                        .first()
                        .is_some_and(|a| !a.as_expression().is_some_and(is_literal))
                {
                    let text = self.text(it.span);
                    self.record_dynamic(it.span.start, &text);
                }
            }
        }
        walk::walk_call_expression(self, it);
    }

    fn visit_jsx_opening_element(&mut self, it: &JSXOpeningElement<'a>) {
        if let Some(r) = self.jsx_callee(&it.name) {
            self.record(it.span.start, r, &[]);
        }
        walk::walk_jsx_opening_element(self, it);
    }

    fn visit_new_expression(&mut self, it: &NewExpression<'a>) {
        if is_dynamic_new_callee(&it.callee) {
            // `new (expr)(...)`: az osztály futásidőben dől el
            let text = format!("new {}", self.text(it.callee.span()));
            self.record_dynamic(it.span.start, &text);
            walk::walk_new_expression(self, it);
            return;
        }
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
        // `import(expr)` nem literál forrással: dinamikus jelölés (a literál import marad a régi `<expr>` sor)
        if !is_literal(&it.source) {
            let text = self.text(it.span);
            self.record_dynamic(it.span.start, &text);
            walk::walk_import_expression(self, it);
            return;
        }
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
            from: None,
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
