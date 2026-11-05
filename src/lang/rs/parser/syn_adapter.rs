//! `RustParser` megvalósítás a `syn`-re (`proc-macro2` `span-locations` a sorszámokhoz).

use super::{ParseError, ParsedFile, RustParser};

/// A `syn::parse_file` adaptere. A `proc-macro2` szálanként gyűjti a forrás-térképet; a closure után
/// törli (`invalidate_current_thread_spans`), így sok fájlnál sem nő a memória.
#[derive(Debug, Default, Clone, Copy)]
pub struct SynAdapter;

impl RustParser for SynAdapter {
    fn parse<R>(
        &self,
        source: &str,
        consume: impl FnOnce(std::result::Result<&ParsedFile<'_>, ParseError>) -> R,
    ) -> R {
        let result = match syn::parse_file(source) {
            Ok(file) => consume(Ok(&ParsedFile::new(&file, source))),
            Err(error) => consume(Err(ParseError {
                line: error.span().start().line as u32,
                message: error.to_string(),
            })),
        };
        proc_macro2::extra::invalidate_current_thread_spans();
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_reports_lines() {
        let lines = SynAdapter.parse("// x\nfn a() {\n}\n", |parsed| {
            let parsed = parsed.unwrap();
            parsed.lines(&parsed.file.items[0])
        });
        assert_eq!(lines, (2, 3));
    }

    #[test]
    fn syntax_error_has_line() {
        let error = SynAdapter.parse("fn a() {}\nfn b( {}\n", |parsed| parsed.err().unwrap());
        assert_eq!(error.line, 2);
    }

    #[test]
    fn text_is_exact_source_slice() {
        let text = SynAdapter.parse("fn a(x: Vec<u8>) {}", |parsed| {
            let parsed = parsed.unwrap();
            let syn::Item::Fn(f) = &parsed.file.items[0] else {
                unreachable!()
            };
            parsed.text(&f.sig.inputs)
        });
        assert_eq!(text, "x: Vec<u8>");
    }
}
