//! Szintaktikai hibák: a parser saját diagnosztikája, hogy a hiányos vázlat ne tűnjön teljesnek.

use serde::{Deserialize, Serialize};

/// Egy szintaktikai hiba a fájlban: az 1-alapú sor és a parser rövid (egysoros) üzenete.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParseError {
    pub line: u32,
    pub message: String,
}

/// Egy fájlon belül legfeljebb ennyi hiba kerül a gráfba (a többi csak zaj: az első után a parser gyakran
/// láncban jelez).
pub const MAX_PARSE_ERRORS: usize = 5;

/// Az üzenet hossz-korlátja (karakterben).
const MAX_MESSAGE: usize = 120;

impl ParseError {
    /// Új hiba: az üzenetből csak az első nem üres sor marad, legfeljebb ~120 karakter.
    pub fn new(line: u32, message: &str) -> Self {
        let first = message
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .unwrap_or("syntax error");
        let message = if first.chars().count() > MAX_MESSAGE {
            let cut: String = first.chars().take(MAX_MESSAGE - 1).collect();
            format!("{}…", cut.trim_end())
        } else {
            first.to_owned()
        };
        Self {
            line: line.max(1),
            message,
        }
    }

    /// Hibalista lezárása: sor szerint rendezve (stabilan), soronként csak az első (a parser ugyanarra a sorra
    /// gyakran láncban jelez; az első a kiváltó ok), legfeljebb `MAX_PARSE_ERRORS` darab.
    pub fn finish(mut errors: Vec<ParseError>) -> Vec<ParseError> {
        errors.sort_by_key(|e| e.line);
        errors.dedup_by_key(|e| e.line);
        errors.truncate(MAX_PARSE_ERRORS);
        errors
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_is_first_line_and_capped() {
        let e = ParseError::new(0, "\n  Unexpected token\n  at foo\n");
        assert_eq!(e, ParseError::new(1, "Unexpected token"));
        let long = "x".repeat(300);
        assert!(ParseError::new(2, &long).message.chars().count() <= 120);
    }

    #[test]
    fn finish_sorts_dedups_and_caps() {
        let errors = (0..8).rev().map(|l| ParseError::new(l, "e")).collect();
        let done = ParseError::finish(errors);
        assert_eq!(done.len(), MAX_PARSE_ERRORS);
        assert_eq!(done[0].line, 1);
        let same_line = vec![ParseError::new(4, "first"), ParseError::new(4, "second")];
        assert_eq!(
            ParseError::finish(same_line),
            vec![ParseError::new(4, "first")]
        );
    }
}
