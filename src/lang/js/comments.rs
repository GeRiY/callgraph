//! A `comments` mező script-részre: TODO/FIXME/XXX/HACK vagy ≥15 karakteres, nem-docblock komment.

use std::sync::LazyLock;

use oxc_ast::ast::{CommentKind, Program};
use regex::Regex;

use super::analysis::{JsAnalysis, JsPass, PassContext};
use super::scope::text::{js_trim, truncate_utf16, utf16_len};
use super::scope::{MethodInfo, Scopes, top_active};
use crate::model::Comment;

static LINE_START: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^\s*\*?\s*").unwrap());
static TAG: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(TODO|FIXME|XXX|HACK)(?-u:\b)").unwrap());

/// A kommentek gyűjtése; a caller a legszűkebb tartalmazó metódus, különben `__top__`.
pub struct CommentsPass;

impl JsPass for CommentsPass {
    fn run(&self, ctx: &PassContext<'_>, program: &Program<'_>, out: &mut JsAnalysis) {
        if program.comments.is_empty() {
            return;
        }
        let scopes = Scopes::build(program, ctx);
        let mut methods: Vec<MethodInfo> = scopes.methods().to_vec();
        if top_active(program, ctx) {
            let first = ctx.lines.line_of(0);
            methods.push(MethodInfo {
                name: scopes.top_name().to_string(),
                container: scopes.base_container().to_string(),
                line: first,
                end_line: first + ctx.lines.line_count() - 1,
            });
        }
        for c in &program.comments {
            let Some(raw) = ctx.source.get(c.span.start as usize..c.span.end as usize) else {
                continue;
            };
            let value = if c.kind == CommentKind::Line {
                raw.strip_prefix("//").unwrap_or(raw)
            } else {
                raw.strip_prefix("/*")
                    .map(|r| r.strip_suffix("*/").unwrap_or(r))
                    .unwrap_or(raw)
            };
            if c.kind != CommentKind::Line && value.starts_with('*') {
                continue;
            }
            let spaced = LINE_START.replace_all(value, " ");
            let text = spaced.split_whitespace().collect::<Vec<_>>().join(" ");
            let text = js_trim(&text);
            let tag = TAG.captures(text).map(|m| m[1].to_string());
            if tag.is_none() && utf16_len(text) < 15 {
                continue;
            }
            let line = ctx.lines.line_of(c.span.start as usize);
            let owner = methods
                .iter()
                .filter(|m| m.line < line && m.end_line >= line)
                .min_by_key(|m| m.end_line - m.line);
            out.builder.comment(Comment {
                container: Some(
                    owner
                        .map_or(scopes.base_container(), |m| &m.container)
                        .to_string(),
                ),
                caller: Some(owner.map_or("__top__", |m| &m.name).to_string()),
                line,
                tag,
                text: truncate_utf16(text, 200).to_string(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::core::SourceFile;
    use crate::lang::Language;
    use crate::lang::js::JsFrontend;

    #[test]
    fn keeps_long_and_tagged_comments_but_not_docblocks() {
        let src = "/** doc block */\n// short\nfunction f() {\n  // TODO: x\n  /* a longer explanation here */\n}\n";
        let f = SourceFile::new("/p/m.js", src.as_bytes().to_vec(), None);
        let g = JsFrontend::with_defaults().analyze(&f).unwrap().graph;
        let got: Vec<_> = g
            .comments
            .iter()
            .map(|c| (c.caller.clone().unwrap(), c.tag.clone()))
            .collect();
        assert_eq!(got, [("f".into(), Some("TODO".into())), ("f".into(), None)]);
    }
}
