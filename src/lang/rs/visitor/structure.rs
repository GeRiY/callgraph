//! Szerkezet: containerek, metódusok / függvények, property-k (struct-mezők) és konstansok
//! (`const`, `static`, `type`, enum-variáns) a `Scopes`-ból.

use syn::{FnArg, ReturnType};

use super::attrs::{doc_of, visibility};
use super::scopes::{ConstNode, FnRole, FnScope, MacroScope};
use super::{FileVisitor, Scopes};
use crate::lang::rs::context::RsContext;
use crate::lang::rs::parser::{ParsedFile, shorten, squash};
use crate::model::{Constant, Container, FileGraphBuilder, Method, Property};

/// A `containers`, `methods`, `properties`, `constants` mezőket tölti.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct StructureVisitor;

impl FileVisitor for StructureVisitor {
    fn visit(
        &self,
        parsed: &ParsedFile<'_>,
        scopes: &Scopes<'_>,
        _ctx: &mut RsContext,
        builder: &mut FileGraphBuilder,
    ) {
        for c in &scopes.containers {
            let list = |v: &Vec<String>| (!v.is_empty()).then(|| v.clone());
            builder.container(Container {
                kind: c.kind.to_owned(),
                name: c.name.clone(),
                line: c.start,
                end_line: c.end,
                is_abstract: None,
                is_final: None,
                extends: list(&c.extends),
                implements: list(&c.implements),
                traits: None,
                doc: doc_of(c.attrs),
            });
        }
        for f in &scopes.fns {
            builder.method(method(parsed, f));
        }
        for m in &scopes.macros {
            if let MacroScope::Def {
                container,
                item,
                name,
            } = m
            {
                let (line, end_line) = parsed.lines(&item.mac);
                builder.method(Method {
                    container: Some(container.clone()),
                    name: name.clone(),
                    line,
                    end_line: Some(end_line),
                    visibility: Some(macro_visibility(item)),
                    is_static: false,
                    is_abstract: false,
                    kind: Some("macro_rules".to_owned()),
                    params: Vec::new(),
                    return_type: None,
                    doc: doc_of(&item.attrs),
                    body: None,
                });
            }
        }
        for field in &scopes.fields {
            let f = field.field;
            builder.property(Property {
                container: Some(field.container.clone()),
                name: f
                    .ident
                    .as_ref()
                    .map_or_else(|| field.index.to_string(), ToString::to_string),
                line: f
                    .ident
                    .as_ref()
                    .map_or_else(|| parsed.lines(&f.ty).0, |i| parsed.line(i.span())),
                type_hint: Some(parsed.short(&f.ty, 60)),
                visibility: visibility(&f.vis),
                is_static: false,
                readonly: false,
                doc: doc_of(&f.attrs),
                js: false,
            });
        }
        for c in &scopes.consts {
            builder.constant(constant(parsed, &c.container, &c.node, c.line));
        }
    }
}

fn macro_visibility(item: &syn::ItemMacro) -> String {
    let exported = item.attrs.iter().any(|a| a.path().is_ident("macro_export"));
    if exported { "pub" } else { "private" }.to_owned()
}

/// A metódus kimeneti alakja; a `kind`: `fn` / `method` / `assoc` (`async ` előtaggal, ha aszinkron).
fn method(parsed: &ParsedFile<'_>, f: &FnScope<'_>) -> Method {
    let base = match (&f.role, f.has_receiver()) {
        (FnRole::Free, _) => "fn",
        (_, true) => "method",
        (_, false) => "assoc",
    };
    let kind = if f.sig.asyncness.is_some() {
        format!("async {base}")
    } else {
        base.to_owned()
    };
    let visibility = match (&f.role, f.vis) {
        (FnRole::Trait | FnRole::Impl(Some(_)), _) => "pub".to_owned(),
        (_, Some(vis)) => visibility(vis),
        (_, None) => "private".to_owned(),
    };
    Method {
        container: Some(f.container.clone()),
        name: f.name.clone(),
        line: f.start,
        end_line: Some(f.end),
        visibility: Some(visibility),
        is_static: !matches!(f.role, FnRole::Free) && !f.has_receiver(),
        is_abstract: matches!(f.role, FnRole::Trait) && f.block.is_none(),
        kind: Some(kind),
        params: f.sig.inputs.iter().map(|arg| param(parsed, arg)).collect(),
        return_type: match &f.sig.output {
            ReturnType::Default => None,
            ReturnType::Type(_, ty) => Some(parsed.short(&**ty, 80)),
        },
        doc: doc_of(f.attrs),
        body: None,
    }
}

/// Egy paraméter forrásszövege (attribútumok nélkül), egy sorba vonva, ≤60 karakter.
fn param(parsed: &ParsedFile<'_>, arg: &FnArg) -> String {
    let text = match arg {
        FnArg::Receiver(r) => match &r.colon_token {
            Some(_) => parsed.text(r),
            None => {
                let reference = r.reference.as_ref().map_or("", |_| "&");
                let mutable = r.mutability.as_ref().map_or("", |_| "mut ");
                format!("{reference}{mutable}self")
            }
        },
        FnArg::Typed(t) => format!("{}: {}", parsed.text(&t.pat), parsed.text(&t.ty)),
    };
    shorten(&squash(&text), 60)
}

fn constant(parsed: &ParsedFile<'_>, container: &str, node: &ConstNode<'_>, line: u32) -> Constant {
    let value = |expr: &syn::Expr| Some(parsed.short(expr, 80));
    let (kind, name, value, vis, attrs) = match node {
        ConstNode::Const(c) => ("const", &c.ident, value(&c.expr), Some(&c.vis), &c.attrs),
        ConstNode::Static(s) => {
            let kind = if matches!(s.mutability, syn::StaticMutability::Mut(_)) {
                "static mut"
            } else {
                "static"
            };
            (kind, &s.ident, value(&s.expr), Some(&s.vis), &s.attrs)
        }
        ConstNode::Type(t) => {
            let ty = Some(parsed.short(&*t.ty, 80));
            ("type", &t.ident, ty, Some(&t.vis), &t.attrs)
        }
        ConstNode::ImplConst(c) => ("const", &c.ident, value(&c.expr), Some(&c.vis), &c.attrs),
        ConstNode::TraitConst(c) => {
            let default = c.default.as_ref().and_then(|(_, e)| value(e));
            ("const", &c.ident, default, None, &c.attrs)
        }
        ConstNode::Variant(v) => {
            let discriminant = v.discriminant.as_ref().and_then(|(_, e)| value(e));
            ("enum_case", &v.ident, discriminant, None, &v.attrs)
        }
    };
    Constant {
        container: Some(container.to_owned()),
        kind: kind.to_owned(),
        name: name.to_string(),
        line,
        value,
        visibility: vis.map(visibility),
        caller: None,
        doc: doc_of(attrs),
        js: false,
    }
}
