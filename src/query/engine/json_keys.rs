//! JSON-fájl kulcs-útja `::` szelektorként (`package.json::scripts`, `composer.json::require`, `files.0`): a kért,
//! metódusként nem létező kulcs a lekérdezés idejére szakasz lesz a pontos forrás-soraival, így a `--body` és a
//! `--lines` nélküli olvasás is a kulcs JSON-részfáját adja. A tárolt gráf nem változik.

use std::path::Path;

use crate::model::{FileGraph, Method};

/// A `names` közül a JSON-kulcs-utakat (amik nem metódusok) szakaszként a gráfhoz adja; nem JSON-fájlnál nincs hatása.
pub(super) fn add_key_sections(path: &Path, graph: &mut FileGraph, names: &[String]) {
    let json = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("json"));
    if !json {
        return;
    }
    let wanted: Vec<&String> = names
        .iter()
        .filter(|name| !graph.methods.iter().any(|m| &m.name == *name))
        .collect();
    if wanted.is_empty() {
        return;
    }
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };
    let container = graph.containers.first().map(|c| c.name.clone());
    for name in wanted {
        if let Some((line, end)) = crate::lang::json::key_lines(&text, name) {
            graph.methods.push(Method {
                container: container.clone(),
                name: name.clone(),
                line,
                end_line: Some(end),
                visibility: None,
                is_static: false,
                is_abstract: false,
                kind: Some("key".into()),
                params: Vec::new(),
                return_type: None,
                doc: None,
                body: None,
            });
        }
    }
}
