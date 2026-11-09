//! A réteg-címke a `show` kimeneteiben, csak kérésre: a mappa-összefoglaló (summary) rétegenként csoportosítva
//! (`--layers`), az outline `FILE` sorainak végén a fájl rétege (`  [repository]`), és a `--by-layer` csoport-fejléce
//! (`[service]  3 calls`). Kapcsoló nélkül a kimenet bájtra azonos.

use crate::error::Result;
use crate::i18n::Lang;
use crate::query::QueryResult;
use crate::query::format::FormatterRegistry;

use super::layer::{Layer, Mixed};
use super::scope::LayerScope;

/// Mit számol a `--by-layer` fejléce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Count {
    /// A csoport találat-hívásai (`--grep` sorai, 1. szint).
    Calls,
    /// A csoport hívó metódusai (a 2+. szint).
    Callers,
}

/// A `--by-layer` csoport-fejléce: `[service]  3 calls` / `[service]  1 caller` (magyarul: `3 hívás`, `1 hívó`).
pub(crate) fn group_header(label: &str, n: usize, count: Count, lang: Lang) -> String {
    let what = match (count, lang, n) {
        (Count::Calls, Lang::En, 1) => "call",
        (Count::Calls, Lang::En, _) => "calls",
        (Count::Callers, Lang::En, 1) => "caller",
        (Count::Callers, Lang::En, _) => "callers",
        (Count::Calls, Lang::Hu, _) => "hívás",
        (Count::Callers, Lang::Hu, _) => "hívó",
    };
    format!("[{label}]  {n} {what}")
}

/// A summary rétegenként: `[controller]  3 files` fejléc, alatta a fájlok szokásos summary-sorai; a rétegek a
/// várt hívásirány sorrendjében, az ismeretlenek a végén.
pub(crate) fn layered_summary(
    rows: &[QueryResult],
    scope: &LayerScope,
    lang: Lang,
) -> Result<String> {
    let mut groups: Vec<(Option<Layer>, Vec<&QueryResult>)> = Vec::new();
    for row in rows {
        let layer = scope.layer_of_key(&row.graph.file);
        match groups.iter_mut().find(|(l, _)| *l == layer) {
            Some((_, members)) => members.push(row),
            None => groups.push((layer, vec![row])),
        }
    }
    groups.sort_by_key(|(layer, _)| {
        (
            layer.is_none(),
            layer.map(|l| (l.role.display_order(), l.eco)),
        )
    });
    let mixed = Mixed::of(groups.iter().filter_map(|(layer, _)| *layer));
    let mut out = String::new();
    for (layer, members) in groups {
        let name = layer.map_or_else(
            || lang.pick("unknown", "ismeretlen").to_owned(),
            |l| l.label(&mixed),
        );
        let n = members.len();
        out.push_str(&match lang {
            Lang::En if n == 1 => format!("[{name}]  1 file\n"),
            Lang::En => format!("[{name}]  {n} files\n"),
            Lang::Hu => format!("[{name}]  {n} fájl\n"),
        });
        for row in members {
            out.push_str(&FormatterRegistry::format(
                "summary",
                std::slice::from_ref(row),
            )?);
        }
    }
    Ok(out)
}

/// Az outline `FILE  <út>[  NS:…]` sorainak végére a fájl rétege (ismeretlen szerepnél változatlan sor).
pub(crate) fn tag_file_lines(text: &str, scope: &LayerScope) -> String {
    let path_of = |line: &str| -> Option<String> {
        let body = line.strip_prefix("FILE  ")?.trim_end_matches('\n');
        Some(body.split("  NS:").next().unwrap_or(body).to_owned())
    };
    let mixed = Mixed::of(
        text.lines()
            .filter_map(path_of)
            .filter_map(|path| scope.layer_of_key(&path)),
    );
    text.split_inclusive('\n')
        .map(|line| {
            let Some(path) = path_of(line) else {
                return line.to_owned();
            };
            let tag = scope.tag(&path, &mixed);
            if tag.is_empty() {
                return line.to_owned();
            }
            let body = line.trim_end_matches('\n');
            let newline = if line.ends_with('\n') { "\n" } else { "" };
            format!("{body}{tag}{newline}")
        })
        .collect()
}
