//! A `strings` mező script-részre: StringLiteral és TemplateLiteral szabályok (a JS-motor `walk`-ja szerint).

use std::collections::HashSet;

use oxc_ast::ast::*;
use oxc_ast_visit::{Visit, walk};

use super::analysis::{JsAnalysis, JsPass, PassContext};
use super::scope::text::{js_trim, truncate_utf16};
use super::scope::{Scopes, scope_hooks};
use crate::model::StringLit;

/// A string-literálok gyűjtése.
pub struct StringsPass;

impl JsPass for StringsPass {
    fn run(&self, ctx: &PassContext<'_>, program: &Program<'_>, out: &mut JsAnalysis) {
        let mut v = StringVisitor {
            ctx,
            scopes: Scopes::build(program, ctx),
            skip: HashSet::new(),
            found: Vec::new(),
        };
        v.visit_program(program);
        for s in v.found {
            out.builder.string(s);
        }
    }
}

struct StringVisitor<'c> {
    ctx: &'c PassContext<'c>,
    scopes: Scopes,
    /// A kihagyott literálok kezdő offsetjei (import-forrás, objektum-kulcs).
    skip: HashSet<u32>,
    found: Vec<StringLit>,
}

impl StringVisitor<'_> {
    fn push(&mut self, start: u32, value: &str) {
        self.found.push(StringLit {
            container: Some(self.scopes.container().to_string()),
            caller: Some(self.scopes.caller().to_string()),
            line: self.ctx.lines.line_of(start as usize),
            end_line: None,
            value: truncate_utf16(value, 300).to_string(),
        });
    }

    fn skip_key(&mut self, key: &PropertyKey<'_>) {
        if let PropertyKey::StringLiteral(s) = key {
            self.skip.insert(s.span.start);
        }
    }

    fn template(&mut self, start: u32, quasis: &[TemplateElement<'_>]) {
        let v = quasis
            .iter()
            .map(|q| {
                q.value
                    .cooked
                    .as_ref()
                    .map_or_else(|| q.value.raw.to_string(), |c| c.display().to_string())
            })
            .collect::<Vec<_>>()
            .join("${…}");
        if !js_trim(&v.replace("${…}", "")).is_empty() {
            self.push(start, &v);
        }
    }
}

impl<'a> Visit<'a> for StringVisitor<'_> {
    scope_hooks!();

    fn visit_directive(&mut self, _it: &Directive<'a>) {}

    fn visit_import_declaration(&mut self, it: &ImportDeclaration<'a>) {
        self.skip.insert(it.source.span.start);
        walk::walk_import_declaration(self, it);
    }

    fn visit_export_from_declaration(&mut self, it: &ExportFromDeclaration<'a>) {
        self.skip.insert(it.source.span.start);
        walk::walk_export_from_declaration(self, it);
    }

    fn visit_export_all_declaration(&mut self, it: &ExportAllDeclaration<'a>) {
        self.skip.insert(it.source.span.start);
        walk::walk_export_all_declaration(self, it);
    }

    fn visit_object_property(&mut self, it: &ObjectProperty<'a>) {
        self.skip_key(&it.key);
        walk::walk_object_property(self, it);
    }

    fn visit_binding_property(&mut self, it: &BindingProperty<'a>) {
        self.skip_key(&it.key);
        walk::walk_binding_property(self, it);
    }

    fn visit_assignment_target_property_property(
        &mut self,
        it: &AssignmentTargetPropertyProperty<'a>,
    ) {
        self.skip_key(&it.name);
        walk::walk_assignment_target_property_property(self, it);
    }

    /// A Babel `SwitchCase`-ben a `consequent` kulcs megelőzi a `test`-et: a sorrend ehhez igazodik.
    fn visit_switch_case(&mut self, it: &SwitchCase<'a>) {
        self.visit_statements(&it.consequent);
        if let Some(t) = &it.test {
            self.visit_expression(t);
        }
    }

    fn visit_string_literal(&mut self, it: &StringLiteral<'a>) {
        if self.skip.contains(&it.span.start) {
            return;
        }
        let value = it.value.display().to_string();
        if !js_trim(&value).is_empty() {
            self.push(it.span.start, &value);
        }
    }

    fn visit_template_literal(&mut self, it: &TemplateLiteral<'a>) {
        self.template(it.span.start, &it.quasis);
        walk::walk_template_literal(self, it);
    }

    fn visit_ts_template_literal_type(&mut self, it: &TSTemplateLiteralType<'a>) {
        self.template(it.span.start, &it.quasis);
        walk::walk_ts_template_literal_type(self, it);
    }
}

#[cfg(test)]
mod tests {
    use crate::core::SourceFile;
    use crate::lang::Language;
    use crate::lang::js::JsFrontend;

    #[test]
    fn collects_strings_with_caller_and_skips_keys_and_imports() {
        let src = "import a from 'x/y';\nconst o = { 'k': 1 };\nfunction f() { return `a${1}b` + 'hi' + ' '; }\n";
        let f = SourceFile::new("/p/m.js", src.as_bytes().to_vec(), None);
        let g = JsFrontend::with_defaults().analyze(&f).unwrap().graph;
        let got: Vec<_> = g
            .strings
            .iter()
            .map(|s| (s.caller.clone().unwrap(), s.value.clone()))
            .collect();
        assert_eq!(
            got,
            [("f".into(), "a${…}b".into()), ("f".into(), "hi".into())]
        );
    }
}
