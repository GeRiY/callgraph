//! Parser-réteg: a külső parser-crate (mago-syntax) elrejtése (Adapter minta).
//! Mago-specifikus típus csak ide és az `analysis::visitor` alá kerülhet.

mod mago_adapter;

pub use mago_adapter::MagoAdapter;

use mago_database::file::File;
use mago_span::{HasSpan, Span};
use mago_syntax::cst::Program;

/// Egy parse-olt fájl nézete: a CST, a forrás és a sor-segédek.
pub struct ParsedFile<'a> {
    pub program: &'a Program<'a>,
    file: &'a File,
    source: &'a [u8],
    /// (bájt-eltolás, üzenet) párok.
    errors: Vec<(u32, String)>,
}

impl<'a> ParsedFile<'a> {
    pub(crate) fn new(
        program: &'a Program<'a>,
        file: &'a File,
        source: &'a [u8],
        errors: Vec<(u32, String)>,
    ) -> Self {
        Self {
            program,
            file,
            source,
            errors,
        }
    }

    /// 1-alapú sorszám egy bájt-offsethez.
    pub fn line_of(&self, offset: u32) -> u32 {
        self.file.line_number(offset) + 1
    }

    /// Egy node kezdő és záró sora (1-alapú, a záró offset exkluzív).
    pub fn lines_of(&self, node: &impl HasSpan) -> (u32, u32) {
        let span = node.span();
        let end = span.end.offset.saturating_sub(1).max(span.start.offset);
        (self.line_of(span.start.offset), self.line_of(end))
    }

    /// A span forrásszövege.
    pub fn text(&self, span: Span) -> String {
        let (s, e) = (span.start.offset as usize, span.end.offset as usize);
        self.source
            .get(s..e)
            .map(|b| String::from_utf8_lossy(b).into_owned())
            .unwrap_or_default()
    }

    /// A forrás teljes sorai a két offset sorai között (mindkét sor egészében, `\n`-nel összefűzve,
    /// a sorvégi `\r` nélkül) — a PHP `file(…, FILE_IGNORE_NEW_LINES)` + `implode("\n")` megfelelője.
    pub fn full_lines(&self, start_offset: u32, end_offset: u32) -> String {
        let src = self.source;
        let s = (start_offset as usize).min(src.len());
        let e = (end_offset.saturating_sub(1).max(start_offset) as usize).min(src.len());
        let from = src[..s]
            .iter()
            .rposition(|&c| c == b'\n')
            .map_or(0, |i| i + 1);
        let to = src[e..]
            .iter()
            .position(|&c| c == b'\n')
            .map_or(src.len(), |i| e + i);
        String::from_utf8_lossy(&src[from..to])
            .split('\n')
            .map(|l| l.strip_suffix('\r').unwrap_or(l))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Szintaktikai hibák: (bájt-eltolás, üzenet) — a parser hibatűrő, a gráf ettől még elkészül.
    pub fn errors(&self) -> &[(u32, String)] {
        &self.errors
    }

    /// A szintaktikai hibák a gráfba: 1-alapú sor (a fájlvégi hiba a hiányos, utolsó tartalmas sorra) + üzenet.
    pub fn parse_errors(&self) -> Vec<crate::model::ParseError> {
        self.errors
            .iter()
            .map(|(offset, message)| {
                let at = crate::core::clamp_to_content(self.source, *offset as usize);
                let line = self.line_of(u32::try_from(at).unwrap_or(*offset));
                crate::model::ParseError::new(line, message)
            })
            .collect()
    }
}

/// PHP-parser kapu: a forrásból `ParsedFile`-t ad a feldolgozó closure-nek.
/// (Closure-os forma, mert a CST az arénához kötött élettartamú.)
pub trait PhpParser {
    fn parse<R>(&self, path: &str, source: &[u8], consume: impl FnOnce(&ParsedFile<'_>) -> R) -> R;
}
