//! A hívás forrássora a hívó-találatok mellé (`L25 │ $this->resolver->getPlayables($id)`): az agentnek így nem kell
//! külön `--body` hívás ahhoz, hogy lássa, HOGYAN hívják a célt. Egy sor, összevont szóközökkel, ~100 karakterre vágva.
//! Egy hívó metóduson belül az azonos sorok egy sorba vonódnak (`L12, L40 │ …`), és legfeljebb `MAX_PER_METHOD`
//! sor marad; a többi sorszáma egy `… +N more: L55, L61` sorba kerül.

use std::cell::RefCell;
use std::collections::HashMap;

use crate::i18n::Lang;

/// Ennyi karakter után a sor `…`-ra végződik.
const MAX_CHARS: usize = 100;

/// Egy hívó metódusban legfeljebb ennyi (összevont) hívás-sor; a többi egy `… +N more: L…` sorban.
pub(crate) const MAX_PER_METHOD: usize = 3;

/// Fájlonként egyszer beolvasott forrássorok (a kimenet egy renderelésére; a nem olvasható fájl üres).
/// A projekt-gráfból előre ismert (tárolt) hívás-sorokhoz a fájlt nem kell újraolvasni (`remember`).
#[derive(Default)]
pub(crate) struct SourceLines {
    files: RefCell<HashMap<String, Option<Vec<String>>>>,
    known: HashMap<String, HashMap<u32, String>>,
}

impl SourceLines {
    /// Egy fájl már tömörített hívás-sorai (a projekt-gráfból); az itt nem szereplő sorokat a `get` a fájlból olvassa.
    pub(crate) fn remember(&mut self, file: &str, snippets: &[(u32, String)]) {
        if self.known.contains_key(file) {
            return;
        }
        self.known
            .insert(file.to_owned(), snippets.iter().cloned().collect());
    }

    /// A `file` `line`-adik (1-től számolt) sora tömörítve; `None`, ha a fájl / sor nem olvasható vagy üres.
    pub(crate) fn get(&self, file: &str, line: u32) -> Option<String> {
        if let Some(code) = self.known.get(file).and_then(|lines| lines.get(&line)) {
            return Some(code.clone());
        }
        let mut files = self.files.borrow_mut();
        let lines = files.entry(file.to_owned()).or_insert_with(|| {
            std::fs::read(file).ok().map(|bytes| {
                crate::io::decode_source(&bytes)
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

/// A fájl hívás-sorai tömörítve (`(sor, kód)`, soronként egyszer, növekvő sorrendben) — a projekt-gráf ezt tárolja,
/// hogy a hívó-találatok kiírásához ne kelljen a fájlt újraolvasni. A nem olvasható fájlra / üres sorra nincs elem.
pub(crate) fn call_snippets(
    path: &std::path::Path,
    lines: impl IntoIterator<Item = u32>,
) -> Vec<(u32, String)> {
    let mut wanted: Vec<u32> = lines.into_iter().collect();
    wanted.sort_unstable();
    wanted.dedup();
    let Ok(bytes) = std::fs::read(path) else {
        return Vec::new();
    };
    let text = crate::io::decode_source(&bytes);
    let source: Vec<&str> = text.lines().collect();
    wanted
        .into_iter()
        .filter_map(|line| {
            let at = usize::try_from(line).ok()?.checked_sub(1)?;
            Some((line, compact(source.get(at)?)?))
        })
        .collect()
}

/// Egy kiírandó (összevont) hívás-sor: a sorszámok az előfordulás sorrendjében, a közös kulcs (pl. a `via`) és a kód.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Site<K> {
    pub lines: Vec<u32>,
    pub key: K,
    pub code: Option<String>,
}

/// Egy hívó metódus hívás-helyei összevonva: az azonos kulcsú (pl. ugyanaz a `via`) és azonos (ismert) kódú helyek
/// egy sorba kerülnek. A sorok sorrendje az első előfordulásé; ismeretlen kódú hely nem vonódik össze.
pub(crate) fn merge<K: PartialEq>(
    sites: impl IntoIterator<Item = (u32, K, Option<String>)>,
) -> Vec<Site<K>> {
    let mut out: Vec<Site<K>> = Vec::new();
    for (line, key, code) in sites {
        if code.is_some()
            && let Some(site) = out
                .iter_mut()
                .find(|site| site.key == key && site.code == code)
        {
            if !site.lines.contains(&line) {
                site.lines.push(line);
            }
            continue;
        }
        out.push(Site {
            lines: vec![line],
            key,
            code,
        });
    }
    out
}

/// Legfeljebb `MAX_PER_METHOD` sor marad; a levágott sorok sorszámai (növekvő, ismétlés nélkül) a `more` sorhoz.
pub(crate) fn cap<K>(mut sites: Vec<Site<K>>) -> (Vec<Site<K>>, Vec<u32>) {
    if sites.len() <= MAX_PER_METHOD {
        return (sites, Vec::new());
    }
    let mut rest: Vec<u32> = sites
        .split_off(MAX_PER_METHOD)
        .into_iter()
        .flat_map(|site| site.lines)
        .collect();
    rest.sort_unstable();
    rest.dedup();
    (sites, rest)
}

/// Sorszám-lista: `L12, L40`.
pub(crate) fn line_list(lines: &[u32]) -> String {
    lines
        .iter()
        .map(|line| format!("L{line}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// A sorszám(ok) utáni jelölések: ` via Resolver`, ` │ kód`, ` via Resolver │ kód` (vagy üres).
pub(crate) fn marks(lang: Lang, via: Option<&str>, code: Option<&str>) -> String {
    let mut out = String::new();
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

/// A hívás helye a jelölésekkel: `L25`, `L25 via Resolver`, `L25 │ kód`, `L25 via Resolver │ kód`.
pub(crate) fn call_line(lang: Lang, line: u32, via: Option<&str>, code: Option<&str>) -> String {
    sites_line(lang, &[line], via, code)
}

/// Mint a `call_line`, több (azonos kódú) sorszámmal: `L12, L40 via Resolver │ kód`.
pub(crate) fn sites_line(
    lang: Lang,
    lines: &[u32],
    via: Option<&str>,
    code: Option<&str>,
) -> String {
    let mut out = line_list(lines);
    out.push_str(&marks(lang, via, code));
    out
}

/// A levágott hívás-sorok jelzése a sorszámaikkal (`… +2 more: L55, L61`).
pub(crate) fn more(lang: Lang, lines: &[u32]) -> String {
    let (count, list) = (lines.len(), line_list(lines));
    ltf!(
        lang,
        "… +{count} more: {list}",
        "… +{count} további: {list}"
    )
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
        assert_eq!(more(Lang::Hu, &[55, 61]), "… +2 további: L55, L61");
        assert_eq!(more(Lang::En, &[9]), "… +1 more: L9");
        assert_eq!(
            sites_line(Lang::En, &[12, 40], Some("Repo"), Some("f();")),
            "L12, L40 via Repo │ f();"
        );
    }

    fn site(
        lines: &[u32],
        key: Option<&'static str>,
        code: Option<&str>,
    ) -> Site<Option<&'static str>> {
        Site {
            lines: lines.to_vec(),
            key,
            code: code.map(str::to_owned),
        }
    }

    #[test]
    fn merge_joins_identical_lines_with_the_same_via_only() {
        let code = |text: &str| Some(text.to_owned());
        let merged = merge([
            (12, None, code("$this->repo->find($id)")),
            (20, None, code("other()")),
            (40, None, code("$this->repo->find($id)")),
            (44, Some("Repo"), code("$this->repo->find($id)")),
            (50, None, None),
            (51, None, None),
        ]);
        assert_eq!(
            merged,
            vec![
                site(&[12, 40], None, Some("$this->repo->find($id)")),
                site(&[20], None, Some("other()")),
                site(&[44], Some("Repo"), Some("$this->repo->find($id)")),
                site(&[50], None, None),
                site(&[51], None, None),
            ]
        );
    }

    #[test]
    fn cap_keeps_three_rows_and_lists_the_rest_sorted() {
        let rows = vec![
            site(&[1], None, Some("a")),
            site(&[2, 9], None, Some("b")),
            site(&[3], None, Some("c")),
            site(&[61], None, Some("d")),
            site(&[4, 55], None, Some("e")),
        ];
        let (shown, rest) = cap(rows.clone());
        assert_eq!(shown, rows[..3].to_vec());
        assert_eq!(rest, vec![4, 55, 61]);
        let (shown, rest) = cap(rows[..3].to_vec());
        assert_eq!(shown.len(), 3);
        assert!(rest.is_empty());
    }

    #[test]
    fn call_snippets_compacts_each_call_line_once() {
        let dir = std::env::temp_dir().join(format!(
            "callgraph-snippets-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a.php");
        std::fs::write(&file, "<?php\n  a(  1 );\n\n  b();\n").unwrap();
        assert_eq!(
            call_snippets(&file, [4, 2, 2, 3, 99]),
            vec![(2, "a( 1 );".to_owned()), (4, "b();".to_owned())]
        );
        assert!(call_snippets(&dir.join("missing.php"), [1]).is_empty());
        let mut lines = SourceLines::default();
        lines.remember("/virtual.php", &[(7, "known();".to_owned())]);
        assert_eq!(lines.get("/virtual.php", 7).as_deref(), Some("known();"));
        assert_eq!(lines.get("/virtual.php", 8), None);
        let _ = std::fs::remove_dir_all(&dir);
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
