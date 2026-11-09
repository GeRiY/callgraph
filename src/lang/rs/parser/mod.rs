//! Parser-réteg: a külső parser-crate (`syn` + `proc-macro2`) elrejtése (Adapter minta).
//! Syn-specifikus típus csak ide és a `lang::rs::visitor` alá kerülhet.

mod syn_adapter;

pub use syn_adapter::SynAdapter;

use proc_macro2::Span;
use syn::spanned::Spanned;

/// Egy parse-olt Rust-fájl nézete: a szintaxisfa, a forrás és a sor-segédek.
#[derive(Clone, Copy)]
pub struct ParsedFile<'a> {
    pub(crate) file: &'a syn::File,
    source: &'a str,
    /// A teljes fájl sorainak száma (előtag-elemzésnél több, mint a `source` sorai).
    lines: u32,
}

impl<'a> ParsedFile<'a> {
    pub(crate) fn new(file: &'a syn::File, source: &'a str) -> Self {
        Self {
            file,
            source,
            lines: line_count_of(source),
        }
    }

    /// A fájl-szintű container záró sora a teljes fájlé marad, ha csak egy előtagot elemeztünk.
    pub(crate) fn with_line_count(self, lines: u32) -> Self {
        Self { lines, ..self }
    }

    /// A teljes forrásszöveg (a kommentek szkenneléséhez).
    pub fn source(&self) -> &'a str {
        self.source
    }

    /// A forrás sorainak száma (a fájl-szintű container záró sora).
    pub(crate) fn line_count(&self) -> u32 {
        self.lines
    }

    /// A span kezdő sora (1-alapú).
    pub(crate) fn line(&self, span: Span) -> u32 {
        span.start().line as u32
    }

    /// A span záró sora (1-alapú).
    pub(crate) fn end_line(&self, span: Span) -> u32 {
        span.end().line as u32
    }

    /// Egy node kezdő és záró sora.
    pub(crate) fn lines(&self, node: &impl Spanned) -> (u32, u32) {
        let span = node.span();
        (self.line(span), self.end_line(span))
    }

    /// A node pontos forrásszövege (a `span-locations` bájt-tartományából).
    pub(crate) fn text(&self, node: &impl Spanned) -> String {
        self.source
            .get(node.span().byte_range())
            .unwrap_or_default()
            .to_owned()
    }

    /// A node forrásszövege egy sorba vonva, `max` karakterre vágva (`…`-tal).
    pub(crate) fn short(&self, node: &impl Spanned, max: usize) -> String {
        shorten(&squash(&self.text(node)), max)
    }
}

/// A szöveg sorainak száma (legalább 1).
pub(crate) fn line_count_of(text: &str) -> u32 {
    text.lines().count().max(1) as u32
}

/// Whitespace-sorozatok egy szóközre cserélése.
pub(crate) fn squash(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `max` karakterre vágás `…` jellel.
pub(crate) fn shorten(text: &str, max: usize) -> String {
    if text.chars().count() > max {
        let mut cut: String = text.chars().take(max).collect();
        cut.push('…');
        cut
    } else {
        text.to_owned()
    }
}

/// Szintaktikai hiba helye és szövege.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub line: u32,
    pub message: String,
}

/// Rust-parser kapu: a forrásból `ParsedFile`-t (vagy hibát) ad a feldolgozó closure-nek.
/// (Closure-os forma, mert a span-ek csak a parse-olás szálán és a closure ideje alatt érvényesek.)
pub trait RustParser {
    fn parse<R>(
        &self,
        source: &str,
        consume: impl FnOnce(std::result::Result<&ParsedFile<'_>, ParseError>) -> R,
    ) -> R;

    /// A forrás első `len` bájtjának elemzése (végzetes szintaktikai hibánál az ép előtag); a sorszámok
    /// változatlanok, a fájl-container a teljes fájl sorait kapja.
    fn parse_prefix<R>(
        &self,
        source: &str,
        len: usize,
        consume: impl FnOnce(std::result::Result<&ParsedFile<'_>, ParseError>) -> R,
    ) -> R {
        let lines = line_count_of(source);
        self.parse(&source[..len], |parsed| match parsed {
            Ok(parsed) => consume(Ok(&parsed.with_line_count(lines))),
            Err(error) => consume(Err(error)),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shorten_and_squash() {
        assert_eq!(squash("a\n   b\tc"), "a b c");
        assert_eq!(shorten("abcdef", 3), "abc…");
        assert_eq!(shorten("abc", 3), "abc");
    }
}
