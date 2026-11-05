//! A `<fájl>::<metódus>` kijelölés szűrése a formázás előtt (a callgraph jq-szűrőjének mása):
//! metódusok, a hozzájuk tartozó hívások / elágazások / stringek, és a metódusok containerei.

use std::borrow::Cow;

use crate::model::FileGraph;
use crate::query::QueryResult;

pub(super) fn selected(row: &QueryResult) -> Cow<'_, FileGraph> {
    let Some(names) = &row.methods else {
        return Cow::Borrowed(&row.graph);
    };
    let picked = |caller: &Option<String>| caller.as_ref().is_some_and(|c| names.contains(c));
    let mut graph = row.graph.clone();
    graph.methods.retain(|m| names.contains(&m.name));
    graph.calls.retain(|c| picked(&c.caller));
    graph.control_flow.retain(|c| picked(&c.caller));
    graph.strings.retain(|s| picked(&s.caller));
    // a kijelölt metódusok containerei (az egymásba ágyazott azonos nevűeknél a tartomány dönt)
    let keep: Vec<bool> = graph
        .containers
        .iter()
        .map(|c| {
            graph
                .methods
                .iter()
                .any(|m| graph.owns(c, m.container.as_deref(), m.line))
        })
        .collect();
    let mut keep = keep.into_iter();
    graph.containers.retain(|_| keep.next().unwrap_or(false));
    Cow::Owned(graph)
}

/// A jq `gsub("[^a-zA-Z0-9]";"_")` mása (a mermaid csomópont-azonosítóihoz).
pub(super) fn sanitize(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

/// A jq `if length < n then pad else + " "` mása.
pub(super) fn pad(text: &str, width: usize) -> String {
    let len = text.chars().count();
    if len < width {
        format!("{text}{}", " ".repeat(width - len))
    } else {
        format!("{text} ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_replaces_every_non_alphanumeric_char() {
        assert_eq!(sanitize("App\\Models\\User::find"), "App_Models_User__find");
        assert_eq!(sanitize("árvíz-1"), "_rv_z_1");
    }

    #[test]
    fn pad_follows_jq_prelude() {
        assert_eq!(pad("abc", 5), "abc  ");
        assert_eq!(pad("abcde", 5), "abcde ");
    }
}
