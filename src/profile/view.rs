//! A réteg-címke a `show` kimeneteiben, csak kérésre (`--layers`): a mappa-összefoglaló (summary) rétegenként
//! csoportosítva, és az outline `FILE` sorainak végén a fájl szerepe (`  [repository]`). Kapcsoló nélkül a
//! kimenet bájtra azonos.

use crate::error::Result;
use crate::i18n::Lang;
use crate::query::QueryResult;
use crate::query::format::FormatterRegistry;

use super::role::Role;
use super::scope::LayerScope;

/// A summary rétegenként: `[controller]  3 files` fejléc, alatta a fájlok szokásos summary-sorai; a rétegek a
/// várt hívásirány sorrendjében, az ismeretlenek a végén.
pub(crate) fn layered_summary(
    rows: &[QueryResult],
    scope: &LayerScope,
    lang: Lang,
) -> Result<String> {
    let mut groups: Vec<(Option<Role>, Vec<&QueryResult>)> = Vec::new();
    for row in rows {
        let role = scope.role_of_key(&row.graph.file);
        match groups.iter_mut().find(|(r, _)| *r == role) {
            Some((_, members)) => members.push(row),
            None => groups.push((role, vec![row])),
        }
    }
    groups.sort_by_key(|(role, _)| (role.is_none(), role.map(Role::display_order)));
    let mut out = String::new();
    for (role, members) in groups {
        let name = role.map_or_else(
            || lang.pick("unknown", "ismeretlen").to_owned(),
            |r| r.name().to_owned(),
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

/// Az outline `FILE  <út>[  NS:…]` sorainak végére a fájl szerepe (ismeretlen szerepnél változatlan sor).
pub(crate) fn tag_file_lines(text: &str, scope: &LayerScope) -> String {
    text.split_inclusive('\n')
        .map(|line| {
            let Some(rest) = line.strip_prefix("FILE  ") else {
                return line.to_owned();
            };
            let body = rest.trim_end_matches('\n');
            let path = body.split("  NS:").next().unwrap_or(body);
            let tag = scope.tag(path);
            if tag.is_empty() {
                return line.to_owned();
            }
            let newline = if line.ends_with('\n') { "\n" } else { "" };
            format!("FILE  {body}{tag}{newline}")
        })
        .collect()
}
