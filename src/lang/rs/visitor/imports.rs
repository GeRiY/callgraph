//! `imports`: a `use`-fák kilapítva (`use a::{b, c as d}` → `b → a::b`, `d → a::c`), plusz `extern crate`.
//! A kimenet a leírt utat adja; a névtáblába a kanonikus (`self::` / `super::` nélküli) út kerül.

use syn::visit::Visit;
use syn::{Item, UseTree};

use super::{FileVisitor, Scopes};
use crate::lang::rs::context::RsContext;
use crate::lang::rs::parser::ParsedFile;
use crate::model::FileGraphBuilder;

/// Az `imports` mezőt és a névtábla importjait tölti (a fájl minden `use`-ja, a függvényeken belüliek is).
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct ImportsVisitor;

impl FileVisitor for ImportsVisitor {
    fn visit(
        &self,
        parsed: &ParsedFile<'_>,
        _scopes: &Scopes<'_>,
        ctx: &mut RsContext,
        builder: &mut FileGraphBuilder,
    ) {
        let mut collector = Collector::default();
        collector.items(&parsed.file.items);
        for (inline, alias, written) in collector.found {
            let canonical = ctx.resolver.canonical(&inline, &written);
            builder.import(alias.clone(), written.join("::"));
            if alias != "*" && !alias.ends_with("::*") {
                ctx.resolver.add_import(alias, canonical);
            }
        }
        builder.imports_as_object();
    }
}

#[derive(Default)]
struct Collector {
    inline: Vec<String>,
    /// (inline modulok, alias, leírt út)
    found: Vec<(Vec<String>, String, Vec<String>)>,
}

impl Collector {
    fn items(&mut self, items: &[Item]) {
        for item in items {
            self.visit_item(item);
        }
    }

    fn tree(&mut self, prefix: &mut Vec<String>, tree: &UseTree) {
        match tree {
            UseTree::Path(p) => {
                prefix.push(p.ident.to_string());
                self.tree(prefix, &p.tree);
                prefix.pop();
            }
            UseTree::Name(n) => {
                let name = n.ident.to_string();
                if name == "self" {
                    let alias = prefix.last().cloned().unwrap_or_default();
                    self.push(alias, prefix.clone());
                } else {
                    let mut full = prefix.clone();
                    full.push(name.clone());
                    self.push(name, full);
                }
            }
            UseTree::Rename(r) => {
                let mut full = prefix.clone();
                if r.ident != "self" {
                    full.push(r.ident.to_string());
                }
                self.push(r.rename.to_string(), full);
            }
            UseTree::Glob(_) => {
                let mut full = prefix.clone();
                full.push("*".to_owned());
                self.push(full.join("::"), full);
            }
            UseTree::Group(g) => {
                for item in &g.items {
                    self.tree(prefix, item);
                }
            }
        }
    }

    fn push(&mut self, alias: String, full: Vec<String>) {
        if alias != "_" {
            self.found.push((self.inline.clone(), alias, full));
        }
    }
}

impl<'ast> Visit<'ast> for Collector {
    fn visit_item_use(&mut self, node: &'ast syn::ItemUse) {
        let mut prefix = Vec::new();
        self.tree(&mut prefix, &node.tree);
    }

    fn visit_item_extern_crate(&mut self, node: &'ast syn::ItemExternCrate) {
        let name = node.ident.to_string();
        let alias = node
            .rename
            .as_ref()
            .map_or_else(|| name.clone(), |(_, r)| r.to_string());
        self.push(alias, vec![name]);
    }

    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        self.inline.push(node.ident.to_string());
        syn::visit::visit_item_mod(self, node);
        self.inline.pop();
    }
}
