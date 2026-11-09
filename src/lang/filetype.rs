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

/// Ennyi bájtot néz meg a minifikált-próba.
const MINIFIED_SNIFF: usize = 8192;
/// Ennél hosszabb sor kell a minifikált-próbához …
const MINIFIED_LONGEST_LINE: usize = 1000;
/// … és ennél nagyobb átlagos sorhossz (a kézzel írt kód átlaga jóval kisebb, egy-egy hosszú sor mellett is).
const MINIFIED_AVERAGE_LINE: usize = 200;

/// Minifikált / bundlolt `.js` / `.mjs` / `.cjs` / `.css` (a `.min.` név nélkül is, pl. a Vite
/// `app-CBW7lKWe.js`-e): az első 8 KB-ban van 1000 karakternél hosszabb sor, és az átlagos sorhossz 200 fölött.
/// A biztonsági háló a `.gitignore` nélküli projektekhez; az elemző a bináris tartalomhoz hasonlóan kihagyja (a
/// beolvasott tartalomból dönt, külön olvasás nélkül, és a projekt-gráf megjegyzi).
pub fn looks_minified(path: &Path, bytes: &[u8]) -> bool {
    let script_or_style = extension_lower(path)
        .is_some_and(|ext| matches!(ext.as_str(), "js" | "mjs" | "cjs" | "css"));
    if !script_or_style {
        return false;
    }
    let head = &bytes[..bytes.len().min(MINIFIED_SNIFF)];
    if head.is_empty() {
        return false;
    }
    let lines: Vec<&[u8]> = head.split(|&b| b == b'\n').collect();
    let longest = lines.iter().map(|line| line.len()).max().unwrap_or(0);
    longest > MINIFIED_LONGEST_LINE && head.len() / lines.len() > MINIFIED_AVERAGE_LINE
}

/// A közvetlenül megadott, kihagyott tartalmú fájl hibája: minifikált kódnál `Generated`, különben `Binary` (az
/// elemző mindkettőt üres, `binary` jelölésű gráffal jelzi).
pub fn skipped_content_error(path: &Path) -> crate::error::CallgraphError {
    let head = std::fs::read(path).unwrap_or_default();
    if looks_minified(path, &head) {
        crate::error::CallgraphError::Generated(path.to_path_buf())
    } else {
        crate::error::CallgraphError::Binary(path.to_path_buf())
    }
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

    #[test]
    fn minified_is_long_lines_in_js_and_css_only() {
        let bundle = format!("var a={};\n", "x".repeat(3000));
        let bundled = |name: &str| looks_minified(Path::new(name), bundle.as_bytes());
        for name in ["public/build/app-CBW7lKWe.js", "x.MJS", "x.cjs", "app.css"] {
            assert!(bundled(name), "{name}");
        }
        // más kiterjesztés: nem
        assert!(!bundled("app.ts") && !bundled("App.vue"));
        // a kézzel írt kód egy hosszú sorral (pl. beágyazott adat) sem: az átlag kicsi
        let mut source = "const x = 1;\n".repeat(200);
        source.push_str(&format!("const data = '{}';\n", "a".repeat(1500)));
        let js = Path::new("src/app.js");
        assert!(!looks_minified(js, source.as_bytes()));
        // rövid sorok, üres fájl: nem
        assert!(!looks_minified(js, b"export const a = 1;\n"));
        assert!(!looks_minified(js, b""));
    }
}
