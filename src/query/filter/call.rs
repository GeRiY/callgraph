use regex::Regex;

use crate::index::ViaMatcher;
use crate::model::{Call, FileGraph};

pub(crate) trait CallFilter {
    fn matches(&self, call: &Call) -> bool;
}

/// A minta: a hívás-célra illeszkedő regex, vagy (ha van) a rokon típuson át hívott keresett metódus.
struct RegexSpec(Regex, Option<ViaMatcher>);
impl CallFilter for RegexSpec {
    fn matches(&self, call: &Call) -> bool {
        let target = call.resolved.as_deref().unwrap_or(&call.target);
        // `?::név` (feloldatlan fogadó): a `Típus::név` minta név szerint illeszkedik rá
        self.0.is_match(target)
            || crate::index::uncertain_match(&self.0, target)
            || self.1.as_ref().is_some_and(|via| via.via(target).is_some())
    }
}

struct CategorySpec(Vec<String>);
impl CallFilter for CategorySpec {
    fn matches(&self, call: &Call) -> bool {
        self.0.iter().any(|category| category == &call.category)
    }
}

/// A `--grep` minta regexe: a callgraph jq-szűrője, `test($pattern; "i")` — kis-/nagybetű-független.
pub(crate) fn pattern_regex(pattern: &str) -> Result<Regex, regex::Error> {
    regex::RegexBuilder::new(pattern)
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
            .filter(|call| self.specs.iter().all(|spec| spec.matches(call)))
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
        // a container akkor marad, ha a neve a találatok container-részében szerepel (nem a metódusokon át)
        filtered
            .containers
            .retain(|container| hits.iter().any(|(name, _)| *name == container.name));
        filtered
            .control_flow
            .retain(|flow| hits.contains(&key(flow.container.as_deref(), flow.caller.as_deref())));
        filtered.properties.clear();
        filtered.constants.clear();
        filtered.comments.clear();
        Some(filtered)
    }
}
