//! Osztályok és tagjaik helye egy meglévő `.ts` / `.js` fájlban (oxc-span alapján, nem szövegkereséssel):
//! az `--into x.ts::Osztály` és a `--replace x.ts::Osztály::tag` célpontja.

use std::path::Path;

use oxc_allocator::Allocator;
use oxc_ast::ast::{
    Class, ClassElement, Declaration, ExportDefaultDeclarationKind, PropertyKey, Statement,
};

use crate::lang::js::{parse_program, source_type_for};

/// Egy osztálytag (metódus, konstruktor, mező) sortartománya (0-alapú, zárt).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberSpan {
    pub name: String,
    pub start: usize,
    pub end: usize,
}

/// Egy osztály helye (0-alapú sorok): a deklaráció sora, a `{` és a záró `}` sora, a tagok.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassSpan {
    pub start: usize,
    pub open: usize,
    pub close: usize,
    pub members: Vec<MemberSpan>,
}

/// A `offset` bájt 0-alapú sora.
fn line_of(src: &str, offset: u32) -> usize {
    src.as_bytes()
        .iter()
        .take(offset as usize)
        .filter(|&&b| b == b'\n')
        .count()
}

/// A tag neve (azonosító vagy string kulcs; a privát `#x` is).
fn key_name(key: &PropertyKey<'_>) -> Option<String> {
    match key {
        PropertyKey::StaticIdentifier(i) => Some(i.name.to_string()),
        PropertyKey::Identifier(i) => Some(i.name.to_string()),
        PropertyKey::PrivateIdentifier(i) => Some(format!("#{}", i.name)),
        PropertyKey::StringLiteral(s) => s.value.as_str().map(str::to_string),
        _ => None,
    }
}

/// Az osztály neve és helye (a névtelen osztály nem jelölt).
fn class_span(class: &Class<'_>, src: &str) -> Option<(String, ClassSpan)> {
    let name = class.id.as_ref()?.name.to_string();
    let mut members = Vec::new();
    for element in &class.body.body {
        let (key, span) = match element {
            ClassElement::MethodDefinition(m) => (key_name(&m.key), m.span),
            ClassElement::PropertyDefinition(p) => (key_name(&p.key), p.span),
            _ => continue,
        };
        if let Some(name) = key {
            members.push(MemberSpan {
                name,
                start: line_of(src, span.start),
                end: line_of(src, span.end.saturating_sub(1)),
            });
        }
    }
    let span = ClassSpan {
        start: line_of(src, class.span.start),
        open: line_of(src, class.body.span.start),
        close: line_of(src, class.body.span.end.saturating_sub(1)),
        members,
    };
    Some((name, span))
}

/// A fájl felső szintű osztályai: (név, hely); az `export` / `export default` osztály is.
fn classes(rel: &str, src: &str) -> Vec<(String, ClassSpan)> {
    let allocator = Allocator::default();
    let parsed = parse_program(&allocator, src, source_type_for(Path::new(rel), None));
    let mut out = Vec::new();
    for stmt in &parsed.program.body {
        let found = match stmt {
            Statement::ExportDeclaration(e) => match &e.declaration {
                Declaration::ClassDeclaration(c) => class_span(c, src),
                _ => None,
            },
            Statement::ExportDefaultDeclaration(e) => match &e.declaration {
                ExportDefaultDeclarationKind::ClassDeclaration(c) => class_span(c, src),
                _ => None,
            },
            other => match other.as_declaration() {
                Some(Declaration::ClassDeclaration(c)) => class_span(c, src),
                _ => None,
            },
        };
        out.extend(found);
    }
    out
}

/// A `name` nevű felső szintű osztály helye.
pub fn find_class(rel: &str, src: &str, name: &str) -> Option<ClassSpan> {
    classes(rel, src)
        .into_iter()
        .find(|(n, _)| n == name)
        .map(|(_, span)| span)
}

/// A fájl felső szintű osztályainak nevei (hibaüzenethez).
pub fn class_names(rel: &str, src: &str) -> Vec<String> {
    classes(rel, src).into_iter().map(|(n, _)| n).collect()
}
