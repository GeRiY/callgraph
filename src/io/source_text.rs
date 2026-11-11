//! Forrásfájl szövegként, kódolás-tűrően.

use std::path::Path;

/// A fájl szövege veszteséges UTF-8 dekódolással: a nem UTF-8 bájtok (pl. egy ISO-8859-2 fájl ékezetes
/// kommentje) U+FFFD-re cserélődnek, de a sorok és a sorszámok épek maradnak. A szigorú `read_to_string` az
/// egész fájlt üresnek mutatta volna, hibaüzenet nélkül.
pub fn read_source_lossy(path: &Path) -> std::io::Result<String> {
    Ok(String::from_utf8_lossy(&std::fs::read(path)?).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latin2_bytes_become_replacement_characters_and_lines_stay_whole() {
        let dir = std::env::temp_dir().join(format!("callgraph-latin2-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a.php");
        std::fs::write(&file, b"<?php\n// t\xf6bb v\xe1ltoz\xf3\n$x = 1;\n").unwrap();
        let text = read_source_lossy(&file).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3);
        assert!(lines[1].starts_with("// t") && lines[1].contains('\u{FFFD}'));
        assert_eq!(lines[2], "$x = 1;");
        assert!(read_source_lossy(&dir.join("missing.php")).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
