use regex::Regex;

use crate::model::{Call, FileGraph};

pub(crate) trait CallFilter {
    fn matches(&self, call: &Call) -> bool;
}

struct RegexSpec(Regex);
impl CallFilter for RegexSpec {
    fn matches(&self, call: &Call) -> bool {
        self.0
            .is_match(call.resolved.as_deref().unwrap_or(&call.target))
    }
}

struct CategorySpec(Vec<String>);
impl CallFilter for CategorySpec {
    fn matches(&self, call: &Call) -> bool {
        self.0.iter().any(|category| category == &call.category)
    }
}

pub(crate) struct FilterChain {
    specs: Vec<Box<dyn CallFilter>>,
}
impl FilterChain {
    pub(crate) fn new(
        pattern: Option<&str>,
        categories: Option<&[String]>,
    ) -> Result<Self, regex::Error> {
        let mut specs: Vec<Box<dyn CallFilter>> = Vec::new();
        if let Some(pattern) = pattern {
            // a callgraph jq-szűrője: test($pattern; "i") — kis-/nagybetű-független
            let regex = regex::RegexBuilder::new(pattern)
                .case_insensitive(true)
                .build()?;
            specs.push(Box::new(RegexSpec(regex)));
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
