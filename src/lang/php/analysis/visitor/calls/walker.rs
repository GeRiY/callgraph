use super::{helpers::Collector, state::State};
use mago_span::HasSpan;
use mago_syntax::cst::*;
use mago_syntax::walker::Walker;

impl<'a, 'arena> Walker<'a, 'arena, State> for Collector<'_, '_> {
    fn walk_in_class(&self, n: &Class<'arena>, s: &mut State) {
        s.container = Some(super::lossy(n.name.value));
        self.collect_properties(&n.members, s);
        s.resolver.enter_container(
            s.container.as_deref().unwrap_or(""),
            n.extends
                .as_ref()
                .map(|x| {
                    x.types
                        .iter()
                        .map(|t| s.resolver.resolve_class(&self.parsed.text(t.span())))
                        .collect()
                })
                .unwrap_or_default(),
        )
    }
    fn walk_out_class(&self, _: &Class<'arena>, s: &mut State) {
        s.container = None;
        s.resolver.leave_container()
    }
    fn walk_in_interface(&self, n: &Interface<'arena>, s: &mut State) {
        s.container = Some(super::lossy(n.name.value));
        self.collect_properties(&n.members, s);
        s.resolver
            .enter_container(s.container.as_deref().unwrap_or(""), vec![])
    }
    fn walk_out_interface(&self, _: &Interface<'arena>, s: &mut State) {
        s.container = None;
        s.resolver.leave_container()
    }
    fn walk_in_trait(&self, n: &Trait<'arena>, s: &mut State) {
        s.container = Some(super::lossy(n.name.value));
        self.collect_properties(&n.members, s);
        s.resolver
            .enter_container(s.container.as_deref().unwrap_or(""), vec![])
    }
    fn walk_out_trait(&self, _: &Trait<'arena>, s: &mut State) {
        s.container = None;
        s.resolver.leave_container()
    }
    fn walk_in_enum(&self, n: &Enum<'arena>, s: &mut State) {
        s.container = Some(super::lossy(n.name.value));
        self.collect_properties(&n.members, s);
        s.resolver
            .enter_container(s.container.as_deref().unwrap_or(""), vec![])
    }
    fn walk_out_enum(&self, _: &Enum<'arena>, s: &mut State) {
        s.container = None;
        s.resolver.leave_container()
    }
    fn walk_in_anonymous_class(&self, n: &AnonymousClass<'arena>, s: &mut State) {
        s.anon.enter(n, &mut s.container, &s.caller);
        // a referencia itt a property-típusokat a névtelen osztályéra cseréli, és kilépéskor nem állítja vissza
        s.tracker.clear_properties();
        self.collect_properties(&n.members, s);
    }
    fn walk_out_anonymous_class(&self, _: &AnonymousClass<'arena>, s: &mut State) {
        s.anon.leave(&mut s.container);
        // a referencia a névtelen osztály után nem ismer aktuális osztályt: a `self` innen közönséges névként oldódik fel
        s.resolver.leave_container();
    }
    fn walk_in_partial_argument_list(&self, n: &PartialArgumentList<'arena>, s: &mut State) {
        s.anon
            .args_in(n.span().start.offset, &mut s.container, &mut s.caller);
    }
    fn walk_out_partial_argument_list(&self, n: &PartialArgumentList<'arena>, s: &mut State) {
        s.anon
            .args_out(n.span().start.offset, &mut s.container, &mut s.caller);
    }
    fn walk_in_method(&self, n: &Method<'arena>, s: &mut State) {
        let mut name = super::lossy(n.name.value);
        if s.closure_depth > 0 {
            name.push_str(&format!(".<closure>{}", s.closure_depth));
        }
        s.caller = Some(name);
        s.tracker.clear_locals();
        s.callables.clear();
        self.register_callable_params(s, &n.parameter_list);
        s.tracker
            .set("__caller", s.caller.clone().unwrap_or_default());
        for p in n.parameter_list.parameters.iter() {
            if let Some(h) = &p.hint {
                let raw_text = self.parsed.text(h.span());
                let raw = raw_text.trim().trim_start_matches('?');
                if !raw.contains(['|', '&'])
                    && !matches!(
                        raw.to_ascii_lowercase().as_str(),
                        "array"
                            | "callable"
                            | "bool"
                            | "float"
                            | "int"
                            | "iterable"
                            | "mixed"
                            | "null"
                            | "object"
                            | "string"
                            | "void"
                    )
                {
                    let ty = s.resolver.resolve_class(raw);
                    let name = super::lossy(p.variable.name);
                    s.tracker.set(name.clone(), ty.clone());
                    if p.is_promoted_property() {
                        s.tracker.set(format!("this->{name}"), ty.clone());
                        s.tracker
                            .set(format!("this->{}", name.trim_start_matches('$')), ty);
                    }
                }
            }
        }
    }
    fn walk_in_assignment(&self, n: &Assignment<'arena>, s: &mut State) {
        self.register_local_closure(s, n);
        if !n.operator.is_assign() {
            return;
        }
        let Some(ty) = self.typed_expr(s, n.rhs) else {
            return;
        };
        if let Expression::Variable(Variable::Direct(v)) = super::strip(n.lhs) {
            s.tracker.set(super::lossy(v.name), ty);
        }
    }
    fn walk_out_method(&self, _: &Method<'arena>, s: &mut State) {
        s.caller = None
    }
    fn walk_in_function(&self, n: &Function<'arena>, s: &mut State) {
        s.callables.clear();
        self.register_callable_params(s, &n.parameter_list);
    }
    fn walk_in_closure(&self, n: &Closure<'arena>, s: &mut State) {
        self.register_callable_params(s, &n.parameter_list);
        s.closure_depth += 1;
    }
    fn walk_out_closure(&self, _: &Closure<'arena>, s: &mut State) {
        s.closure_depth = s.closure_depth.saturating_sub(1);
    }
    fn walk_in_arrow_function(&self, n: &ArrowFunction<'arena>, s: &mut State) {
        self.register_callable_params(s, &n.parameter_list);
        s.closure_depth += 1;
    }
    fn walk_out_arrow_function(&self, _: &ArrowFunction<'arena>, s: &mut State) {
        s.closure_depth = s.closure_depth.saturating_sub(1);
    }
    fn walk_in_function_call(&self, n: &FunctionCall<'arena>, s: &mut State) {
        if let Expression::Identifier(i) = super::strip(n.function) {
            let name: String = super::lossy(i.value()).trim_start_matches('\\').into();
            let x = self.push(s, n, "func_call", name.clone());
            s.calls[x].args = self.args(&n.argument_list);
            // `call_user_func($x)`, `app($var)`… — a hívás marad, mellé a dinamikus jelölés
            self.dynamic_named_function(s, n, &name);
        } else {
            // `$fn(...)`, `$handlers['k'](...)`: változó / kifejezés a hívott
            self.dynamic_callee(s, n);
        }
    }
    fn walk_in_method_call(&self, n: &MethodCall<'arena>, s: &mut State) {
        if let ClassLikeMemberSelector::Identifier(m) = &n.method {
            let x = self.push(s, n, "method_call", super::lossy(m.value));
            s.calls[x].on = Some(match super::strip(n.object) {
                Expression::Variable(Variable::Direct(v)) if v.name == b"$this" => "this".into(),
                Expression::Variable(Variable::Direct(v)) => super::lossy(v.name),
                Expression::Access(Access::Property(p)) => {
                    match (super::strip(p.object), &p.property) {
                        (
                            Expression::Variable(Variable::Direct(v)),
                            ClassLikeMemberSelector::Identifier(i),
                        ) if v.name == b"$this" => format!("this->{}", super::lossy(i.value)),
                        _ => "<expr>".into(),
                    }
                }
                _ => "<expr>".into(),
            });
            if let Some(ty) = self.set_receiver_type(s, n.object) {
                s.calls[x].resolved = Some(format!("{ty}::{}", super::lossy(m.value)));
            }
            s.calls[x].args = self.args(&n.argument_list);
            self.dynamic_container_make(s, n, n.object, m.value, &n.argument_list);
        } else {
            // `$obj->$m()`, `$obj->{$expr}()`: a metódusnév futásidőben dől el
            let text = self.span_text(n.object, &n.method);
            self.push_dynamic(s, n, &text);
        }
    }
    /// Első osztályú callable (`$this->m(...)`): a referencia sima `method_call`-ként rögzíti, argumentumok nélkül.
    fn walk_in_method_partial_application(
        &self,
        n: &MethodPartialApplication<'arena>,
        s: &mut State,
    ) {
        if let ClassLikeMemberSelector::Identifier(m) = &n.method {
            let x = self.push(s, n, "method_call", super::lossy(m.value));
            s.calls[x].on = Some(match super::strip(n.object) {
                Expression::Variable(Variable::Direct(v)) if v.name == b"$this" => "this".into(),
                Expression::Variable(Variable::Direct(v)) => super::lossy(v.name),
                _ => "<expr>".into(),
            });
            if let Some(ty) = self.set_receiver_type(s, n.object) {
                s.calls[x].resolved = Some(format!("{ty}::{}", super::lossy(m.value)));
            }
        } else {
            let text = self.span_text(n.object, &n.method);
            self.push_dynamic(s, n, &text);
        }
    }
    fn walk_in_null_safe_method_call(&self, n: &NullSafeMethodCall<'arena>, s: &mut State) {
        if let ClassLikeMemberSelector::Identifier(m) = &n.method {
            let x = self.push(s, n, "nullsafe_method_call", super::lossy(m.value));
            s.calls[x].on = Some(match super::strip(n.object) {
                Expression::Variable(Variable::Direct(v)) if v.name == b"$this" => "this".into(),
                Expression::Variable(Variable::Direct(v)) => super::lossy(v.name),
                Expression::Access(Access::Property(p)) => {
                    match (super::strip(p.object), &p.property) {
                        (
                            Expression::Variable(Variable::Direct(v)),
                            ClassLikeMemberSelector::Identifier(i),
                        ) if v.name == b"$this" => format!("this->{}", super::lossy(i.value)),
                        _ => "<expr>".into(),
                    }
                }
                _ => "<expr>".into(),
            });
            if let Some(ty) = self.set_receiver_type(s, n.object) {
                s.calls[x].resolved = Some(format!("{ty}::{}", super::lossy(m.value)));
            }
            s.calls[x].args = self.args(&n.argument_list)
        } else {
            let text = self.span_text(n.object, &n.method);
            self.push_dynamic(s, n, &text);
        }
    }
    fn walk_in_static_method_call(&self, n: &StaticMethodCall<'arena>, s: &mut State) {
        // `Foo::$m()`, `$class::{$expr}()`: a metódusnév futásidőben dől el — nem `Foo::<dynamic>` hívás-él
        let ClassLikeMemberSelector::Identifier(name) = &n.method else {
            let text = self.span_text(n.class, &n.method);
            self.push_dynamic(s, n, &text);
            return;
        };
        if let Some(c) = self.class(n.class) {
            let m = super::lossy(name.value);
            let x = self.push(s, n, "static_call", format!("{c}::{m}"));
            s.calls[x].resolved = Some(format!("{}::{m}", s.resolver.resolve_class(&c)));
            s.calls[x].args = self.args(&n.argument_list)
        }
    }
    fn walk_in_instantiation(&self, n: &Instantiation<'arena>, s: &mut State) {
        if let Some(c) = self.class(n.class) {
            let x = self.push(s, n, "new", c.clone());
            s.calls[x].resolved = Some(s.resolver.resolve_class(&c))
        } else {
            // `new $class(...)`, `new $obj->prop(...)`: az osztály futásidőben dől el
            self.dynamic_new(s, n);
        }
    }
    fn walk_in_class_constant_access(&self, n: &ClassConstantAccess<'arena>, s: &mut State) {
        if let (Some(c), ClassLikeConstantSelector::Identifier(k)) =
            (self.class(n.class), &n.constant)
            && k.value != b"class"
        {
            let x = self.push(
                s,
                n,
                "const_fetch",
                format!("{c}::{}", super::lossy(k.value)),
            );
            s.calls[x].resolved = Some(format!(
                "{}::{}",
                s.resolver.resolve_class(&c),
                super::lossy(k.value)
            ))
        }
    }
    fn walk_in_static_property_access(&self, n: &StaticPropertyAccess<'arena>, s: &mut State) {
        let (Some(c), Variable::Direct(v)) = (self.class(n.class), &n.property) else {
            return;
        };
        let name = super::lossy(v.name);
        let target = format!("{c}::{name}");
        let x = self.push(s, n, "static_property_fetch", target.clone());
        s.calls[x].resolved = Some(format!("{}::{name}", s.resolver.resolve_class(&c)));
        s.calls[x].on = Some(target);
    }
}
