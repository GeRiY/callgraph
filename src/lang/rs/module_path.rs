//! A fájl modul-útvonala a fájl útjából (`src/cli/args.rs` → `crate::cli::args`), és a relatív
//! (`self::` / `super::`) útvonalak kanonizálása.

use std::path::Path;

/// A fájl modulja: a `crate` utáni szegmensek (`["cli", "args"]`; `lib.rs` / `main.rs`: üres).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ModulePath {
    segments: Vec<String>,
}

impl ModulePath {
    /// A fájl útjából: az utolsó `src` mappa utáni rész; `mod.rs` → a mappa neve, `lib.rs` / `main.rs` → gyökér.
    /// `src` nélkül (pl. `tests/x.rs`) a fájlnév az egyetlen szegmens.
    pub(crate) fn from_file(path: &str) -> Self {
        let parts: Vec<String> = Path::new(path)
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        // `src` nélkül a fájl (`mod.rs`-nél a mappája) az egyetlen szegmens.
        let from = parts
            .iter()
            .rposition(|p| p == "src")
            .map_or_else(|| parts.len().saturating_sub(2), |i| i + 1);
        let mut segments = parts[from.min(parts.len())..].to_vec();
        if segments.len() == 2 && !parts.iter().any(|p| p == "src") && segments[1] != "mod.rs" {
            segments.remove(0);
        }
        if let Some(last) = segments.pop() {
            let stem = last.strip_suffix(".rs").unwrap_or(&last).to_owned();
            match stem.as_str() {
                "mod" => {}
                "lib" | "main" if segments.is_empty() => {}
                _ => segments.push(stem),
            }
        }
        Self { segments }
    }

    /// A fájl-szintű container neve: a modul utolsó szegmense, a crate-gyökérnél `crate`.
    pub(crate) fn container_name(&self) -> String {
        self.segments
            .last()
            .cloned()
            .unwrap_or_else(|| "crate".to_owned())
    }

    /// A teljes út `crate`-tel kezdve, a beágyazott (inline) modulokkal kiegészítve.
    pub(crate) fn absolute(&self, inline: &[String]) -> Vec<String> {
        std::iter::once("crate".to_owned())
            .chain(self.segments.iter().cloned())
            .chain(inline.iter().cloned())
            .collect()
    }

    /// Egy leírt útvonal kanonizálása: `self::` / `super::` → `crate::…`; a többi változatlan.
    pub(crate) fn canonical(&self, inline: &[String], written: &[String]) -> Vec<String> {
        let mut base = self.absolute(inline);
        let mut rest = written;
        match written.first().map(String::as_str) {
            Some("self") => rest = &written[1..],
            Some("super") => {
                while rest.first().map(String::as_str) == Some("super") {
                    if base.len() > 1 {
                        base.pop();
                    }
                    rest = &rest[1..];
                }
            }
            _ => return written.to_vec(),
        }
        base.extend(rest.iter().cloned());
        base
    }
}

/// Egy kanonikus út rövid, a fájlok container-neveivel egyező alakja: az utolsó két szegmens
/// (`crate::cli::args::parse` → `args::parse`, `crate::model::Method::new` → `Method::new`).
pub(crate) fn short_key(path: &[String]) -> String {
    let from = path.len().saturating_sub(2);
    path[from..].join("::")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segs(s: &str) -> Vec<String> {
        s.split("::").map(str::to_owned).collect()
    }

    #[test]
    fn module_from_file_path() {
        let m = ModulePath::from_file("/x/src/cli/args.rs");
        assert_eq!(m.absolute(&[]), segs("crate::cli::args"));
        assert_eq!(m.container_name(), "args");
        assert_eq!(
            ModulePath::from_file("src/lang/php/mod.rs").container_name(),
            "php"
        );
        assert_eq!(
            ModulePath::from_file("src/lib.rs").container_name(),
            "crate"
        );
        assert_eq!(
            ModulePath::from_file("src/main.rs").container_name(),
            "crate"
        );
        assert_eq!(
            ModulePath::from_file("tests/lang_js.rs").container_name(),
            "lang_js"
        );
        assert_eq!(
            ModulePath::from_file("handlers.rs").container_name(),
            "handlers"
        );
    }

    #[test]
    fn canonical_relative_paths() {
        let m = ModulePath::from_file("src/cli/args.rs");
        assert_eq!(m.canonical(&[], &segs("super::X")), segs("crate::cli::X"));
        assert_eq!(
            m.canonical(&[], &segs("self::y::f")),
            segs("crate::cli::args::y::f")
        );
        let tests = ["tests".to_owned()];
        assert_eq!(
            m.canonical(&tests, &segs("super::f")),
            segs("crate::cli::args::f")
        );
        assert_eq!(m.canonical(&[], &segs("std::fs")), segs("std::fs"));
    }

    #[test]
    fn short_key_is_last_two() {
        assert_eq!(short_key(&segs("crate::cli::args::parse")), "args::parse");
        assert_eq!(short_key(&segs("f")), "f");
    }
}
