//! oxc-parsolás a fájltípus szerinti `SourceType`-pal.

use std::path::Path;

use oxc_allocator::Allocator;
use oxc_ast::ast::Program;
use oxc_parser::Parser;
use oxc_span::SourceType;

/// Egy parsolt program és a (nem végzetes) szintaktikai hibák szövege.
pub struct ParsedProgram<'a> {
    pub program: Program<'a>,
    pub errors: Vec<String>,
}

/// A `SourceType` a fájlkiterjesztésből; `.vue`-nál a script blokk `lang` attribútuma dönt.
pub fn source_type_for(path: &Path, vue_lang: Option<&str>) -> SourceType {
    if path.extension().and_then(|e| e.to_str()) == Some("vue") {
        return match vue_lang {
            Some("ts") => SourceType::ts(),
            Some("tsx") => SourceType::tsx(),
            _ => SourceType::jsx(),
        };
    }
    SourceType::from_path(path).unwrap_or_else(|_| SourceType::jsx())
}

/// Parsolja a forrást; a program az `allocator`-ban él.
pub fn parse_program<'a>(
    allocator: &'a Allocator,
    source: &'a str,
    source_type: SourceType,
) -> ParsedProgram<'a> {
    let ret = Parser::new(allocator, source, source_type).parse();
    let errors = ret.diagnostics.iter().map(|e| e.to_string()).collect();
    ParsedProgram {
        program: ret.program,
        errors,
    }
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
}
