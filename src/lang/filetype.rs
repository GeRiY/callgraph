//! Melyik fájlt dolgozza fel a callgraph: a kiterjesztés (kis-/nagybetűtől függetlenül), a generált fájlok
//! (lock-fájl, minifikált kód) és a bináris tartalom szabályai. A bejáró, a frissítés, az MCP-figyelő, a hook és
//! a közvetlenül megadott fájl ugyanezt használja.

use std::path::Path;

/// A fájl kiterjesztése kisbetűvel (`B.PHP` → `php`); kiterjesztés nélkül vagy nem UTF-8 névnél `None`.
pub fn extension_lower(path: &Path) -> Option<String> {
    Some(path.extension()?.to_str()?.to_ascii_lowercase())
}

/// Generált, nem kézzel írt fájl, amelyet a callgraph a támogatott kiterjesztése ellenére kihagy: a
/// `package-lock.json` és a minifikált `*.min.js` / `*.min.css` (kis-/nagybetűtől függetlenül).
pub fn is_generated(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let name = name.to_ascii_lowercase();
    name == "package-lock.json" || name.ends_with(".min.js") || name.ends_with(".min.css")
}

/// Ennyi bájtot néz meg a bináris-próba (mint a git).
const BINARY_SNIFF: usize = 8000;

/// Bináris tartalom: NUL bájt az első 8000 bájtban (a git próbája). Forráskódban NUL nem fordul elő, egy kép,
/// archívum vagy lefordított fájl elején szinte mindig van.
pub fn looks_binary(bytes: &[u8]) -> bool {
    bytes[..bytes.len().min(BINARY_SNIFF)].contains(&0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_is_lowercased() {
        assert_eq!(
            extension_lower(Path::new("src/B.PHP")).as_deref(),
            Some("php")
        );
        assert_eq!(
            extension_lower(Path::new("App.Vue")).as_deref(),
            Some("vue")
        );
        assert_eq!(
            extension_lower(Path::new("a.php.bak")).as_deref(),
            Some("bak")
        );
        assert_eq!(extension_lower(Path::new("Makefile")), None);
        assert_eq!(extension_lower(Path::new(".hidden")), None);
    }

    #[test]
    fn generated_files_are_recognised_case_insensitively() {
        for name in [
            "package-lock.json",
            "web/PACKAGE-LOCK.JSON",
            "dist/app.min.js",
            "dist/App.Min.JS",
            "css/site.min.css",
        ] {
            assert!(is_generated(Path::new(name)), "{name}");
        }
        for name in [
            "package.json",
            "composer.json",
            "src/admin.js",
            "src/minimal.js",
            "src/min.js",
            "css/site.css",
            "src/app.min.ts",
        ] {
            assert!(!is_generated(Path::new(name)), "{name}");
        }
    }

    #[test]
    fn binary_is_a_nul_byte_near_the_start() {
        assert!(looks_binary(b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR"));
        assert!(!looks_binary(b"<?php echo 'hello';"));
        assert!(!looks_binary("<?php echo 'árvíztűrő';".as_bytes()));
        assert!(!looks_binary(b""));
        // a NUL a próbaablakon túl: szövegnek számít
        let mut late = vec![b'a'; BINARY_SNIFF];
        late.push(0);
        assert!(!looks_binary(&late));
    }
}
