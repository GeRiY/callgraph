//! oxc-parsolás a fájltípus szerinti `SourceType`-pal; végzetes szintaktikai hibánál az ép előtag újraparsolása.

use std::path::Path;

use oxc_allocator::Allocator;
use oxc_ast::ast::Program;
use oxc_parser::Parser;
use oxc_span::SourceType;

/// Egy szintaktikai hiba: a (blokkon belüli) bájt-eltolás és a parser rövid üzenete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsSyntaxError {
    pub offset: usize,
    pub message: String,
}

/// Egy parsolt program és a szintaktikai hibák.
pub struct ParsedProgram<'a> {
    pub program: Program<'a>,
    pub errors: Vec<JsSyntaxError>,
    /// A parser feladta (az AST üres, pedig volt hiba): a teljes forrásból semmi nem használható.
    pub fatal: bool,
    /// A ténylegesen elemzett forrás: az eredeti, vagy (`parse_recovering`) végzetes hibánál egy előtagja.
    pub source: &'a str,
}

/// A `SourceType` a fájlkiterjesztésből; `.vue`-nál a script blokk `lang` attribútuma dönt.
pub fn source_type_for(path: &Path, vue_lang: Option<&str>) -> SourceType {
    let ext = crate::lang::filetype::extension_lower(path);
    if ext.as_deref() == Some("vue") {
        return match vue_lang {
            Some("ts") => SourceType::ts(),
            Some("tsx") => SourceType::tsx(),
            _ => SourceType::jsx(),
        };
    }
    // nagybetűs kiterjesztésnél (`App.TS`) a kisbetűs alak dönt
    let path = ext.map_or_else(|| path.to_path_buf(), |ext| path.with_extension(ext));
    SourceType::from_path(&path).unwrap_or_else(|_| SourceType::jsx())
}

/// Parsolja a forrást; a program az `allocator`-ban él.
pub fn parse_program<'a>(
    allocator: &'a Allocator,
    source: &'a str,
    source_type: SourceType,
) -> ParsedProgram<'a> {
    let ret = Parser::new(allocator, source, source_type).parse();
    let errors: Vec<JsSyntaxError> = ret
        .diagnostics
        .iter()
        .map(|e| {
            let label = e
                .labels
                .iter()
                .find(|l| l.primary())
                .or_else(|| e.labels.first());
            JsSyntaxError {
                offset: label.map_or(0, |l| l.offset() as usize),
                message: e.message.to_string(),
            }
        })
        .collect();
    let fatal = ret.fatal_error || (ret.program.body.is_empty() && !errors.is_empty());
    ParsedProgram {
        program: ret.program,
        errors,
        fatal,
        source,
    }
}

/// Mint a `parse_program`, de végzetes hibánál a forrás egyre rövidebb előtagját parsolja újra (a legkorábbi hiba
/// sorának elejéig, utána korábbi felső szintű határokig — `core::prefix_cuts`), hogy a hiba előtti, ép rész
/// megmaradjon. A hibák mindig az eredeti, teljes parsolásból jönnek; előtag, ezért a sorszámok nem változnak.
pub fn parse_recovering<'a>(
    allocator: &'a Allocator,
    source: &'a str,
    source_type: SourceType,
) -> ParsedProgram<'a> {
    let parsed = parse_program(allocator, source, source_type);
    if !parsed.fatal {
        return parsed;
    }
    let first = parsed.errors.iter().map(|e| e.offset).min().unwrap_or(0);
    for cut in crate::core::prefix_cuts(source, first) {
        let prefix = parse_program(allocator, &source[..cut], source_type);
        if !prefix.fatal {
            return ParsedProgram {
                program: prefix.program,
                errors: parsed.errors,
                fatal: true,
                source: prefix.source,
            };
        }
    }
    parsed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_source_type_by_extension_and_lang() {
        assert!(source_type_for(Path::new("a.ts"), None).is_typescript());
        assert!(source_type_for(Path::new("a.tsx"), None).is_jsx());
        assert!(source_type_for(Path::new("a.mjs"), None).is_module());
        assert!(source_type_for(Path::new("a.vue"), Some("ts")).is_typescript());
        assert!(!source_type_for(Path::new("a.vue"), None).is_typescript());
    }

    #[test]
    fn collects_syntax_errors() {
        let alloc = Allocator::default();
        let p = parse_program(&alloc, "const = ;", SourceType::jsx());
        assert!(!p.errors.is_empty());
    }

    #[test]
    fn fatal_error_recovers_the_prefix() {
        let alloc = Allocator::default();
        let src = "export function a() { return 1 }\nexport function b() { return 2 }\nexport function c( {\n";
        for source_type in [SourceType::mjs(), SourceType::ts()] {
            let p = parse_recovering(&alloc, src, source_type);
            assert!(p.fatal && !p.errors.is_empty());
            assert_eq!(p.program.body.len(), 2, "{:?}", p.errors);
            assert!(p.source.len() < src.len());
        }
    }

    #[test]
    fn valid_source_is_not_fatal() {
        let alloc = Allocator::default();
        let p = parse_recovering(&alloc, "function a() {}\n", SourceType::mjs());
        assert!(!p.fatal && p.errors.is_empty() && p.source.len() == 16);
    }
}
