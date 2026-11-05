//! `trait` és `impl` blokkok: a metódusok a céltípus containeréhez kerülnek
//! (`impl Trait for X` → X `implements`-e; ha X nem ebben a fájlban van, egy `impl` container).

use syn::spanned::Spanned;
use syn::{ImplItem, ItemImpl, ItemTrait, TraitItem, Type};

use super::walk::{FnParts, Walker};
use super::{ConstNode, FnRole};
use crate::lang::rs::resolve::qualify;

impl<'a> Walker<'_, 'a> {
    pub(super) fn trait_item(&mut self, t: &'a ItemTrait, inline: &[String]) {
        let name = qualify(inline, &t.ident.to_string());
        let start = self.start(Some(&t.vis), t.trait_token.span);
        let lines = (start, self.parsed.end_line(t.brace_token.span.close()));
        self.container("trait", name.clone(), lines, &t.attrs);
        let extends = t
            .supertraits
            .iter()
            .map(|b| self.parsed.short(b, 60))
            .collect();
        if let Some(last) = self.scopes.containers.last_mut() {
            last.extends = extends;
        }
        for item in &t.items {
            match item {
                TraitItem::Fn(f) => {
                    let start = self.parsed.line(f.sig.span());
                    let end = match &f.default {
                        Some(block) => self.parsed.end_line(block.brace_token.span.close()),
                        None => self.parsed.end_line(f.span()),
                    };
                    let parts = FnParts {
                        role: FnRole::Trait,
                        attrs: &f.attrs,
                        vis: None,
                        sig: &f.sig,
                        block: f.default.as_ref(),
                        lines: (start, end),
                    };
                    self.function(&name, inline, parts);
                }
                TraitItem::Const(c) => {
                    let line = self.parsed.line(c.const_token.span);
                    self.constant(&name, inline, ConstNode::TraitConst(c), line);
                }
                _ => {}
            }
        }
    }

    pub(super) fn impl_item(&mut self, i: &'a ItemImpl, inline: &[String]) {
        let name = self.self_type(&i.self_ty, inline);
        let start = self.parsed.line(i.impl_token.span);
        let end = self.parsed.end_line(i.brace_token.span.close());
        match self.scopes.containers.iter_mut().find(|c| c.name == name) {
            Some(c) if c.kind == "impl" => {
                c.start = c.start.min(start);
                c.end = c.end.max(end);
            }
            Some(_) => {}
            None if self.types.contains(&name) => {}
            None => self.container("impl", name.clone(), (start, end), &i.attrs),
        }
        let trait_name = i.trait_.as_ref().map(|(bang, path, _)| {
            let text = self.parsed.short(path, 60);
            if bang.is_some() {
                format!("!{text}")
            } else {
                text
            }
        });
        if let Some(t) = &trait_name {
            self.implements
                .entry(name.clone())
                .or_default()
                .push(t.clone());
        }
        for item in &i.items {
            match item {
                ImplItem::Fn(f) => {
                    let start = self.start(Some(&f.vis), f.sig.span());
                    let end = self.parsed.end_line(f.block.brace_token.span.close());
                    let parts = FnParts {
                        role: FnRole::Impl(trait_name.clone()),
                        attrs: &f.attrs,
                        vis: Some(&f.vis),
                        sig: &f.sig,
                        block: Some(&f.block),
                        lines: (start, end),
                    };
                    self.function(&name, inline, parts);
                }
                ImplItem::Const(c) => {
                    let line = self.start(Some(&c.vis), c.const_token.span);
                    self.constant(&name, inline, ConstNode::ImplConst(c), line);
                }
                _ => {}
            }
        }
    }

    /// Az `impl` céltípusának container-neve: saját típusnál a minősített név, különben a típus szövege.
    fn self_type(&self, ty: &Type, inline: &[String]) -> String {
        let base = match ty {
            Type::Path(p) if p.qself.is_none() => {
                p.path.segments.last().map(|s| s.ident.to_string())
            }
            Type::Reference(r) => return self.self_type(&r.elem, inline),
            _ => None,
        };
        let Some(base) = base else {
            return self.parsed.short(ty, 40);
        };
        (0..=inline.len())
            .rev()
            .map(|depth| qualify(&inline[..depth], &base))
            .find(|q| self.types.contains(q))
            .unwrap_or_else(|| qualify(inline, &base))
    }
}
