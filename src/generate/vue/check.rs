//! A kész szöveg ellenőrzése írás előtt, szakaszonként: a script / composable oxc-cal (`lang::js`), a template
//! kiegyensúlyozott tagekkel, a stílus a CSS / SCSS parserrel (raffia). Az első hiba a szakasz nevét és a sort adja.

use std::path::Path;

use oxc_allocator::Allocator;
use raffia::ast::Stylesheet;
use raffia::{Parser, Syntax};

use super::template::check_tags;
use crate::lang::js::{Sfc, SfcBlockKind, parse_program, source_type_for};

/// Az első hiba helye és leírása.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    /// `script` / `template` / `style` / a fájl neve.
    pub section: String,
    /// 1-alapú sor a teljes fájlban.
    pub line: usize,
    pub message: String,
}

/// A `offset`-ig álló sortörések száma (a szakaszon belüli 0-alapú sor).
fn newlines(text: &str, offset: usize) -> usize {
    text.as_bytes()
        .iter()
        .take(offset)
        .filter(|&&b| b == b'\n')
        .count()
}

/// Az első szintaktikai hiba a JS / TS szövegben: (0-alapú sor, üzenet).
fn first_js_error(path: &str, lang: Option<&str>, src: &str) -> Option<(usize, String)> {
    let allocator = Allocator::default();
    let source_type = source_type_for(Path::new(path), lang);
    let parsed = parse_program(&allocator, src, source_type);
    parsed
        .errors
        .first()
        .map(|e| (newlines(src, e.offset), e.message.clone()))
}

/// Egy `.ts` / `.js` fájl (composable) ellenőrzése.
pub fn check_script_file(rel: &str, text: &str) -> Result<(), Problem> {
    match first_js_error(rel, None, text) {
        Some((line, message)) => Err(Problem {
            section: rel.to_owned(),
            line: line + 1,
            message,
        }),
        None => Ok(()),
    }
}

/// Egy `.vue` fájl ellenőrzése: minden script / template / style blokk.
pub fn check_vue(text: &str) -> Result<(), Problem> {
    let sfc = Sfc::parse(text);
    for block in &sfc.blocks {
        let src = text.get(block.start..block.end).unwrap_or("");
        let base = block.start_line as usize;
        let found = match block.kind {
            SfcBlockKind::Script | SfcBlockKind::ScriptSetup => {
                first_js_error("x.vue", block.lang.as_deref(), src)
                    .map(|(line, message)| ("script", line, message))
            }
            SfcBlockKind::Template => check_tags(src)
                .err()
                .map(|(offset, message)| ("template", newlines(src, offset), message)),
            SfcBlockKind::Style => {
                let syntax = match block.lang.as_deref() {
                    None | Some("css") => Some(Syntax::Css),
                    Some("scss") => Some(Syntax::Scss),
                    _ => None,
                };
                syntax.and_then(|syntax| {
                    Parser::new(src, syntax)
                        .parse::<Stylesheet>()
                        .err()
                        .map(|e| {
                            (
                                "style",
                                newlines(src, e.span.start),
                                format!("{:?}", e.kind),
                            )
                        })
                })
            }
        };
        if let Some((section, line, message)) = found {
            return Err(Problem {
                section: section.to_owned(),
                line: base + line,
                message,
            });
        }
    }
    Ok(())
}
