//! A függvénytörzsek és a felső szintű kifejezések (konstans-értékek, makróhívások) közös bejárása:
//! minden törzs előtt a visitor megkapja a helyet (container, caller, inline modulok).

use std::collections::HashSet;

use syn::visit::Visit;

use super::receivers::TypeEnv;
use super::scopes::{ConstNode, MacroScope, Scopes};

/// A bejárt kód helye.
pub(super) struct Location {
    pub container: String,
    pub caller: String,
    pub inline: Vec<String>,
    /// A függvény paraméterei és helyi kötései (`let x`, closure-paraméter).
    pub bindings: HashSet<String>,
    /// A helyi nevek következtetett típusa (a metódushívások fogadójához).
    pub types: TypeEnv,
}

/// Egy törzs-bejáró: bármely élettartamú szintaxisfán fut (a makró-argumentumok saját fát kapnak).
pub(super) trait BodyVisit: for<'ast> Visit<'ast> {
    fn enter(&mut self, location: Location);
}

/// Minden függvénytörzs, konstans-érték és felső szintű makróhívás bejárása.
pub(super) fn walk_bodies<V: BodyVisit>(scopes: &Scopes<'_>, visitor: &mut V) {
    for f in &scopes.fns {
        let Some(block) = f.block else {
            continue;
        };
        let mut bindings = Bindings::default();
        for input in &f.sig.inputs {
            bindings.visit_fn_arg(input);
        }
        bindings.visit_block(block);
        visitor.enter(Location {
            container: f.container.clone(),
            caller: f.name.clone(),
            inline: f.inline.clone(),
            bindings: bindings.0,
            types: TypeEnv::of_fn(f.sig, block),
        });
        visitor.visit_block(block);
    }
    for c in &scopes.consts {
        let expr = match c.node {
            ConstNode::Const(item) => Some(&*item.expr),
            ConstNode::Static(item) => Some(&*item.expr),
            ConstNode::ImplConst(item) => Some(&item.expr),
            ConstNode::TraitConst(item) => item.default.as_ref().map(|(_, e)| e),
            ConstNode::Variant(v) => v.discriminant.as_ref().map(|(_, e)| e),
            ConstNode::Type(_) => None,
        };
        if let Some(expr) = expr {
            visitor.enter(top(&c.container, &c.inline));
            visitor.visit_expr(expr);
        }
    }
    for m in &scopes.macros {
        if let MacroScope::Call {
            container,
            inline,
            mac,
        } = m
        {
            visitor.enter(top(container, inline));
            visitor.visit_macro(mac);
        }
    }
}

fn top(container: &str, inline: &[String]) -> Location {
    Location {
        container: container.to_owned(),
        caller: "__top__".to_owned(),
        inline: inline.to_vec(),
        bindings: HashSet::new(),
        types: TypeEnv::default(),
    }
}

/// A mintákban kötött azonosítók gyűjtése.
#[derive(Default)]
struct Bindings(HashSet<String>);

impl<'ast> Visit<'ast> for Bindings {
    fn visit_pat_ident(&mut self, node: &'ast syn::PatIdent) {
        self.0.insert(node.ident.to_string());
        syn::visit::visit_pat_ident(self, node);
    }
}
