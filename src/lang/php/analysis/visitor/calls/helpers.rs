use super::state::State;
use crate::lang::php::{
    analysis::{NameResolver, type_tracker::TypeTracker},
    parser::ParsedFile,
    printer::{Printer, lossy, strip_expr as strip},
};
use crate::model::Call;
use mago_span::HasSpan;
use mago_syntax::cst::Call as AstCall;
use mago_syntax::cst::*;
use std::collections::HashSet;

pub(super) struct Collector<'p, 'a> {
    pub(super) parsed: &'p ParsedFile<'a>,
}

impl Collector<'_, '_> {
    pub(super) fn push(&self, s: &mut State, n: &impl HasSpan, k: &str, t: String) -> usize {
        s.calls.push(Call {
            handler: None,
            from: None,
            container: s.container.clone(),
            caller: Some(s.caller.clone().unwrap_or_else(|| "__top__".into())),
            line: self.parsed.lines_of(n).0,
            end_line: None,
            kind: k.into(),
            target: t,
            resolved: None,
            on: None,
            category: String::new(),
            args: None,
        });
        s.calls.len() - 1
    }

    pub(super) fn args(&self, l: &ArgumentList<'_>) -> Option<Vec<String>> {
        if l.arguments.is_empty() {
            return None;
        }
        let mut p = Printer::new(self.parsed);
        Some(
            l.arguments
                .iter()
                .map(|a| {
                    p.indent = 0;
                    let e = match a {
                        Argument::Positional(x) => x.value,
                        Argument::Named(x) => x.value,
                    };
                    let s = p.expr(e);
                    if s.chars().count() > 60 {
                        format!("{}…", s.chars().take(60).collect::<String>())
                    } else {
                        s
                    }
                })
                .collect(),
        )
    }

    pub(super) fn class(&self, e: &Expression<'_>) -> Option<String> {
        match strip(e) {
            Expression::Identifier(i) => Some(lossy(i.value()).trim_start_matches('\\').into()),
            Expression::Self_(x) | Expression::Static(x) | Expression::Parent(x) => {
                Some(self.parsed.text(x.span()))
            }
            _ => None,
        }
    }

    fn receiver_type(&self, s: &State, e: &Expression<'_>) -> Option<String> {
        match strip(e) {
            Expression::Variable(Variable::Direct(v)) => s.tracker.type_of(&lossy(v.name)),
            Expression::Access(Access::Property(p)) => {
                let Expression::Variable(Variable::Direct(v)) = strip(p.object) else {
                    return None;
                };
                if v.name != b"$this" {
                    return None;
                }
                let ClassLikeMemberSelector::Identifier(i) = &p.property else {
                    return None;
                };
                let name = lossy(i.value);
                s.tracker
                    .type_of(&format!("this->{name}"))
                    .or_else(|| s.tracker.type_of(name.trim_start_matches('$')))
            }
            // `app(Foo::class)->bar()`, `resolve(Foo::class)->bar()`: a konténerből kért példány
            Expression::Call(_) => self.container_type(s, e),
            _ => None,
        }
    }

    /// A Laravel-konténerből kért példány típusa: `app(Foo::class)`, `resolve(Foo::class)`,
    /// `app()->make(Foo::class)`, `$this->app->make(Foo::class)`, `App::make(Foo::class)` (a homlokzat teljes
    /// nevével is). Csak az `X::class` első argumentum számít (a további argumentumok nem); a sztring (`app('foo')`)
    /// egy kötés neve, nem osztály — abból nincs típus. Az `X` az importok és a névtér szerint oldódik fel.
    pub(super) fn container_type(&self, s: &State, e: &Expression<'_>) -> Option<String> {
        let args = match strip(e) {
            Expression::Call(AstCall::Function(f)) => {
                let Expression::Identifier(i) = strip(f.function) else {
                    return None;
                };
                let name = lossy(i.value());
                let name = name.trim_start_matches('\\');
                if !(name.eq_ignore_ascii_case("app") || name.eq_ignore_ascii_case("resolve")) {
                    return None;
                }
                &f.argument_list
            }
            Expression::Call(AstCall::Method(m)) => {
                let ClassLikeMemberSelector::Identifier(name) = &m.method else {
                    return None;
                };
                if !name.value.eq_ignore_ascii_case(b"make") || !self.is_container(m.object) {
                    return None;
                }
                &m.argument_list
            }
            Expression::Call(AstCall::StaticMethod(c)) => {
                let ClassLikeMemberSelector::Identifier(name) = &c.method else {
                    return None;
                };
                if !name.value.eq_ignore_ascii_case(b"make") || !self.is_app_facade(s, c.class) {
                    return None;
                }
                &c.argument_list
            }
            _ => return None,
        };
        let value = match args.arguments.iter().next()? {
            Argument::Positional(x) if x.ellipsis.is_none() => x.value,
            Argument::Named(x) => x.value,
            Argument::Positional(_) => return None,
        };
        let Expression::Access(Access::ClassConstant(c)) = strip(value) else {
            return None;
        };
        let ClassLikeConstantSelector::Identifier(k) = &c.constant else {
            return None;
        };
        if !k.value.eq_ignore_ascii_case(b"class") {
            return None;
        }
        self.class(c.class).map(|x| s.resolver.resolve_class(&x))
    }

    /// A konténer maga: `app()` (argumentum nélkül) vagy `$this->app`.
    fn is_container(&self, e: &Expression<'_>) -> bool {
        match strip(e) {
            Expression::Access(Access::Property(p)) => {
                matches!(strip(p.object), Expression::Variable(Variable::Direct(v)) if v.name == b"$this")
                    && matches!(&p.property, ClassLikeMemberSelector::Identifier(i) if i.value == b"app")
            }
            Expression::Call(AstCall::Function(f)) => {
                f.argument_list.arguments.is_empty()
                    && matches!(strip(f.function), Expression::Identifier(i) if lossy(i.value()).trim_start_matches('\\').eq_ignore_ascii_case("app"))
            }
            _ => false,
        }
    }

    /// Az `App` homlokzat: `Illuminate\Support\Facades\App` (importtal vagy teljes névvel), vagy import nélkül a
    /// globális `App` álnév.
    fn is_app_facade(&self, s: &State, class: &Expression<'_>) -> bool {
        const FACADE: &str = "Illuminate\\Support\\Facades\\App";
        let Some(raw) = self.class(class) else {
            return false;
        };
        raw.eq_ignore_ascii_case(FACADE)
            || s.resolver.resolve_class(&raw).eq_ignore_ascii_case(FACADE)
            || (raw == "App" && !s.imports.contains("App"))
    }

    /// Az `ast_graph.php` `resolveTypedExpr()` tükre: csak `Class::m()`, `new Class` és az
    /// ezekből induló `->m()` láncok adnak típust (változó / property / nullsafe nem). Ezen felül a Laravel-konténer
    /// `app(Foo::class)` / `resolve(Foo::class)` / `…->make(Foo::class)` / `App::make(Foo::class)` alakja is
    /// (`container_type`; a referenciában nincs).
    pub(super) fn typed_expr(&self, s: &State, e: &Expression<'_>) -> Option<String> {
        if let Some(ty) = self.container_type(s, e) {
            return Some(ty);
        }
        match strip(e) {
            Expression::Instantiation(i) => {
                self.class(i.class).map(|x| s.resolver.resolve_class(&x))
            }
            Expression::Call(AstCall::StaticMethod(c)) => {
                self.class(c.class).map(|x| s.resolver.resolve_class(&x))
            }
            Expression::Call(AstCall::Method(c)) => self.typed_expr(s, c.object),
            _ => None,
        }
    }

    pub(super) fn set_receiver_type(&self, s: &State, e: &Expression<'_>) -> Option<String> {
        self.receiver_type(s, e)
    }

    pub(super) fn collect_properties(
        &self,
        members: &Sequence<'_, ClassLikeMember<'_>>,
        s: &mut State,
    ) {
        for m in members.iter() {
            let Property::Plain(p) = (match m {
                ClassLikeMember::Property(p) => p,
                _ => continue,
            }) else {
                continue;
            };
            let Some(h) = &p.hint else { continue };
            let raw = self
                .parsed
                .text(h.span())
                .trim()
                .trim_start_matches('?')
                .to_string();
            if raw.contains(['|', '&'])
                || matches!(
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
                continue;
            }
            let ty = s.resolver.resolve_class(&raw);
            for item in p.items.iter() {
                let v = match item {
                    PropertyItem::Abstract(i) => &i.variable,
                    PropertyItem::Concrete(i) => &i.variable,
                };
                let name = lossy(v.name);
                s.tracker.set(format!("this->{name}"), ty.clone());
                s.tracker.set(
                    format!("this->{}", name.trim_start_matches('$')),
                    ty.clone(),
                );
            }
        }
    }

    pub(super) fn source_resolver(&self) -> (NameResolver, HashSet<String>) {
        let mut r = NameResolver::default();
        let mut i = HashSet::new();
        for l in self.parsed.text(self.parsed.program.span()).lines() {
            let l = l.trim();
            let l = l.strip_prefix("<?php").map_or(l, str::trim_start);
            let l = l.trim_end_matches("?>").trim_end().trim_end_matches(';');
            if let Some(x) = l.strip_prefix("namespace ") {
                r = NameResolver::new(Some(x.trim_end_matches('{').trim().into()))
            } else if let Some(x) = l.strip_prefix("use ") {
                for x in x.split(',') {
                    let mut p = x.split_ascii_whitespace();
                    let f = p.next().unwrap_or("").trim_start_matches('\\');
                    if f.is_empty() {
                        continue;
                    }
                    let a = if p.next() == Some("as") {
                        p.next().unwrap_or("")
                    } else {
                        f.rsplit('\\').next().unwrap_or(f)
                    };
                    r.add_import(a, f);
                    i.insert(a.into());
                }
            }
        }
        (r, i)
    }
}
