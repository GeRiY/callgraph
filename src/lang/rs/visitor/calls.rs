//! `calls`: út-hívások (`Foo::new()`, `f()`), metódushívások (`x.bar()`), makróhívások (`println!`) és
//! konstans-hivatkozások (`MAX`, `Self::LIMIT`, `config::PORT` → `const_fetch`), a makró-argumentumokban
//! lévőkkel együtt; a kategóriát a `Categorizer` adja.
//!
//! Metódushívásnál a fogadó típusa, ha olcsón következtethető (`receivers`), a hívás `Típus::metódus`-ra
//! oldódik; ha nem, és a hívás más szabállyal sem oldható fel, a `resolved` `?::metódus` (feloldatlan
//! fogadó — a `Típus::metódus` hívó-keresés név szerint, bizonytalanként találja meg).

use std::collections::HashSet;

use syn::spanned::Spanned;
use syn::visit::{self, Visit};
use syn::{Expr, ExprCall, ExprMethodCall, ExprPath, Macro, Member};

use super::body::{BodyVisit, Location, walk_bodies};
use super::macros::visit_body;
use super::receivers::{segments, type_path};
use super::scopes::ConstNode;
use super::{FileVisitor, Scopes};
use crate::index::UNKNOWN_RECEIVER;
use crate::lang::rs::categorize::{CallInput, Callee, Categorizer, Verdict, is_std_method};
use crate::lang::rs::context::RsContext;
use crate::lang::rs::parser::ParsedFile;
use crate::lang::rs::resolve::{Resolution, Resolver, qualify_container};
use crate::model::{Call, FileGraphBuilder};

/// A `calls` mezőt tölti (a kategorizáló a konstruktoron át jön).
pub(crate) struct CallsVisitor {
    categorizer: Categorizer,
}

impl CallsVisitor {
    pub(crate) fn new(categorizer: Categorizer) -> Self {
        Self { categorizer }
    }

    pub(crate) fn with_defaults() -> Self {
        Self::new(Categorizer::with_defaults())
    }
}

impl FileVisitor for CallsVisitor {
    fn visit(
        &self,
        parsed: &ParsedFile<'_>,
        scopes: &Scopes<'_>,
        ctx: &mut RsContext,
        builder: &mut FileGraphBuilder,
    ) {
        let mut collector = Collector {
            parsed,
            scopes,
            resolver: &ctx.resolver,
            categorizer: &self.categorizer,
            location: None,
            calls: Vec::new(),
        };
        walk_bodies(scopes, &mut collector);
        for call in collector.calls {
            builder.call(call);
        }
    }
}

struct Collector<'p, 'a> {
    parsed: &'p ParsedFile<'a>,
    scopes: &'p Scopes<'p>,
    resolver: &'p Resolver,
    categorizer: &'p Categorizer,
    location: Option<Location>,
    calls: Vec<Call>,
}

/// Konstans-szerű név: `SCREAMING_CASE`, legalább két karakter, nagybetűvel kezdődik.
fn is_const_name(name: &str) -> bool {
    name.chars().count() >= 2
        && name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        && name
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

impl Collector<'_, '_> {
    fn verdict(&self, location: &Location, callee: &Callee) -> Verdict {
        let bound_locally = matches!(callee, Callee::Path(segs)
            if segs.len() == 1 && location.bindings.contains(&segs[0]));
        self.categorizer.categorize(&CallInput {
            callee,
            container: &location.container,
            inline: &location.inline,
            bound_locally,
            resolver: self.resolver,
        })
    }

    fn push(
        &mut self,
        kind: &str,
        target: String,
        verdict: Verdict,
        line: u32,
        on: Option<String>,
    ) {
        let Some(location) = &self.location else {
            return;
        };
        self.calls.push(Call {
            container: Some(location.container.clone()),
            caller: Some(location.caller.clone()),
            line,
            end_line: None,
            kind: kind.to_owned(),
            target,
            resolved: verdict.resolved,
            on,
            category: verdict.category.to_owned(),
            args: None,
            handler: None,
            from: None,
        });
    }

    fn record(&mut self, callee: Callee, line: u32, on: Option<String>) {
        let Some(location) = &self.location else {
            return;
        };
        let verdict = self.verdict(location, &callee);
        self.push(callee.kind(), callee.target(), verdict, line, on);
    }

    /// `x.név()`: a saját `self`-metódus, a következtetett fogadó-típus, a többi szabály, végül `?::név`.
    fn record_method(&mut self, node: &ExprMethodCall) {
        let Some(location) = &self.location else {
            return;
        };
        let name = node.method.to_string();
        let on_self = matches!(&*node.receiver, Expr::Path(p) if p.path.is_ident("self"));
        let callee = Callee::Method {
            name: name.clone(),
            on_self,
        };
        let base = self.verdict(location, &callee);
        let verdict = if on_self && self.resolver.has_method(&location.container, &name) {
            base
        } else if let Some(typed) = self
            .receiver_type(location, &node.receiver)
            .and_then(|ty| self.typed_verdict(location, ty, &name))
        {
            typed
        } else if base.category == "external_call" && base.resolved.is_none() {
            Verdict {
                category: base.category,
                resolved: Some(format!("{UNKNOWN_RECEIVER}{name}")),
            }
        } else {
            base
        };
        let on = self.parsed.short(&*node.receiver, 40);
        let line = self.parsed.line(node.method.span());
        self.push(callee.kind(), name, verdict, line, Some(on));
    }

    /// A metódushívás `Típus::név` útként kategorizálva. Gyakori std-metódusnévnél (`len`, `clone`…)
    /// csak akkor, ha a fájl saját típusának tényleg van ilyen metódusa (különben marad `native_rust`).
    fn typed_verdict(&self, location: &Location, ty: Vec<String>, name: &str) -> Option<Verdict> {
        if is_std_method(name) {
            // ismert, de nem saját típus (`names: Vec<String>` → `names.len()`): std-metódus, nem a fájl
            // azonos nevű metódusa (a név szerinti szabály különben a saját `Cache::len`-re kötné)
            let Some(own) = self.local_type(location, &ty) else {
                return Some(Verdict {
                    category: "native_rust",
                    resolved: None,
                });
            };
            if !self.resolver.has_method(&own, name) {
                return None;
            }
        }
        let mut path = ty;
        path.push(name.to_owned());
        let verdict = self.verdict(location, &Callee::Path(path.clone()));
        let resolved = verdict.resolved.unwrap_or_else(|| path.join("::"));
        Some(Verdict {
            category: verdict.category,
            resolved: Some(resolved),
        })
    }

    /// A fájlban definiált típus container-neve (`Self` → a hívó containere).
    fn local_type(&self, location: &Location, ty: &[String]) -> Option<String> {
        match self
            .resolver
            .resolve(ty, &location.container, &location.inline)
        {
            Resolution::Local(key) => Some(key),
            _ => None,
        }
    }

    /// A fogadó típusa: helyi név (`x`, `self`), saját struct mezője (`self.repo`, `x.inner`), zárójel / `&`.
    fn receiver_type(&self, location: &Location, expr: &Expr) -> Option<Vec<String>> {
        match expr {
            Expr::Path(p) if p.qself.is_none() => {
                let ident = p.path.get_ident()?;
                location.types.get(&ident.to_string()).cloned()
            }
            Expr::Paren(p) => self.receiver_type(location, &p.expr),
            Expr::Reference(r) => self.receiver_type(location, &r.expr),
            Expr::Field(f) => {
                let Member::Named(field) = &f.member else {
                    return None;
                };
                let owner = self.receiver_type(location, &f.base)?;
                let owner = self.local_type(location, &owner)?;
                let found = self.scopes.fields.iter().find(|x| {
                    x.container == owner && x.field.ident.as_ref().is_some_and(|i| i == field)
                })?;
                type_path(&found.field.ty, &HashSet::new())
            }
            _ => None,
        }
    }

    /// Konstans-hivatkozás (`const_fetch`): a fájl saját `const` / `static`-ja `<container>::NÉV`,
    /// különben az út-hívás szabályai szerint (`Self::X`, `config::PORT`, `u32::MAX`).
    fn record_const(&mut self, segs: Vec<String>, line: u32) {
        let Some(location) = &self.location else {
            return;
        };
        let target = segs.join("::");
        let own = match segs.as_slice() {
            [name] if !location.bindings.contains(name) => self.own_const(location, name),
            _ => None,
        };
        let verdict = match own {
            Some(container) => Verdict {
                category: "internal_helper",
                resolved: Some(format!("{container}::{target}")),
            },
            None => {
                let mut verdict = self.verdict(location, &Callee::Path(segs));
                if verdict.category == "external_call" {
                    verdict.category = "module_use";
                }
                verdict
            }
        };
        self.push("const_fetch", target, verdict, line, None);
    }

    /// Az egyszegmensű konstans containere: előbb a hívás helyének modulja, utána bármely modul-szintű.
    fn own_const(&self, location: &Location, name: &str) -> Option<String> {
        let here = qualify_container(self.resolver.module(), &location.inline);
        let candidates: Vec<&str> = self
            .scopes
            .consts
            .iter()
            .filter(|c| match c.node {
                ConstNode::Const(item) => item.ident == name,
                ConstNode::Static(item) => item.ident == name,
                _ => false,
            })
            .map(|c| c.container.as_str())
            .collect();
        candidates
            .iter()
            .find(|c| **c == here)
            .or_else(|| candidates.first())
            .map(|c| (*c).to_owned())
    }
}

impl BodyVisit for Collector<'_, '_> {
    fn enter(&mut self, location: Location) {
        self.location = Some(location);
    }
}

impl<'ast> Visit<'ast> for Collector<'_, '_> {
    fn visit_expr_call(&mut self, node: &'ast ExprCall) {
        if let Expr::Path(p) = &*node.func {
            let line = self.parsed.line(p.span());
            self.record(Callee::Path(segments(&p.path)), line, None);
            // a hívott út maga nem konstans-hivatkozás: csak az argumentumok
            for arg in &node.args {
                self.visit_expr(arg);
            }
            return;
        }
        visit::visit_expr_call(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &'ast ExprMethodCall) {
        self.record_method(node);
        visit::visit_expr_method_call(self, node);
    }

    fn visit_expr_path(&mut self, node: &'ast ExprPath) {
        if node.qself.is_none()
            && node
                .path
                .segments
                .last()
                .is_some_and(|s| is_const_name(&s.ident.to_string()))
        {
            let line = self.parsed.line(node.span());
            self.record_const(segments(&node.path), line);
        }
        visit::visit_expr_path(self, node);
    }

    fn visit_macro(&mut self, node: &'ast Macro) {
        let line = self.parsed.line(node.path.span());
        self.record(Callee::Macro(segments(&node.path)), line, None);
        visit_body(self, node);
    }
}

#[cfg(test)]
mod tests {
    use super::is_const_name;

    #[test]
    fn const_names() {
        assert!(is_const_name("MAX"));
        assert!(is_const_name("BUF_SIZE_2"));
        assert!(!is_const_name("X"));
        assert!(!is_const_name("Max"));
        assert!(!is_const_name("max"));
        assert!(!is_const_name("_X"));
    }
}
