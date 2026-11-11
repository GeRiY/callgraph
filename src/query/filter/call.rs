use regex::Regex;

use crate::index::ViaMatcher;
use crate::model::{Call, FileGraph};

pub(crate) trait CallFilter {
    fn matches(&self, graph: &FileGraph, call: &Call) -> bool;
}

/// A saját példányon át hívott metódus (`$this->m()`, `this.m()`, `self.m()`) célja a hívó típusán:
/// `Ns\Osztály::m`. Így a `Típus::metódus` minta (és rajta át a leszármazottak „via” illesztése) az örökölt
/// `$this->` hívást is megtalálja. `None`, ha a hívás már fel van oldva, vagy nem a saját példányon megy.
pub(crate) fn receiver_target(graph: &FileGraph, call: &Call) -> Option<String> {
    if call.resolved.is_some() || !matches!(call.on.as_deref(), Some("this" | "self")) {
        return None;
    }
    let container = call.container.as_deref().filter(|c| !c.is_empty())?;
    let method = call.target.as_str();
    if method.is_empty() || method.contains(['.', ':', '>', '(', ' ']) {
        return None;
    }
    Some(
        match graph.namespace.as_deref().filter(|ns| !ns.is_empty()) {
            Some(ns) => format!("{ns}\\{container}::{method}"),
            None => format!("{container}::{method}"),
        },
    )
}

/// A minta: a hívás-célra illeszkedő regex, vagy (ha van) a rokon típuson át hívott keresett metódus.
struct RegexSpec(Regex, Option<ViaMatcher>);
impl CallFilter for RegexSpec {
    fn matches(&self, graph: &FileGraph, call: &Call) -> bool {
        // a dinamikus hívás jelölés, nem hívó-él: a `--grep` nem találja meg (csak a `--category dynamic_call`)
        if call.is_dynamic() {
            return false;
        }
        let target = call.resolved.as_deref().unwrap_or(&call.target);
        // `?::név` (feloldatlan fogadó): a `Típus::név` minta név szerint illeszkedik rá
        if self.0.is_match(target)
            || crate::index::uncertain_match(&self.0, target)
            || self.1.as_ref().is_some_and(|via| via.via(target).is_some())
        {
            return true;
        }
        receiver_target(graph, call).is_some_and(|own| {
            self.0.is_match(&own) || self.1.as_ref().is_some_and(|via| via.via(&own).is_some())
        })
    }
}

struct CategorySpec(Vec<String>);
impl CallFilter for CategorySpec {
    fn matches(&self, _graph: &FileGraph, call: &Call) -> bool {
        self.0.iter().any(|category| category == &call.category)
    }
}

/// A `--grep` minta regexe: a callgraph jq-szűrője, `test($pattern; "i")` — kis-/nagybetű-független.
pub(crate) fn pattern_regex(pattern: &str) -> Result<Regex, regex::Error> {
    regex::RegexBuilder::new(&super::namespace_pattern(pattern))
        .case_insensitive(true)
        .build()
}

pub(crate) struct FilterChain {
    specs: Vec<Box<dyn CallFilter>>,
}
impl FilterChain {
    /// A szűrőlánc; a `via` a mintát kiegészítő „interfészen át” illesztés (lásd `index::ViaMatcher`).
    pub(crate) fn new(
        pattern: Option<&str>,
        categories: Option<&[String]>,
        via: Option<ViaMatcher>,
    ) -> Result<Self, regex::Error> {
        let mut specs: Vec<Box<dyn CallFilter>> = Vec::new();
        if let Some(pattern) = pattern {
            specs.push(Box::new(RegexSpec(pattern_regex(pattern)?, via)));
        }
        if let Some(categories) = categories {
            specs.push(Box::new(CategorySpec(categories.to_vec())));
        }
        Ok(Self { specs })
    }
    pub(crate) fn apply(&self, graph: &FileGraph) -> Option<FileGraph> {
        if self.specs.is_empty() {
            return Some(graph.clone());
        }
        let calls: Vec<_> = graph
            .calls
            .iter()
            .filter(|call| self.specs.iter().all(|spec| spec.matches(graph, call)))
            .cloned()
            .collect();
        if calls.is_empty() {
            return None;
        }
        // a jq kulcsa: (container // "") + "::" + (caller // "__top__") — a container nélküli hívás is számít
        let key = |container: Option<&str>, caller: Option<&str>| {
            (
                container.unwrap_or("").to_owned(),
                caller.unwrap_or("__top__").to_owned(),
            )
        };
        let hits: Vec<_> = calls
            .iter()
            .map(|call| key(call.container.as_deref(), call.caller.as_deref()))
            .collect();
        let mut filtered = graph.clone();
        filtered.calls = calls;
        filtered.methods.retain(|method| {
            hits.contains(&key(
                method.container.as_deref(),
                Some(method.name.as_str()),
            ))
        });
        // a container akkor marad, ha egy találat az övé (név szerint; az egymásba ágyazott azonos nevűeknél a
        // tartomány dönt, lásd `FileGraph::owns`) — nem a metódusokon át
        let keep: Vec<bool> = graph
            .containers
            .iter()
            .map(|container| {
                filtered
                    .calls
                    .iter()
                    .any(|call| graph.owns(container, call.container.as_deref(), call.line))
            })
            .collect();
        let mut keep = keep.into_iter();
        filtered.containers.retain(|_| keep.next().unwrap_or(false));
        filtered
            .control_flow
            .retain(|flow| hits.contains(&key(flow.container.as_deref(), flow.caller.as_deref())));
        filtered.properties.clear();
        filtered.constants.clear();
        filtered.comments.clear();
        Some(filtered)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(category: &str, kind: &str, target: &str) -> Call {
        Call {
            container: Some("A".into()),
            caller: Some("f".into()),
            line: 3,
            end_line: None,
            kind: kind.into(),
            target: target.into(),
            resolved: None,
            on: None,
            category: category.into(),
            args: None,
            handler: None,
        }
    }

    fn graph(calls: Vec<Call>) -> FileGraph {
        FileGraph {
            file: "a.php".into(),
            module: None,
            framework: None,
            namespace: None,
            imports: crate::model::Imports::default(),
            containers: Vec::new(),
            properties: Vec::new(),
            constants: Vec::new(),
            methods: Vec::new(),
            calls,
            comments: Vec::new(),
            control_flow: Vec::new(),
            strings: Vec::new(),
            writes: Vec::new(),
            sfc: None,
            binary: false,
            parse_errors: Vec::new(),
        }
    }

    #[test]
    fn grep_pattern_never_matches_a_dynamic_marker() {
        let g = graph(vec![
            call("dynamic_call", "dynamic", "$this->$joinMethod"),
            call("internal_helper", "method_call", "joinMethod"),
        ]);
        let chain = FilterChain::new(Some("joinMethod"), None, None).unwrap();
        let kept = chain.apply(&g).expect("the ordinary call matches");
        assert_eq!(kept.calls.len(), 1);
        assert_eq!(kept.calls[0].category, "internal_helper");
        // csak a jelölés van: a minta semmit sem talál
        let only = graph(vec![call("dynamic_call", "dynamic", "$this->$joinMethod")]);
        assert!(chain.apply(&only).is_none());
    }

    #[test]
    fn category_filter_still_selects_the_markers() {
        let g = graph(vec![
            call("dynamic_call", "dynamic", "$fn"),
            call("internal_helper", "method_call", "run"),
        ]);
        let chain = FilterChain::new(None, Some(&["dynamic_call".to_string()]), None).unwrap();
        let kept = chain.apply(&g).expect("the marker is selected");
        assert_eq!(kept.calls.len(), 1);
        assert_eq!(kept.calls[0].target, "$fn");
    }
}
