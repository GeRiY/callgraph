//! Fájlszintű mezők: framework, module, sfc és az osztály-containerek.

use std::path::Path;

use oxc_ast::ast::{Class, Declaration, ExportDefaultDeclarationKind, Program, Statement};
use oxc_span::GetSpan;

use super::analysis::{JsAnalysis, JsPass, PassContext};
use super::resolve::Resolver;
use super::sfc::{Sfc, SfcBlock, SfcBlockKind};
use crate::model::{Container, FileGraphBuilder};

/// A `framework`, `module` és (`.vue`-nál) `sfc` mező kitöltése a fájl szövegéből.
pub fn apply_file_meta(
    builder: &mut FileGraphBuilder,
    resolver: &Resolver,
    path: &Path,
    text: &str,
    is_vue: bool,
) {
    builder.framework(Some(if is_vue { "vue" } else { "js" }.to_string()));
    builder.imports_as_object();
    builder.module(Some(resolver.module_path(path)));
    if is_vue {
        builder.sfc(Some(sfc_summary(&Sfc::parse(text), text)));
    }
}

/// `{template:{line,endLine}|null, scriptSetup, styles}`.
fn sfc_summary(sfc: &Sfc, text: &str) -> serde_json::Value {
    let template = sfc
        .blocks
        .iter()
        .find(|b| b.kind == SfcBlockKind::Template && !is_ignored(text, b))
        .map(|t| serde_json::json!({ "line": t.start_line, "endLine": t.end_line }));
    let count = |kind| {
        sfc.blocks
            .iter()
            .filter(|b| b.kind == kind && !is_ignored(text, b))
            .count()
    };
    serde_json::json!({
        "template": template,
        "scriptSetup": count(SfcBlockKind::ScriptSetup) > 0,
        "styles": count(SfcBlockKind::Style),
    })
}

/// A `compiler-sfc` `ignoreEmpty` szabálya: az üres, `src` nélküli blokk nem számít.
fn is_ignored(text: &str, block: &SfcBlock) -> bool {
    let open = text[..block.start]
        .rfind('<')
        .map_or("", |i| &text[i..block.start]);
    text[block.start..block.end].trim().is_empty() && !open.contains(" src")
}

/// A top-level osztály-deklarációkat `type:'class'` containerként veszi fel.
pub struct ClassContainerPass;

impl JsPass for ClassContainerPass {
    fn run(&self, ctx: &PassContext<'_>, program: &Program<'_>, out: &mut JsAnalysis) {
        for stmt in &program.body {
            let (class, own_doc) = match stmt {
                Statement::ClassDeclaration(c) => (c.as_ref(), true),
                Statement::ExportDeclaration(e) => match &e.declaration {
                    Declaration::ClassDeclaration(c) => (c.as_ref(), false),
                    _ => continue,
                },
                Statement::ExportDefaultDeclaration(e) => match &e.declaration {
                    ExportDefaultDeclarationKind::ClassDeclaration(c) => (c.as_ref(), false),
                    _ => continue,
                },
                _ => continue,
            };
            let Some(id) = &class.id else { continue };
            let doc = own_doc
                .then(|| doc_before(program, ctx.source, class))
                .flatten();
            out.builder.container(Container {
                kind: "class".to_string(),
                name: id.name.to_string(),
                line: ctx.lines.line_of(class.span.start as usize),
                end_line: ctx.lines.line_of(class.span.end as usize),
                is_abstract: Some(false),
                is_final: Some(false),
                extends: Some(
                    class
                        .heritage
                        .iter()
                        .map(|h| squeeze(slice(ctx.source, h.expression.span()), 60))
                        .collect(),
                ),
                implements: Some(Vec::new()),
                traits: Some(Vec::new()),
                doc,
            });
        }
    }
}

fn slice(src: &str, span: oxc_span::Span) -> &str {
    src.get(span.start as usize..span.end as usize)
        .unwrap_or("")
}

/// Whitespace összevonása, ≤`max` karakterre vágva (`…` jelzéssel).
fn squeeze(text: &str, max: usize) -> String {
    let s = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.chars().count() > max {
        let head: String = s.chars().take(max - 1).collect();
        format!("{head}…")
    } else {
        s
    }
}

/// Az osztály elé tapadó utolsó `/** */` docblock szövege (≤300 karakter).
fn doc_before(program: &Program<'_>, src: &str, class: &Class<'_>) -> Option<String> {
    let comment = program
        .comments
        .iter()
        .filter(|c| c.attached_to == class.span.start && c.is_block())
        .rfind(|c| slice(src, c.content_span()).starts_with('*'))?;
    let text = slice(src, comment.content_span())
        .trim_start_matches('*')
        .split('\n')
        .map(|l| {
            let t = l.trim_start();
            let t = t
                .strip_prefix('*')
                .map_or(t, |r| r.strip_prefix(' ').unwrap_or(r));
            t.trim().to_string()
        })
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    (!text.is_empty()).then(|| text.chars().take(300).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn squeeze_collapses_and_truncates() {
        assert_eq!(squeeze("a  b\n c", 60), "a b c");
        assert_eq!(squeeze(&"x".repeat(70), 60).chars().count(), 60);
    }

    #[test]
    fn sfc_summary_counts_blocks() {
        let text = "<template>\n<p/>\n</template>\n<script setup>\nlet a\n</script>\n<style>a{}</style>\n<style>a{}</style>\n<style scoped>\n</style>\n";
        let v = sfc_summary(&Sfc::parse(text), text);
        assert_eq!(v["scriptSetup"], true);
        assert_eq!(v["styles"], 2);
        assert_eq!(v["template"]["line"], 1);
    }
}
