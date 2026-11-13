//! A PHP-emitter (Strategy): a nyelvfüggetlen AST-ból PSR-12 szerinti PHP-sorok. Szabályok:
//! - 4 szóköz behúzás, a nyitó kapcsos zárójel a deklaráció / metódus következő sorában, tagok között egy üres sor;
//! - a paraméter-lista többsoros (záró vesszővel), ha promoted ctor-paraméter van benne vagy 100 karakternél hosszabb;
//! - törzs nélkül: `void` / típus nélkül `// TODO`, nem-void visszatérésnél `throw new \LogicException('TODO');`,
//!   `Generator` / `iterable` / `iter<T>` visszatérésnél `yield from [];`; promoted ctor-nál üres törzs;
//! - a `list<T>` / `map<K,V>` natív típusa `array`, a docblock `@param` / `@return` / `@var` viszi a tartalmát;
//! - `?T` mező alapértéke `= null`, ha nincs megadva és nem readonly.

use std::collections::BTreeSet;

use super::builder::PhpFileBuilder;
use super::types::{doc_type, is_generator_type, member_names, needs_doc, php_type, type_names};
use crate::generate::ast::{Member, MemberKind, Param, TypeDecl, TypeKind, TypeRef, Visibility};
use crate::generate::emitter::{Emitter, FileSpec};
use crate::lang::php::PhpBackend;
use crate::lang::php::parser::{MagoAdapter, PhpParser};
use crate::model::FileGraph;

const IND: &str = "    ";
const TODO_THROW: &str = "throw new \\LogicException('TODO');";

/// A PHP célnyelv.
#[derive(Debug, Default, Clone, Copy)]
pub struct PhpEmitter;

fn vis_word(v: Visibility) -> &'static str {
    match v {
        Visibility::Public => "public",
        Visibility::Protected => "protected",
        Visibility::Private => "private",
    }
}

fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'"))
}

fn php_param(p: &Param) -> String {
    let mut s = String::new();
    if let Some(v) = p.promote {
        s.push_str(vis_word(v));
        s.push(' ');
    }
    if p.readonly {
        s.push_str("readonly ");
    }
    if let Some(ty) = &p.ty {
        s.push_str(&php_type(ty));
        s.push(' ');
    }
    if p.by_ref {
        s.push('&');
    }
    if p.variadic {
        s.push_str("...");
    }
    s.push('$');
    s.push_str(&p.name);
    if let Some(d) = &p.default {
        s.push_str(" = ");
        s.push_str(d);
    }
    s
}

impl PhpEmitter {
    fn header(&self, decl: &TypeDecl) -> String {
        let mut s = String::new();
        match decl.kind {
            TypeKind::Class | TypeKind::Struct => {
                if decl.is_abstract {
                    s.push_str("abstract ");
                }
                if decl.is_final {
                    s.push_str("final ");
                }
                if decl.is_readonly {
                    s.push_str("readonly ");
                }
                s.push_str("class ");
            }
            TypeKind::Interface => s.push_str("interface "),
            TypeKind::Trait => s.push_str("trait "),
            TypeKind::Enum => s.push_str("enum "),
        }
        s.push_str(&decl.name);
        if decl.kind == TypeKind::Enum
            && let Some(backing) = &decl.backing
        {
            s.push_str(": ");
            s.push_str(&php_type(backing));
        }
        if !decl.extends.is_empty()
            && matches!(
                decl.kind,
                TypeKind::Class | TypeKind::Struct | TypeKind::Interface
            )
        {
            s.push_str(" extends ");
            s.push_str(&Self::type_list(&decl.extends));
        }
        if !decl.implements.is_empty() && decl.kind != TypeKind::Interface {
            s.push_str(" implements ");
            s.push_str(&Self::type_list(&decl.implements));
        }
        s
    }

    fn type_list(types: &[TypeRef]) -> String {
        types.iter().map(php_type).collect::<Vec<_>>().join(", ")
    }

    fn case_lines(&self, decl: &TypeDecl) -> Vec<String> {
        let string_backed = decl
            .backing
            .as_ref()
            .is_some_and(|b| php_type(b) == "string");
        decl.cases
            .iter()
            .map(|case| match &case.value {
                Some(v) if string_backed && !v.starts_with(['\'', '"']) => {
                    format!("{IND}case {} = {};", case.name, quote(v))
                }
                Some(v) => format!("{IND}case {} = {v};", case.name),
                None => format!("{IND}case {};", case.name),
            })
            .collect()
    }

    fn member(&self, kind: TypeKind, m: &Member) -> Vec<String> {
        match m.kind {
            MemberKind::Field => self.field(m),
            MemberKind::Const => self.constant(m),
            MemberKind::Method | MemberKind::Ctor => self.method(kind, m),
        }
    }

    fn field(&self, m: &Member) -> Vec<String> {
        let mut lines = Vec::new();
        if let Some(ty) = &m.ty
            && needs_doc(ty)
        {
            lines.push(format!("{IND}/** @var {} */", doc_type(ty)));
        }
        let mut parts = vec![vis_word(m.visibility.unwrap_or(Visibility::Private)).to_owned()];
        if m.is_static {
            parts.push("static".to_owned());
        }
        if m.is_readonly {
            parts.push("readonly".to_owned());
        }
        if let Some(ty) = &m.ty {
            parts.push(php_type(ty));
        }
        parts.push(format!("${}", m.name));
        let mut line = format!("{IND}{}", parts.join(" "));
        let nullable = matches!(m.ty, Some(TypeRef::Opt(_))) && !m.is_readonly;
        match (&m.default, nullable) {
            (Some(d), _) => line.push_str(&format!(" = {d}")),
            (None, true) => line.push_str(" = null"),
            (None, false) => {}
        }
        line.push(';');
        lines.push(line);
        lines
    }

    fn constant(&self, m: &Member) -> Vec<String> {
        let mut line = String::from(IND);
        if m.is_final {
            line.push_str("final ");
        }
        line.push_str(vis_word(m.visibility.unwrap_or(Visibility::Public)));
        line.push_str(" const ");
        if let Some(ty) = &m.ty {
            line.push_str(&php_type(ty));
            line.push(' ');
        }
        line.push_str(&format!(
            "{} = {};",
            m.name,
            m.default.as_deref().unwrap_or("null")
        ));
        vec![line]
    }

    fn method_doc(&self, m: &Member) -> Vec<String> {
        let mut tags = Vec::new();
        for p in &m.params {
            if let Some(ty) = &p.ty
                && !p.variadic
                && needs_doc(ty)
            {
                tags.push(format!("@param {} ${}", doc_type(ty), p.name));
            }
        }
        if m.kind != MemberKind::Ctor
            && let Some(ty) = &m.ty
            && needs_doc(ty)
        {
            tags.push(format!("@return {}", doc_type(ty)));
        }
        if tags.is_empty() {
            return Vec::new();
        }
        let mut lines = vec![format!("{IND}/**")];
        lines.extend(tags.into_iter().map(|t| format!("{IND} * {t}")));
        lines.push(format!("{IND} */"));
        lines
    }

    fn body_lines(&self, m: &Member) -> Vec<String> {
        if let Some(body) = &m.body {
            return body
                .lines()
                .map(|l| {
                    if l.is_empty() {
                        String::new()
                    } else {
                        format!("{IND}{IND}{l}")
                    }
                })
                .collect();
        }
        let todo = || vec![format!("{IND}{IND}// TODO")];
        if m.kind == MemberKind::Ctor {
            return if m.params.iter().any(|p| p.promote.is_some()) {
                Vec::new()
            } else {
                todo()
            };
        }
        match &m.ty {
            Some(ty) if is_generator_type(ty) => vec![format!("{IND}{IND}yield from [];")],
            None | Some(TypeRef::Void) => todo(),
            Some(_) => vec![format!("{IND}{IND}{TODO_THROW}")],
        }
    }

    fn method(&self, kind: TypeKind, m: &Member) -> Vec<String> {
        let is_ctor = m.kind == MemberKind::Ctor;
        let name = if is_ctor {
            "__construct"
        } else {
            m.name.as_str()
        };
        let interface = kind == TypeKind::Interface;
        let params: Vec<String> = m.params.iter().map(php_param).collect();
        let ret = match (&m.ty, is_ctor) {
            (Some(ty), false) => format!(": {}", php_type(ty)),
            _ => String::new(),
        };
        let mut mods = String::new();
        if m.is_abstract && !interface {
            mods.push_str("abstract ");
        }
        if m.is_final {
            mods.push_str("final ");
        }
        mods.push_str(vis_word(m.visibility.unwrap_or(Visibility::Public)));
        mods.push(' ');
        if m.is_static {
            mods.push_str("static ");
        }
        mods.push_str("function ");
        let single = format!("{IND}{mods}{name}({}){ret}", params.join(", "));
        let multi = m.params.iter().any(|p| p.promote.is_some()) || single.chars().count() > 100;
        let last_variadic = m.params.last().is_some_and(|p| p.variadic);
        let mut lines = self.method_doc(m);
        let open = |lines: &mut Vec<String>| {
            lines.push(format!("{IND}{mods}{name}("));
            for (i, p) in params.iter().enumerate() {
                let comma = if last_variadic && i + 1 == params.len() {
                    ""
                } else {
                    ","
                };
                lines.push(format!("{IND}{IND}{p}{comma}"));
            }
        };
        if interface || m.is_abstract {
            if multi {
                open(&mut lines);
                lines.push(format!("{IND}){ret};"));
            } else {
                lines.push(format!("{single};"));
            }
            return lines;
        }
        let body = self.body_lines(m);
        if multi {
            open(&mut lines);
            if body.is_empty() {
                lines.push(format!("{IND}){ret} {{}}"));
                return lines;
            }
            lines.push(format!("{IND}){ret} {{"));
        } else {
            lines.push(single);
            lines.push(format!("{IND}{{"));
        }
        lines.extend(body);
        lines.push(format!("{IND}}}"));
        lines
    }
}

impl Emitter for PhpEmitter {
    fn extension(&self) -> &'static str {
        "php"
    }

    fn declaration<'a>(&'a self, decl: &'a TypeDecl) -> impl Iterator<Item = String> + 'a {
        let cases = self.case_lines(decl);
        let gap = if cases.is_empty() || decl.members.is_empty() {
            Vec::new()
        } else {
            vec![String::new()]
        };
        std::iter::once(self.header(decl))
            .chain(std::iter::once("{".to_owned()))
            .chain(cases)
            .chain(gap)
            .chain(self.members(decl.kind, &decl.members))
            .chain(std::iter::once("}".to_owned()))
    }

    fn members<'a>(
        &'a self,
        kind: TypeKind,
        members: &'a [Member],
    ) -> impl Iterator<Item = String> + 'a {
        members.iter().enumerate().flat_map(move |(i, m)| {
            let mut lines = Vec::new();
            if i > 0 {
                lines.push(String::new());
            }
            lines.extend(self.member(kind, m));
            lines
        })
    }

    fn referenced_names(&self, decls: &[TypeDecl], loose: &[Member]) -> BTreeSet<String> {
        let mut names = BTreeSet::new();
        for decl in decls {
            for ty in decl
                .extends
                .iter()
                .chain(&decl.implements)
                .chain(decl.backing.iter())
            {
                type_names(ty, &mut names);
            }
            for member in &decl.members {
                member_names(member, &mut names);
            }
        }
        for member in loose {
            member_names(member, &mut names);
        }
        names.retain(|n| !decls.iter().any(|d| &d.name == n));
        names
    }

    fn file(&self, spec: &FileSpec<'_>, decls: &[TypeDecl]) -> String {
        let mut builder = PhpFileBuilder::new()
            .strict_types(spec.strict_types)
            .namespace(spec.namespace.map(str::to_owned));
        for (fqcn, alias) in spec.imports {
            builder = builder.import(fqcn, alias.as_deref());
        }
        let mut body = Vec::new();
        for (i, decl) in decls.iter().enumerate() {
            if i > 0 {
                body.push(String::new());
            }
            body.extend(self.declaration(decl));
        }
        builder.body(body.into_iter()).build()
    }

    fn check(&self, path: &str, text: &str) -> Result<(), (u32, String)> {
        let errors = MagoAdapter.parse(path, text.as_bytes(), |parsed| parsed.parse_errors());
        match errors.into_iter().next() {
            Some(e) => Err((e.line, e.message)),
            None => Ok(()),
        }
    }

    fn analyze(&self, path: &str, text: &str) -> FileGraph {
        PhpBackend::with_defaults()
            .analyze_source(path, text.as_bytes())
            .graph
    }
}
