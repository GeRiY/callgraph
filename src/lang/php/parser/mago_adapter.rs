//! `PhpParser` megvalósítás a mago-syntax-ra (R2 döntés, `spike/DONTES.md`).

use std::borrow::Cow;

use mago_allocator::prelude::LocalArena;
use mago_database::file::{File, FileType};
use mago_span::HasSpan;

use super::{ParsedFile, PhpParser};

/// A mago-syntax parser adaptere; fájlonként friss aréna.
#[derive(Debug, Default, Clone, Copy)]
pub struct MagoAdapter;

impl PhpParser for MagoAdapter {
    fn parse<R>(&self, path: &str, source: &[u8], consume: impl FnOnce(&ParsedFile<'_>) -> R) -> R {
        let arena = LocalArena::new();
        let file = File::new(
            Cow::Owned(path.as_bytes().to_vec()),
            FileType::Host,
            None,
            Cow::Owned(source.to_vec()),
        );
        let program = mago_syntax::parser::parse_file(&arena, &file);
        let errors = program
            .errors
            .iter()
            .map(|e| (e.span().start.offset, e.to_string()))
            .collect();
        consume(&ParsedFile::new(program, &file, source, errors))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_source_without_errors() {
        let n = MagoAdapter.parse("a.php", b"<?php\nclass A {}\n", |p| p.errors().len());
        assert_eq!(n, 0);
    }

    #[test]
    fn reports_syntax_errors() {
        let n = MagoAdapter.parse("a.php", b"<?php\nfunction f( { \n", |p| p.errors().len());
        assert!(n > 0);
    }

    #[test]
    fn syntax_error_carries_its_line() {
        let src = b"<?php\nclass K {\n  public function ok() { return 1; }\n  public function broken( { return $x + ; }\n  public function after() { return 2; }\n}\n";
        let errors = MagoAdapter.parse("k.php", src, |p| p.parse_errors());
        assert_eq!(errors.first().map(|e| e.line), Some(4), "{errors:?}");
    }
}
