//! `comments`: a sima (nem doc-) kommentek közül a jelzettek (`TODO`, `FIXME`, `XXX`, `HACK`) és az
//! informatívak (≥15 karakter); a `caller` a legszűkebb tartalmazó függvény, különben `__top__`.

use super::{FileVisitor, Scopes};
use crate::lang::rs::comment_scan::scan;
use crate::lang::rs::context::RsContext;
use crate::lang::rs::parser::{ParsedFile, shorten};
use crate::model::{Comment, FileGraphBuilder};

const TAGS: &[&str] = &["FIXME", "TODO", "XXX", "HACK"];

/// A `comments` mezőt tölti.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct CommentsVisitor;

impl FileVisitor for CommentsVisitor {
    fn visit(
        &self,
        parsed: &ParsedFile<'_>,
        scopes: &Scopes<'_>,
        _ctx: &mut RsContext,
        builder: &mut FileGraphBuilder,
    ) {
        for raw in scan(parsed.source()) {
            let Some(comment) = keep(&raw.text) else {
                continue;
            };
            let within = |start: u32, end: u32| start <= raw.line && raw.line <= end;
            let method = scopes
                .fns
                .iter()
                .filter(|f| within(f.start, f.end))
                .min_by_key(|f| f.end - f.start);
            let (container, caller) = match method {
                Some(f) => (Some(f.container.clone()), f.name.clone()),
                None => {
                    let container = scopes
                        .containers
                        .iter()
                        .filter(|c| within(c.start, c.end))
                        .min_by_key(|c| c.end - c.start)
                        .map(|c| c.name.clone());
                    (container, "__top__".to_owned())
                }
            };
            builder.comment(Comment {
                container,
                caller: Some(caller),
                line: raw.line,
                tag: comment.0,
                text: comment.1,
            });
        }
    }
}

/// A megtartandó komment: (tag, szöveg ≤150 karakter); a rövid, jelöletlen komment kimarad.
fn keep(text: &str) -> Option<(Option<String>, String)> {
    if text.is_empty() {
        return None;
    }
    let upper = text.to_ascii_uppercase();
    let tag = TAGS
        .iter()
        .find(|tag| {
            upper
                .split(|c: char| !c.is_ascii_alphanumeric())
                .any(|word| word == **tag)
        })
        .map(|tag| (*tag).to_owned());
    (tag.is_some() || text.chars().count() >= 15).then(|| (tag, shorten(text, 150)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_tagged_or_long_comments() {
        assert_eq!(
            keep("TODO: x"),
            Some((Some("TODO".into()), "TODO: x".into()))
        );
        assert_eq!(keep("short"), None);
        assert_eq!(keep("this one is long enough").map(|c| c.0), Some(None));
    }
}
