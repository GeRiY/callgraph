//! The props a React component accepts, read from a TS type: a literal, a local or imported `interface` / `type`
//! alias, `extends`, `A & B`, `PropsWithChildren<T>`. Whatever cannot be resolved (`Omit<…>`, `HTMLAttributes`, a
//! package type, a union) makes the set open: an attribute the set does not hold is then not "unknown".

use std::path::Path;

use oxc_ast::ast::*;

use crate::lang::js::parse::{parse_program, source_type_for};
use crate::lang::js::props::key_name;
use crate::lang::js::resolve::Resolver;

/// The biggest file read for an imported props type.
const MAX_TYPE_FILE: u64 = 1_500_000;

/// The declared props (name, required) and whether more may exist.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(super) struct PropSet {
    pub props: Vec<(String, bool)>,
    pub open: bool,
}

impl PropSet {
    /// A prop; a second mention keeps the stronger `required`.
    pub fn add(&mut self, name: &str, required: bool) {
        match self.props.iter_mut().find(|(n, _)| n == name) {
            Some(entry) => entry.1 |= required,
            None => self.props.push((name.to_string(), required)),
        }
    }

    /// The prop is optional from now on (a destructuring default, `defaultProps`).
    pub fn make_optional(&mut self, name: &str) {
        match self.props.iter_mut().find(|(n, _)| n == name) {
            Some(entry) => entry.1 = false,
            None => self.props.push((name.to_string(), false)),
        }
    }
}

/// Where the type names are looked up.
pub(super) struct Types<'s, 'a> {
    pub program: &'s Program<'a>,
    pub path: &'s Path,
    pub resolver: &'s Resolver,
}

impl Types<'_, '_> {
    pub fn collect(&self, ty: &TSType<'_>, depth: usize, out: &mut PropSet) {
        // self-referencing / circular type chains
        if depth > 6 {
            out.open = true;
            return;
        }
        match ty {
            TSType::TSTypeLiteral(lit) => self.members(&lit.members, out),
            TSType::TSIntersectionType(i) => {
                for part in &i.types {
                    self.collect(part, depth + 1, out);
                }
            }
            TSType::TSParenthesizedType(p) => self.collect(&p.type_annotation, depth + 1, out),
            TSType::TSTypeReference(r) => self.reference(r, depth, out),
            _ => out.open = true,
        }
    }

    fn reference(&self, r: &TSTypeReference<'_>, depth: usize, out: &mut PropSet) {
        let (name, qualified) = match &r.type_name {
            TSTypeName::IdentifierReference(id) => (id.name.to_string(), false),
            TSTypeName::QualifiedName(q) => (q.right.name.to_string(), true),
            TSTypeName::ThisExpression(_) => {
                out.open = true;
                return;
            }
        };
        let first_arg = r.type_arguments.as_ref().and_then(|a| a.params.first());
        if matches!(name.as_str(), "PropsWithChildren" | "PropsWithoutRef") {
            out.add("children", false);
            match first_arg {
                Some(arg) => self.collect(arg, depth + 1, out),
                None => out.open = true,
            }
            return;
        }
        if qualified {
            out.open = true;
            return;
        }
        self.named(&name, depth + 1, out);
    }

    /// A top-level `interface <name>` / `type <name> = …` of the file, else an imported one; unresolved: open.
    fn named(&self, name: &str, depth: usize, out: &mut PropSet) {
        if depth > 6 {
            out.open = true;
            return;
        }
        for stmt in &self.program.body {
            let decl = match stmt {
                Statement::ExportDeclaration(e) => Some(&e.declaration),
                other => other.as_declaration(),
            };
            match decl {
                Some(Declaration::TSInterfaceDeclaration(i)) if i.id.name == name => {
                    for parent in &i.extends {
                        match &parent.type_name {
                            TSTypeName::IdentifierReference(id) => {
                                self.named(id.name.as_str(), depth + 1, out)
                            }
                            _ => out.open = true,
                        }
                    }
                    self.members(&i.body.body, out);
                    return;
                }
                Some(Declaration::TSTypeAliasDeclaration(a)) if a.id.name == name => {
                    self.collect(&a.type_annotation, depth + 1, out);
                    return;
                }
                _ => {}
            }
        }
        self.imported(name, depth, out);
    }

    /// `import type { Props } from './types'`: read from the imported file (a relative or aliased import).
    fn imported(&self, name: &str, depth: usize, out: &mut PropSet) {
        for stmt in &self.program.body {
            let Statement::ImportDeclaration(decl) = stmt else {
                continue;
            };
            for spec in decl.specifiers.iter().flatten() {
                let (local, remote) = match spec {
                    ImportDeclarationSpecifier::ImportSpecifier(s) => {
                        (s.local.name.as_str(), s.imported.name().to_string())
                    }
                    ImportDeclarationSpecifier::ImportDefaultSpecifier(_)
                    | ImportDeclarationSpecifier::ImportNamespaceSpecifier(_) => continue,
                };
                if local != name {
                    continue;
                }
                let source = decl.source.value.display().to_string();
                let Some(file) = self.resolver.resolve_file(&source, self.path) else {
                    out.open = true;
                    return;
                };
                let readable = std::fs::metadata(&file)
                    .ok()
                    .is_some_and(|m| m.len() <= MAX_TYPE_FILE);
                let text = readable
                    .then(|| std::fs::read_to_string(&file).ok())
                    .flatten();
                let Some(text) = text else {
                    out.open = true;
                    return;
                };
                let allocator = oxc_allocator::Allocator::default();
                let parsed = parse_program(&allocator, &text, source_type_for(&file, None));
                let types = Types {
                    program: &parsed.program,
                    path: &file,
                    resolver: self.resolver,
                };
                types.named(&remote, depth + 1, out);
                return;
            }
        }
        // not imported here: a global or package type
        out.open = true;
    }

    fn members(&self, members: &[TSSignature<'_>], out: &mut PropSet) {
        for m in members {
            let (key, optional) = match m {
                TSSignature::TSPropertySignature(p) => (&p.key, p.optional),
                TSSignature::TSMethodSignature(p) => (&p.key, p.optional),
                TSSignature::TSCallSignatureDeclaration(_)
                | TSSignature::TSConstructSignatureDeclaration(_) => continue,
                _ => {
                    out.open = true;
                    continue;
                }
            };
            match key_name(key) {
                Some(name) => out.add(name, !optional),
                None => out.open = true,
            }
        }
    }
}
