//! A hívás forrássora a hívó-találatok mellé (`L25 │ $this->resolver->getPlayables($id)`): az agentnek így nem kell
//! külön `--body` hívás ahhoz, hogy lássa, HOGYAN hívják a célt. Egy sor, összevont szóközökkel, ~100 karakterre vágva.

use std::cell::RefCell;
use std::collections::HashMap;

use crate::i18n::Lang;

/// Ennyi karakter után a sor `…`-ra végződik.
const MAX_CHARS: usize = 100;

/// Egy metódusban legfeljebb ennyi hívás-sor; a többi egy `… +N` sorban.
pub(crate) const MAX_PER_METHOD: usize = 5;

/// Fájlonként egyszer beolvasott forrássorok (a kimenet egy renderelésére; a nem olvasható fájl üres).
#[derive(Default)]
pub(crate) struct SourceLines {
    files: RefCell<HashMap<String, Option<Vec<String>>>>,
}

impl SourceLines {
    /// A `file` `line`-adik (1-től számolt) sora tömörítve; `None`, ha a fájl / sor nem olvasható vagy üres.
    pub(crate) fn get(&self, file: &str, line: u32) -> Option<String> {
        let mut files = self.files.borrow_mut();
        let lines = files.entry(file.to_owned()).or_insert_with(|| {
            std::fs::read(file).ok().map(|bytes| {
                String::from_utf8_lossy(&bytes)
                    .lines()
                    .map(str::to_owned)
                    .collect()
            })
        });
        let text = lines
            .as_ref()?
            .get(usize::try_from(line).ok()?.checked_sub(1)?)?;
        compact(text)
    }
}

/// Egy forrássor tömörítve: trim, a szóköz-sorozatok egy szóközre, `MAX_CHARS` fölött `…`. Üres sorra `None`.
pub(crate) fn compact(text: &str) -> Option<String> {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return None;
    }
    if collapsed.chars().count() <= MAX_CHARS {
        return Some(collapsed);
    }
    let mut cut: String = collapsed.chars().take(MAX_CHARS).collect();
    cut.truncate(cut.trim_end().len());
    cut.push('…');
    Some(cut)
}

/// A hívás helye a jelölésekkel: `L25`, `L25 via Resolver`, `L25 │ kód`, `L25 via Resolver │ kód`.
pub(crate) fn call_line(lang: Lang, line: u32, via: Option<&str>, code: Option<&str>) -> String {
    let mut out = format!("L{line}");
    if let Some(via) = via {
        out.push(' ');
        out.push_str(&ltf!(lang, "via {via}", "via {via}"));
    }
    if let Some(code) = code {
        out.push_str(" │ ");
        out.push_str(code);
    }
    out
}

/// A levágott hívás-sorok jelzése (`… +3 more`).
pub(crate) fn more(lang: Lang, count: usize) -> String {
    ltf!(lang, "… +{count} more", "… +{count} további")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_trims_collapses_and_truncates() {
        assert_eq!(
            compact("    $x =\t$this->run( 1,   2 );  ").as_deref(),
            Some("$x = $this->run( 1, 2 );")
        );
        assert_eq!(compact("   \t "), None);
        let long = format!("call({})", "a".repeat(150));
        let cut = compact(&long).unwrap();
        assert_eq!(cut.chars().count(), MAX_CHARS + 1);
        assert!(cut.ends_with('…'), "{cut}");
    }

    #[test]
    fn call_line_marks_via_and_code() {
        assert_eq!(call_line(Lang::En, 7, None, None), "L7");
        assert_eq!(
            call_line(Lang::En, 7, Some("Resolver"), Some("$r->get();")),
            "L7 via Resolver │ $r->get();"
        );
        assert_eq!(call_line(Lang::Hu, 7, None, Some("f();")), "L7 │ f();");
        assert_eq!(more(Lang::Hu, 2), "… +2 további");
    }

    #[test]
    fn source_lines_reads_the_file_once_and_tolerates_missing_ones() {
        let dir = std::env::temp_dir().join(format!(
            "callgraph-snippet-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a.php");
        std::fs::write(&file, "<?php\n\n    foo(  1 );\n").unwrap();
        let lines = SourceLines::default();
        let path = file.to_string_lossy();
        assert_eq!(lines.get(&path, 3).as_deref(), Some("foo( 1 );"));
        assert_eq!(lines.get(&path, 2), None);
        assert_eq!(lines.get(&path, 99), None);
        assert_eq!(lines.get(&path, 0), None);
        assert_eq!(lines.get("/no/such/file.php", 1), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
