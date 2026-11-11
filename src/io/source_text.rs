//! Forrásfájl szövegként, kódolás-tűrően.

use std::borrow::Cow;
use std::path::Path;

use super::legacy_tables::{CP1250, CP1252, LATIN2};

/// A fájl szövege: érvényes UTF-8 változatlanul, egyébként felismert egybájtos kódolásból (lásd `decode_source`)
/// dekódolva. A szigorú `read_to_string` az egész fájlt üresnek mutatta volna, hibaüzenet nélkül.
pub fn read_source_lossy(path: &Path) -> std::io::Result<String> {
    Ok(decode_source(&std::fs::read(path)?).into_owned())
}

/// A bájtok szövege. Érvényes UTF-8 érintetlen marad. Egyébként a windows-1252, windows-1250 és ISO-8859-2
/// jelöltek közül a legjobb pontszámút használja (a magyar ékezetes betűk +2, egyéb betű +1, írásjel -2,
/// vezérlőjel -4); ha egyik sem pozitív (szemét), veszteséges UTF-8 (U+FFFD). Mind egybájtos és az ASCII
/// (így a sortörés is) változatlan, ezért a sorok és a sorszámok épek maradnak.
pub fn decode_source(bytes: &[u8]) -> Cow<'_, str> {
    if let Ok(text) = std::str::from_utf8(bytes) {
        return Cow::Borrowed(text);
    }
    let mut best: Option<(i64, &[char; 128])> = None;
    for table in [&CP1252, &CP1250, &LATIN2] {
        let score = score(bytes, table);
        if score > 0 && best.is_none_or(|(top, _)| score > top) {
            best = Some((score, table));
        }
    }
    match best {
        Some((_, table)) => Cow::Owned(
            bytes
                .iter()
                .map(|&b| if b < 0x80 { char::from(b) } else { table[usize::from(b - 0x80)] })
                .collect(),
        ),
        None => String::from_utf8_lossy(bytes),
    }
}

/// A jelölt pontszáma: csak a 0x80 feletti bájtok számítanak.
fn score(bytes: &[u8], table: &[char; 128]) -> i64 {
    const HUNGARIAN: &str = "áéíóöőúüűÁÉÍÓÖŐÚÜŰ";
    bytes
        .iter()
        .filter(|&&b| b >= 0x80)
        .map(|&b| {
            let c = table[usize::from(b - 0x80)];
            if HUNGARIAN.contains(c) {
                2
            } else if c.is_alphabetic() {
                1
            } else if c.is_control() {
                -4
            } else {
                -2
            }
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latin2_bytes_are_decoded_and_lines_stay_whole() {
        let dir = std::env::temp_dir().join(format!("callgraph-latin2-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a.php");
        std::fs::write(&file, b"<?php\n// t\xf6bb v\xe1ltoz\xf3 \xf5\xfb\n$x = 1;\n").unwrap();
        let text = read_source_lossy(&file).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[1], "// többb változó őű");
        assert_eq!(lines[2], "$x = 1;");
        assert!(read_source_lossy(&dir.join("missing.php")).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn cp1250_and_latin1_samples_are_readable() {
        // cp1250: ą (0xB9), ż (0xBF), ś (0x9C), ź (0x9F) csak itt betű
        assert_eq!(decode_source(b"\xb9\xbf\x9c\x9f"), "ążśź");
        // latin1/cp1252: é, ü, ñ
        assert_eq!(decode_source(b"caf\xe9 \xfcber ma\xf1ana"), "café über mañana");
        // ISO-8859-2: Ą (0xA1) és ł (0xB3) csak itt betű
        assert_eq!(decode_source(b"\xa1 \xb3\xf3d\xbc"), "Ą łódź");
    }

    #[test]
    fn valid_utf8_is_untouched_and_garbage_falls_back_to_lossy() {
        assert!(matches!(decode_source("árvíztűrő".as_bytes()), Cow::Borrowed(_)));
        let garbage = [b'a', 0x81, 0x8d, 0x8f, 0x90, b'\n', 0x9d];
        let text = decode_source(&garbage);
        assert!(text.contains('\u{FFFD}') && text.lines().count() == 2);
    }
}
