//! Route / action lookups for the `trace` command (`crate::packages::frameworks::trace`): which routes a target names
//! (route name, `GET:/uri`, a concrete URI, `Controller::method`), and what the controller action behind a route
//! validates and touches. Everything comes from the tables the other views already build (route table, model
//! catalogue) plus the controller's source text; nothing is parsed twice in a different way.

use std::collections::{BTreeSet, HashMap};
use std::sync::LazyLock;

use regex::Regex;

use super::super::files::ProjectFiles;
use super::routes::{Action, Route, short_class, strip_comments};
use super::{Laravel, has_verb, rules};

/// What one controller action validates and touches.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ActionFacts {
    /// The controller file (project-relative) and the line of the method.
    pub file: String,
    pub line: u32,
    /// The FormRequest classes (short names) the action takes.
    pub requests: Vec<String>,
    /// The validation fields with the required flag (inline rules and FormRequests merged).
    pub rules: Vec<(String, bool)>,
    /// The Eloquent models the action mentions, `Shelf(shelves)`.
    pub models: Vec<String>,
}

/// Does the route URI pattern (`/events/{event}`) accept the concrete (or equally written) URI?
fn uri_fits(pattern: &str, uri: &str) -> bool {
    let a: Vec<&str> = pattern.split('/').filter(|s| !s.is_empty()).collect();
    let b: Vec<&str> = uri.split('/').filter(|s| !s.is_empty()).collect();
    a.len() == b.len()
        && a.iter()
            .zip(&b)
            .all(|(p, u)| p == u || (p.starts_with('{') && p.ends_with('}')))
}

/// The line (1-based) of `function <method>` in the source, 1 when not found.
fn method_line(text: &str, method: &str) -> u32 {
    let needle = format!("function {method}");
    text.lines()
        .position(|line| {
            line.find(&needle).is_some_and(|at| {
                !line[at + needle.len()..]
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_alphanumeric() || c == '_')
            })
        })
        .and_then(|index| u32::try_from(index + 1).ok())
        .unwrap_or(1)
}

impl Laravel {
    /// The `composer.json` PSR-4 map for the class lookup (call once before `action_facts`).
    pub(crate) fn load_psr4(&mut self, files: &dyn ProjectFiles) {
        self.forms.psr4 = super::forms::composer_psr4(&self.root, files);
    }

    /// The routes a target names: a route name (`password.reset`, `route::password.reset`), a verb and URI
    /// (`route:GET:/events/{event}`, `GET:/events/1`), a URI (`/events/{event}`), or a controller action
    /// (`EventController::show`, `EventController@show`, `EventController`).
    pub(crate) fn find_routes(&self, target: &str) -> Vec<&Route> {
        let target = target.trim();
        let name = target
            .strip_prefix("route::")
            .or_else(|| target.strip_prefix("route:"))
            .unwrap_or(target);
        if let Some(&index) = self.table.by_name.get(name) {
            return vec![&self.table.routes[index]];
        }
        if let Some((verb, uri)) = name.split_once(':')
            && !verb.is_empty()
            && verb.chars().all(|c| c.is_ascii_uppercase())
            && uri.starts_with('/')
        {
            return self
                .table
                .routes
                .iter()
                .filter(|route| has_verb(&route.verb, verb) && uri_fits(&route.uri, uri))
                .collect();
        }
        if name.starts_with('/') {
            return self
                .table
                .routes
                .iter()
                .filter(|route| uri_fits(&route.uri, name))
                .collect();
        }
        let (class, method) = match name.split_once("::").or_else(|| name.split_once('@')) {
            Some((class, method)) => (class, Some(method)),
            None => (name, None),
        };
        let class = short_class(class);
        self.table
            .routes
            .iter()
            .filter(|route| {
                matches!(&route.action, Action::Controller { class: c, method: m }
                    if short_class(c) == class && method.is_none_or(|want| want == m))
            })
            .collect()
    }

    /// The routes of one controller action (short class name and method).
    pub(crate) fn routes_of_action(&self, class: &str, method: &str) -> Vec<&Route> {
        self.table
            .routes
            .iter()
            .filter(|route| {
                matches!(&route.action, Action::Controller { class: c, method: m }
                    if short_class(c) == short_class(class) && m == method)
            })
            .collect()
    }

    /// The routes whose handler is the Inertia page (`Route::inertia` and config-generated pages).
    pub(crate) fn routes_of_page(&self, page: &str) -> Vec<&Route> {
        self.table
            .routes
            .iter()
            .filter(|route| {
                matches!(&route.action, Action::Page(p) if p == page)
                    || route.page.as_deref() == Some(page)
            })
            .collect()
    }

    /// Every name a target could have been (for the "did you mean" hint): route names, `VERB /uri`, controller
    /// actions.
    pub(crate) fn trace_candidates(&self) -> Vec<String> {
        let mut out: BTreeSet<String> = BTreeSet::new();
        for route in &self.table.routes {
            if let Some(name) = &route.name {
                out.insert(format!("route::{name}"));
            }
            out.insert(format!("route:{}:{}", route.verb, route.uri));
            if let Action::Controller { class, method } = &route.action {
                out.insert(format!("{}::{method}", short_class(class)));
            }
        }
        out.into_iter().collect()
    }

    /// The source text of a class: the first readable candidate path and its text.
    fn read_class(
        &self,
        class: &str,
        classes: &HashMap<String, String>,
        files: &dyn ProjectFiles,
    ) -> Option<(String, String)> {
        self.class_rels(class, classes).into_iter().find_map(|rel| {
            let text = files.read(&self.root.join(&rel))?;
            Some((rel, text))
        })
    }

    /// What the controller action validates and touches; `None`: the controller file was not found.
    pub(crate) fn action_facts(
        &self,
        class: &str,
        method: &str,
        classes: &HashMap<String, String>,
        files: &dyn ProjectFiles,
    ) -> Option<ActionFacts> {
        let (file, text) = self.read_class(class, classes, files)?;
        let mut facts = ActionFacts {
            line: method_line(&text, method),
            file,
            ..ActionFacts::default()
        };
        let clean = strip_comments(&text);
        let Some((params, body)) = rules::method_parts(&clean, method) else {
            return Some(facts);
        };
        let (imports, namespace) = rules::imports_of(&clean);
        let mut validation = rules::body_validation(&body);
        for ty in rules::param_types(&params) {
            let Some(full) = rules::qualify(&ty, &imports, namespace.as_deref()) else {
                continue;
            };
            let Some((_, request)) = self.read_class(&full, classes, files) else {
                continue;
            };
            let from_request = rules::request_validation(&request);
            if from_request.found {
                facts.requests.push(short_class(&full).to_owned());
                validation.merge(from_request);
            }
        }
        facts.rules = validation
            .rules
            .iter()
            .map(|(field, flags)| (field.clone(), flags.required))
            .collect();
        facts.models = self.models_in(&format!("{params} {body}"));
        Some(facts)
    }

    /// The known Eloquent models (`Shelf(shelves)`) named in a piece of PHP text, in catalogue order.
    fn models_in(&self, text: &str) -> Vec<String> {
        static IDENT: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"\b[A-Z][A-Za-z0-9_]*\b").expect("valid regex"));
        let Some(data) = self.models.data.as_ref() else {
            return Vec::new();
        };
        let used: BTreeSet<&str> = IDENT.find_iter(text).map(|m| m.as_str()).collect();
        let mut seen = BTreeSet::new();
        data.models
            .iter()
            .filter(|model| used.contains(short_class(&model.class)) && seen.insert(&model.class))
            .map(|model| format!("{}({})", short_class(&model.class), model.table))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uri_pattern_accepts_params_and_trailing_slashes() {
        assert!(uri_fits("/events/{event}", "/events/7"));
        assert!(uri_fits("/events/{event}/", "/events/{event}"));
        assert!(!uri_fits("/events/{event}", "/events"));
        assert!(!uri_fits("/events/new", "/events/7"));
    }

    #[test]
    fn method_line_needs_the_whole_name() {
        let text = "<?php\nclass A {\n  function showAll() {}\n  public function show() {}\n}";
        assert_eq!(method_line(text, "show"), 4);
        assert_eq!(method_line(text, "gone"), 1);
    }
}
