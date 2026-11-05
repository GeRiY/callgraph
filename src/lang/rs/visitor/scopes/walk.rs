//! Az elemek bejárása (inline modulokkal együtt) a `Scopes`-ba.

use std::collections::{HashMap, HashSet};

use proc_macro2::Span;
use syn::spanned::Spanned;
use syn::{Attribute, Block, Field, Item, Signature, Visibility};

use super::{
    ConstNode, ConstScope, ContainerScope, FieldScope, FnRole, FnScope, MacroScope, Scopes,
};
use crate::lang::rs::module_path::ModulePath;
use crate::lang::rs::parser::ParsedFile;
use crate::lang::rs::resolve::{qualify, qualify_container};

/// A fájl elemeinek összegyűjtése; a függvénynév-ütközés containeren belül `név@L<sor>`.
pub(crate) fn collect<'a>(parsed: &ParsedFile<'a>, module: &ModulePath) -> Scopes<'a> {
    let mut walker = Walker {
        parsed,
        module,
        types: HashSet::new(),
        scopes: Scopes::default(),
        implements: HashMap::new(),
    };
    walker.type_names(&parsed.file.items, &mut Vec::new());
    let name = module.container_name();
    walker.container("module", name, (1, parsed.line_count()), &parsed.file.attrs);
    walker.items(&parsed.file.items, &mut Vec::new());
    let Walker {
        mut scopes,
        implements,
        ..
    } = walker;
    for container in &mut scopes.containers {
        if let Some(list) = implements.get(&container.name) {
            container.implements = list.clone();
        }
    }
    let mut seen: HashSet<(String, String)> = HashSet::new();
    for f in &mut scopes.fns {
        if !seen.insert((f.container.clone(), f.name.clone())) {
            f.name = format!("{}@L{}", f.name, f.start);
        }
    }
    scopes
}

/// Egy függvény részei (a `FnScope` összeállításához).
pub(super) struct FnParts<'a> {
    pub role: FnRole,
    pub attrs: &'a [Attribute],
    pub vis: Option<&'a Visibility>,
    pub sig: &'a Signature,
    pub block: Option<&'a Block>,
    pub lines: (u32, u32),
}

pub(super) struct Walker<'p, 'a> {
    pub parsed: &'p ParsedFile<'a>,
    module: &'p ModulePath,
    pub types: HashSet<String>,
    pub scopes: Scopes<'a>,
    pub implements: HashMap<String, Vec<String>>,
}

impl<'a> Walker<'_, 'a> {
    /// Előzetes kör: a saját típusok minősített nevei (az `impl X` célja ettől függ).
    fn type_names(&mut self, items: &'a [Item], inline: &mut Vec<String>) {
        for item in items {
            let name = match item {
                Item::Struct(s) => &s.ident,
                Item::Enum(e) => &e.ident,
                Item::Union(u) => &u.ident,
                Item::Trait(t) => &t.ident,
                Item::Mod(m) => {
                    if let Some((_, content)) = &m.content {
                        inline.push(m.ident.to_string());
                        self.type_names(content, inline);
                        inline.pop();
                    }
                    continue;
                }
                _ => continue,
            };
            self.types.insert(qualify(inline, &name.to_string()));
        }
    }

    /// Az elem kezdősora: a láthatóság (`pub`) vagy a kulcsszó sora — az attribútumok és a doc nélkül.
    pub(super) fn start(&self, vis: Option<&Visibility>, keyword: Span) -> u32 {
        match vis {
            Some(Visibility::Public(token)) => self.parsed.line(token.span),
            Some(Visibility::Restricted(r)) => self.parsed.line(r.pub_token.span),
            _ => self.parsed.line(keyword),
        }
    }

    pub(super) fn container(
        &mut self,
        kind: &'static str,
        name: String,
        (start, end): (u32, u32),
        attrs: &'a [Attribute],
    ) {
        self.scopes.containers.push(ContainerScope {
            kind,
            name,
            start,
            end,
            attrs,
            extends: Vec::new(),
            implements: Vec::new(),
        });
    }

    pub(super) fn function(&mut self, container: &str, inline: &[String], parts: FnParts<'a>) {
        self.scopes.fns.push(FnScope {
            container: container.to_owned(),
            name: parts.sig.ident.to_string(),
            inline: inline.to_vec(),
            role: parts.role,
            attrs: parts.attrs,
            vis: parts.vis,
            sig: parts.sig,
            block: parts.block,
            start: parts.lines.0,
            end: parts.lines.1,
        });
    }

    pub(super) fn constant(
        &mut self,
        container: &str,
        inline: &[String],
        node: ConstNode<'a>,
        line: u32,
    ) {
        self.scopes.consts.push(ConstScope {
            container: container.to_owned(),
            inline: inline.to_vec(),
            node,
            line,
        });
    }

    fn fields(&mut self, container: &str, fields: impl Iterator<Item = &'a Field>) {
        for (index, field) in fields.enumerate() {
            self.scopes.fields.push(FieldScope {
                container: container.to_owned(),
                field,
                index,
            });
        }
    }

    fn items(&mut self, items: &'a [Item], inline: &mut Vec<String>) {
        let own = qualify_container(self.module, inline);
        for item in items {
            match item {
                Item::Fn(f) => {
                    let start = self.start(Some(&f.vis), f.sig.span());
                    let end = self.parsed.end_line(f.block.brace_token.span.close());
                    let parts = FnParts {
                        role: FnRole::Free,
                        attrs: &f.attrs,
                        vis: Some(&f.vis),
                        sig: &f.sig,
                        block: Some(&f.block),
                        lines: (start, end),
                    };
                    self.function(&own, inline, parts);
                }
                Item::Struct(s) => {
                    let name = qualify(inline, &s.ident.to_string());
                    let start = self.start(Some(&s.vis), s.struct_token.span);
                    let lines = (start, self.parsed.end_line(s.span()));
                    self.container("struct", name.clone(), lines, &s.attrs);
                    self.fields(&name, s.fields.iter());
                }
                Item::Union(u) => {
                    let name = qualify(inline, &u.ident.to_string());
                    let start = self.start(Some(&u.vis), u.union_token.span);
                    let lines = (start, self.parsed.end_line(u.span()));
                    self.container("union", name.clone(), lines, &u.attrs);
                    self.fields(&name, u.fields.named.iter());
                }
                Item::Enum(e) => {
                    let name = qualify(inline, &e.ident.to_string());
                    let start = self.start(Some(&e.vis), e.enum_token.span);
                    let lines = (start, self.parsed.end_line(e.brace_token.span.close()));
                    self.container("enum", name.clone(), lines, &e.attrs);
                    for v in &e.variants {
                        let line = self.parsed.line(v.ident.span());
                        self.constant(&name, inline, ConstNode::Variant(v), line);
                    }
                }
                Item::Trait(t) => self.trait_item(t, inline),
                Item::Impl(i) => self.impl_item(i, inline),
                Item::Mod(m) => {
                    let Some((brace, content)) = &m.content else {
                        continue;
                    };
                    inline.push(m.ident.to_string());
                    let start = self.start(Some(&m.vis), m.mod_token.span);
                    let lines = (start, self.parsed.end_line(brace.span.close()));
                    self.container("mod", inline.join("::"), lines, &m.attrs);
                    self.items(content, inline);
                    inline.pop();
                }
                Item::Const(c) => {
                    let line = self.start(Some(&c.vis), c.const_token.span);
                    self.constant(&own, inline, ConstNode::Const(c), line);
                }
                Item::Static(s) => {
                    let line = self.start(Some(&s.vis), s.static_token.span);
                    self.constant(&own, inline, ConstNode::Static(s), line);
                }
                Item::Type(t) => {
                    let line = self.start(Some(&t.vis), t.type_token.span);
                    self.constant(&own, inline, ConstNode::Type(t), line);
                }
                Item::Macro(m) => {
                    let scope = match &m.ident {
                        Some(ident) if m.mac.path.is_ident("macro_rules") => MacroScope::Def {
                            container: own.clone(),
                            item: m,
                            name: format!("{ident}!"),
                        },
                        _ => MacroScope::Call {
                            container: own.clone(),
                            inline: inline.clone(),
                            mac: &m.mac,
                        },
                    };
                    self.scopes.macros.push(scope);
                }
                _ => {}
            }
        }
    }
}
