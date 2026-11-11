//! Dinamikus (futásidőben eldőlő) hívások jelölése: `$obj->$m()`, `$class::$m()`, `$fn()`, `new $class`,
//! `call_user_func($x)`, `app($var)`, `$this->app->make($var)`. A jelölés `dynamic` típusú hívás (kategóriája
//! `dynamic_call`), célja a hívott kifejezés szövege; hívó-él nem keletkezik belőle.

use super::{helpers::Collector, state::State};
use crate::lang::php::analysis::type_tracker::TypeTracker;
use crate::lang::php::printer::lossy;
use crate::model::{Call, DYNAMIC_KIND};
use mago_span::HasSpan;
use mago_syntax::cst::Call as AstCall;
use mago_syntax::cst::*;

/// Sztring-literál (`'x'`, `"x"`; az interpolált `"a$b"` nem az).
fn is_string_literal(e: &Expression<'_>) -> bool {
    matches!(super::strip(e), Expression::Literal(Literal::String(_)))
}

/// `X::class` (a konstans-azonosító `class`).
fn is_class_constant(e: &Expression<'_>) -> bool {
    matches!(
        super::strip(e),
        Expression::Access(Access::ClassConstant(c))
            if matches!(&c.constant, ClassLikeConstantSelector::Identifier(k) if k.value == b"class")
    )
}

/// A tömb utolsó eleme sztring-literál (`[$this, 'method']`, `[Foo::class, 'bar']`): a metódus statikusan ismert.
fn array_ends_with_string(e: &Expression<'_>) -> bool {
    let last = match super::strip(e) {
        Expression::Array(a) => a.elements.iter().last(),
        Expression::LegacyArray(a) => a.elements.iter().last(),
        _ => None,
    };
    match last {
        Some(ArrayElement::Value(v)) => is_string_literal(v.value),
        Some(ArrayElement::KeyValue(v)) => is_string_literal(v.value),
        _ => false,
    }
}

/// Az első argumentum és hogy szét van-e szórva (`...$args`).
fn first_arg<'x, 'y>(l: &'x ArgumentList<'y>) -> Option<(&'x Expression<'y>, bool)> {
    l.arguments.iter().next().map(|a| match a {
        Argument::Positional(x) => (x.value, x.ellipsis.is_some()),
        Argument::Named(x) => (x.value, false),
    })
}

impl Collector<'_, '_> {
    /// A dinamikus hívás-jelölés felvétele: a cél a hívott kifejezés szövege (egy sorra vágva, rövidítve).
    pub(super) fn push_dynamic(&self, s: &mut State, n: &impl HasSpan, expr: &str) {
        self.push(s, n, DYNAMIC_KIND, Call::dynamic_target(expr));
    }

    /// Két node közös forrásszövege (az elsőtől a másodikig): `$this->$joinMethod`.
    pub(super) fn span_text(&self, from: &impl HasSpan, to: &impl HasSpan) -> String {
        self.parsed.text(from.span().join(to.span()))
    }

    /// Függvényhívás: nem azonosító hívott kifejezés (`$fn(...)`, `$a['k'](...)`, `foo()(...)`) — a closure-
    /// literál közvetlen hívása (`(function () {})()`) nem dinamikus.
    pub(super) fn dynamic_callee(&self, s: &mut State, n: &FunctionCall<'_>) {
        match super::strip(n.function) {
            Expression::Identifier(_) | Expression::Closure(_) | Expression::ArrowFunction(_) => {}
            // `$next($r)`, ahol a `$next` Closure / callable paraméter vagy helyi closure: callback, nem dinamikus
            Expression::Variable(Variable::Direct(v)) if s.callables.contains(&lossy(v.name)) => {}
            // `($this->prop)(…)`: osztály-típusú property → statikusan ismert `__invoke` hívás
            Expression::Access(Access::Property(p)) if self.invokable_property(s, n, p) => {}
            other => {
                let text = self.parsed.text(other.span());
                self.push_dynamic(s, n, &text);
            }
        }
    }

    /// `($this->prop)(…)` osztály- / interfész-típusú property-n: felveszi a `Típus::__invoke` method_call-t és
    /// `true`-t ad. `Closure` / `callable` / típus nélküli property-re `false` (marad dinamikus).
    fn invokable_property(
        &self,
        s: &mut State,
        n: &FunctionCall<'_>,
        p: &PropertyAccess<'_>,
    ) -> bool {
        let (Expression::Variable(Variable::Direct(v)), ClassLikeMemberSelector::Identifier(i)) =
            (super::strip(p.object), &p.property)
        else {
            return false;
        };
        if v.name != b"$this" {
            return false;
        }
        let name = lossy(i.value);
        let Some(ty) = s
            .tracker
            .type_of(&format!("this->{name}"))
            .or_else(|| s.tracker.type_of(&format!("this->${name}")))
        else {
            return false;
        };
        if ty
            .rsplit('\\')
            .next()
            .is_some_and(|t| t.eq_ignore_ascii_case("closure"))
        {
            return false;
        }
        let x = self.push(s, n, "method_call", "__invoke".into());
        s.calls[x].on = Some(format!("this->{name}"));
        s.calls[x].resolved = Some(format!("{ty}::__invoke"));
        s.calls[x].args = self.args(&n.argument_list);
        true
    }

    /// A paraméterlista `Closure` / `callable` típusú paramétereit (`?Closure`, `Closure|callable`, `\Closure`) felveszi
    /// a hívható változók közé.
    pub(super) fn register_callable_params(
        &self,
        s: &mut State,
        list: &FunctionLikeParameterList<'_>,
    ) {
        for p in list.parameters.iter() {
            let Some(h) = &p.hint else { continue };
            let text = self.parsed.text(h.span());
            let callable = text
                .split(['|', '&', '?', '(', ')'])
                .map(|t| t.trim().trim_start_matches('\\').to_ascii_lowercase())
                .any(|t| t == "closure" || t == "callable");
            if callable {
                s.callables.insert(lossy(p.variable.name));
            }
        }
    }

    /// `$var = function (…) {…}` / `fn (…) => …` / `static fn …`: helyi closure.
    pub(super) fn register_local_closure(&self, s: &mut State, n: &Assignment<'_>) {
        if !n.operator.is_assign() {
            return;
        }
        if let Expression::Variable(Variable::Direct(v)) = super::strip(n.lhs)
            && matches!(
                super::strip(n.rhs),
                Expression::Closure(_) | Expression::ArrowFunction(_)
            )
        {
            s.callables.insert(lossy(v.name));
        }
    }

    /// Azonosítóval hívott függvény dinamikus változatai: `call_user_func($x)`, `call_user_func_array($x, …)`,
    /// `app($var)`, `resolve($var)` (nem literál / `X::class` argumentummal).
    pub(super) fn dynamic_named_function(&self, s: &mut State, n: &FunctionCall<'_>, name: &str) {
        let Some((arg, spread)) = first_arg(&n.argument_list) else {
            return;
        };
        let dynamic = match name.to_ascii_lowercase().as_str() {
            "call_user_func" | "call_user_func_array" => {
                spread || !(is_string_literal(arg) || array_ends_with_string(arg))
            }
            "app" | "resolve" => spread || !(is_string_literal(arg) || is_class_constant(arg)),
            _ => false,
        };
        if dynamic {
            let text = format!("{name}({})", self.parsed.text(arg.span()));
            self.push_dynamic(s, n, &text);
        }
    }

    /// `$this->app->make($var)` / `app()->make($var)`: a konténer-feloldás nem literál argumentummal.
    pub(super) fn dynamic_container_make(
        &self,
        s: &mut State,
        n: &impl HasSpan,
        object: &Expression<'_>,
        method: &[u8],
        args: &ArgumentList<'_>,
    ) {
        if !method.eq_ignore_ascii_case(b"make") {
            return;
        }
        let container = match super::strip(object) {
            Expression::Access(Access::Property(p)) => {
                matches!(super::strip(p.object), Expression::Variable(Variable::Direct(v)) if v.name == b"$this")
                    && matches!(&p.property, ClassLikeMemberSelector::Identifier(i) if i.value == b"app")
            }
            Expression::Call(AstCall::Function(f)) => {
                matches!(super::strip(f.function), Expression::Identifier(i) if lossy(i.value()).trim_start_matches('\\').eq_ignore_ascii_case("app"))
            }
            _ => false,
        };
        if !container {
            return;
        }
        let Some((arg, spread)) = first_arg(args) else {
            return;
        };
        if spread || !(is_string_literal(arg) || is_class_constant(arg)) {
            let text = format!(
                "{}->make({})",
                self.parsed.text(object.span()),
                self.parsed.text(arg.span())
            );
            self.push_dynamic(s, n, &text);
        }
    }

    /// `new $class(...)` / `new $obj->prop(...)` / `new ($expr)(...)`: változó osztálynév.
    pub(super) fn dynamic_new(&self, s: &mut State, n: &Instantiation<'_>) {
        let class = super::strip(n.class);
        if matches!(class, Expression::AnonymousClass(_)) {
            return;
        }
        let text = self.parsed.text(class.span());
        self.push_dynamic(s, n, &format!("new {text}"));
    }
}
