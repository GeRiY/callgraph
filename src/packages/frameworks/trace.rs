//! `callgraph trace <target>`: the task slice. One compact chain (at most 30 lines) from a route to the page it
//! renders: route (verb, URI, name, `file:line`) → middleware → controller action → FormRequest rules → models the
//! action touches → Inertia page (+ the props the controller passes) → the page's child components. The target is a
//! Laravel route name (`route::password.reset`), `route:GET:/events/{event}` or a URI, a controller action
//! (`EventController::show`) or an Inertia page (`Pages/Events/Show.vue`); for a JS / TS project the server route
//! (`GET /api/chats`) with the client calls that reach it.
//!
//! Nothing is parsed in a new way: the route table, the model catalogue, the render sites and the validation rules
//! are the ones the `routes`, `models`, `inertia` and `forms` views use; this module only composes them.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use super::laravel::{Action, Route, short_class};
use super::{DiskProjectFiles, Inertia, Laravel};
use crate::analysis::Analyzer;
use crate::i18n::Lang;
use crate::locate::ProjectSource;

/// The whole output stays within this many lines.
const MAX_LINES: usize = 30;
/// Route blocks printed in full (the rest are listed as `+N routes`).
const MAX_ROUTES: usize = 2;
/// Names in one list (middleware, components, models, props) before `+N`.
const MAX_NAMES: usize = 6;
/// Validation fields listed before `+N`.
const MAX_FIELDS: usize = 8;
/// Names in the "did you mean" hint.
const MAX_HINTS: usize = 5;

/// The trace of `target` in the project at `root`; always a short text (a hint for an unknown target).
pub(crate) fn run(root: &Path, target: &str, lang: Lang) -> String {
    let target = target.trim();
    let packages = crate::packages::open(root);
    let mut inertia = Inertia::detect(root, &packages, &DiskProjectFiles);
    let mut laravel = Laravel::detect(root, &packages);
    let base = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let mut classes: HashMap<String, String> = HashMap::new();
    let mut components: HashMap<String, Vec<String>> = HashMap::new();
    if laravel.is_some() || inertia.is_some() {
        let analyzer = Analyzer::with_defaults();
        // a failed project graph only leaves the tables empty (the hint then says so)
        let _ = ProjectSource::new(root, &analyzer).with_files(|files| {
            let code = || {
                files
                    .iter()
                    .filter(|file| file.code)
                    .map(|file| (file.path, file.graph))
            };
            if let Some(laravel) = laravel.as_mut() {
                laravel.scan(code(), &DiskProjectFiles, false);
                laravel.scan_models(code(), &DiskProjectFiles);
                laravel.load_psr4(&DiskProjectFiles);
            }
            if let Some(inertia) = inertia.as_mut() {
                inertia.scan(code(), &DiskProjectFiles);
            }
            if let (Some(inertia), Some(laravel)) = (inertia.as_mut(), laravel.as_ref()) {
                inertia.add_sites(laravel.page_sites());
            }
            for file in files {
                let Some(rel) = relative(file.path, root, &base) else {
                    continue;
                };
                if rel.ends_with(".php") {
                    if let Some(namespace) = file.graph.namespace.as_deref() {
                        for container in &file.graph.containers {
                            classes.insert(
                                format!("{}\\{}", namespace.trim_matches('\\'), container.name),
                                rel.clone(),
                            );
                        }
                    }
                } else if rel.ends_with(".vue") {
                    let used: Vec<String> = file
                        .graph
                        .imports
                        .parts()
                        .0
                        .iter()
                        .filter(|(_, full)| full.ends_with(".vue"))
                        .map(|(alias, _)| alias.clone())
                        .collect();
                    if !used.is_empty() {
                        components.insert(rel, used);
                    }
                }
            }
        });
    }
    let chain = Chain {
        laravel: laravel.as_ref(),
        inertia: inertia.as_ref(),
        classes,
        components,
        lang,
    };
    if let Some(lines) = chain.lines(target) {
        return render(cap_lines(lines));
    }
    // a JS / TS project: the server route and the client calls that reach it
    let web = crate::packages::WebRoutes::open(root, &base, HashSet::new());
    let lines = web.trace_lines(target);
    if !lines.is_empty() {
        return render(cap_lines(lines));
    }
    let mut candidates = chain.candidates();
    candidates.extend(web.trace_candidates());
    render(vec![unknown_line(target, &candidates, lang)])
}

/// The project-relative, `/`-separated path of a file.
fn relative(path: &Path, root: &Path, base: &Path) -> Option<String> {
    let stripped = path
        .strip_prefix(root)
        .ok()
        .or_else(|| path.strip_prefix(base).ok())?;
    Some(stripped.to_string_lossy().replace('\\', "/"))
}

fn render(lines: Vec<String>) -> String {
    format!("{}\n", lines.join("\n"))
}

/// At most `MAX_LINES` lines; the rest is one `+N` line (never cut inside a line).
fn cap_lines(mut lines: Vec<String>) -> Vec<String> {
    if lines.len() > MAX_LINES {
        let rest = lines.len() - (MAX_LINES - 1);
        lines.truncate(MAX_LINES - 1);
        lines.push(format!("+{rest}"));
    }
    lines
}

/// `a, b, c, +2`: at most `max` names, the rest counted.
fn capped(names: &[String], max: usize) -> String {
    let mut text = names
        .iter()
        .take(max)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    if names.len() > max {
        text.push_str(&format!(", +{}", names.len() - max));
    }
    text
}

/// The names of `candidates` that share the most word-like tokens with the target (at most `MAX_HINTS`).
fn closest(target: &str, candidates: &[String]) -> Vec<String> {
    let bare = target
        .strip_prefix("route::")
        .or_else(|| target.strip_prefix("route:"))
        .unwrap_or(target);
    let tokens: Vec<String> = bare
        .split(|c: char| !c.is_alphanumeric())
        .filter(|token| token.chars().count() >= 3)
        .map(str::to_lowercase)
        .collect();
    let mut scored: Vec<(usize, &String)> = candidates
        .iter()
        .filter_map(|candidate| {
            let low = candidate.to_lowercase();
            let score = tokens.iter().filter(|t| low.contains(t.as_str())).count();
            (score > 0).then_some((score, candidate))
        })
        .collect();
    scored.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then(a.1.len().cmp(&b.1.len()))
            .then(a.1.cmp(b.1))
    });
    scored.dedup_by(|a, b| a.1 == b.1);
    scored
        .into_iter()
        .take(MAX_HINTS)
        .map(|(_, name)| name.clone())
        .collect()
}

/// The one-line answer for an unknown target.
fn unknown_line(target: &str, candidates: &[String], lang: Lang) -> String {
    let near = closest(target, candidates);
    if near.is_empty() {
        crate::tfl!(
            lang,
            "? unknown target '{target}' (no similar route, controller action or page)",
            "? ismeretlen cél: '{target}' (nincs hasonló route, controller-akció vagy oldal)"
        )
    } else {
        let list = near.join(", ");
        crate::tfl!(
            lang,
            "? unknown target '{target}', closest: {list}",
            "? ismeretlen cél: '{target}', legközelebbi: {list}"
        )
    }
}

/// The page name a target names (`Pages/Events/Show.vue`, `resources/js/Pages/Events/Show.vue`, `Events/Show`), if the
/// page file exists.
fn page_of(inertia: &Inertia, target: &str) -> Option<String> {
    if let Some(name) = inertia.page_name(target) {
        return Some(name);
    }
    let stem = target
        .rsplit_once('.')
        .filter(|(_, ext)| (1..=6).contains(&ext.len()) && ext.chars().all(char::is_alphanumeric))
        .map_or(target, |(stem, _)| stem);
    let lower = stem.to_ascii_lowercase();
    let name = match lower.rfind("pages/") {
        Some(at) => &stem[at + "pages/".len()..],
        None => stem,
    };
    inertia
        .resolve(name)
        .filter(|link| link.exists)
        .map(|_| name.to_owned())
}

/// The scanned project facts the chain is composed from.
struct Chain<'a> {
    laravel: Option<&'a Laravel>,
    inertia: Option<&'a Inertia>,
    /// Class (full name) → project-relative file, from the project graph.
    classes: HashMap<String, String>,
    /// Page file → the `.vue` components it imports.
    components: HashMap<String, Vec<String>>,
    lang: Lang,
}

impl Chain<'_> {
    /// The chain for a Laravel / Inertia target; `None`: the target is none of its routes, actions or pages.
    fn lines(&self, target: &str) -> Option<Vec<String>> {
        if let Some(laravel) = self.laravel {
            let routes = laravel.find_routes(target);
            if !routes.is_empty() {
                return Some(self.route_blocks(&routes, None));
            }
        }
        let inertia = self.inertia?;
        let page = page_of(inertia, target)?;
        let mut routes: Vec<&Route> = Vec::new();
        if let Some(laravel) = self.laravel {
            for site in inertia.sites().iter().filter(|site| site.page == page) {
                if let (Some(class), Some(method)) = (&site.container, &site.caller) {
                    routes.extend(laravel.routes_of_action(class, method));
                }
            }
            routes.extend(laravel.routes_of_page(&page));
        }
        // the same route found through two render sites is one route
        let mut seen = HashSet::new();
        routes.retain(|route| seen.insert((route.verb.clone(), route.uri.clone(), route.line)));
        if routes.is_empty() {
            let mut lines = Vec::new();
            let props = self.page_props(&page, None);
            self.page_lines(&page, &props, &mut lines);
            return Some(lines);
        }
        Some(self.route_blocks(&routes, Some(page.as_str())))
    }

    /// Every target the Laravel / Inertia side understands (for the hint).
    fn candidates(&self) -> Vec<String> {
        let mut out = self
            .laravel
            .map(Laravel::trace_candidates)
            .unwrap_or_default();
        if let Some(inertia) = self.inertia {
            out.extend(inertia.page_shorts());
        }
        out
    }

    /// The blocks of the first routes, then one `+N routes` line.
    fn route_blocks(&self, routes: &[&Route], only_page: Option<&str>) -> Vec<String> {
        let mut lines = Vec::new();
        for route in routes.iter().take(MAX_ROUTES) {
            self.route_block(route, only_page, &mut lines);
        }
        if routes.len() > MAX_ROUTES {
            let rest: Vec<String> = routes[MAX_ROUTES..]
                .iter()
                .map(|route| format!("{} {}", route.verb, route.uri))
                .collect();
            lines.push(format!(
                "+{} {}: {}",
                rest.len(),
                self.lang.pick("routes", "route"),
                capped(&rest, 4)
            ));
        }
        lines
    }

    fn route_block(&self, route: &Route, only_page: Option<&str>, lines: &mut Vec<String>) {
        let mut head = format!("route {} {}", route.verb, route.uri);
        if let Some(name) = &route.name {
            head.push_str(&format!("  name={name}"));
        }
        head.push_str(&format!("  {}:{}", route.file, route.line));
        lines.push(head);
        if !route.middleware.is_empty() {
            let names: Vec<String> = route
                .middleware
                .iter()
                .map(|name| short_class(name).to_owned())
                .collect();
            lines.push(format!("mw {}", capped(&names, MAX_NAMES)));
        }
        match &route.action {
            Action::Controller { class, method } => {
                self.action_lines(class, method, only_page, lines);
            }
            Action::Page(page) => {
                let props = self.page_props(page, None);
                self.page_lines(page, &props, lines);
            }
            Action::View(view) => lines.push(format!("view {view}")),
            Action::Redirect(to) => lines.push(format!("redirect {to}")),
            Action::Closure => lines.push("action closure".to_owned()),
        }
        if let Some(page) = &route.page {
            let props = self.page_props(page, None);
            self.page_lines(page, &props, lines);
        }
    }

    /// The controller action, its validation, the models it touches and the pages it renders.
    fn action_lines(
        &self,
        class: &str,
        method: &str,
        only_page: Option<&str>,
        lines: &mut Vec<String>,
    ) {
        let short = format!("{}@{method}", short_class(class));
        let facts = self.laravel.and_then(|laravel| {
            laravel.action_facts(class, method, &self.classes, &DiskProjectFiles)
        });
        let Some(facts) = facts else {
            lines.push(format!(
                "action {short}  ({})",
                self.lang.pick(
                    "controller file not found",
                    "a controller-fájl nem található"
                )
            ));
            return;
        };
        lines.push(format!("action {short}  {}:{}", facts.file, facts.line));
        if !facts.rules.is_empty() {
            let from = if facts.requests.is_empty() {
                "inline".to_owned()
            } else {
                facts.requests.join("+")
            };
            let fields: Vec<String> = facts
                .rules
                .iter()
                .map(|(field, required)| {
                    if *required {
                        format!("{field}*")
                    } else {
                        field.clone()
                    }
                })
                .collect();
            lines.push(format!("rules {from}: {}", capped(&fields, MAX_FIELDS)));
        }
        if !facts.models.is_empty() {
            lines.push(format!("models {}", capped(&facts.models, MAX_NAMES)));
        }
        let Some(inertia) = self.inertia else {
            return;
        };
        let mut pages: Vec<&str> = Vec::new();
        for site in inertia.sites() {
            let here = site.file == facts.file && site.caller.as_deref() == Some(method);
            if here
                && !pages.contains(&site.page.as_str())
                && only_page.is_none_or(|p| p == site.page)
            {
                pages.push(&site.page);
            }
        }
        for page in pages.into_iter().take(2) {
            let props = self.page_props(page, Some((facts.file.as_str(), method)));
            self.page_lines(page, &props, lines);
        }
    }

    /// The props the render sites pass (of one action, or of every render site of the page), without repeats.
    fn page_props(&self, page: &str, action: Option<(&str, &str)>) -> Vec<String> {
        let mut props: Vec<String> = Vec::new();
        let Some(inertia) = self.inertia else {
            return props;
        };
        for site in inertia.sites().iter().filter(|site| site.page == page) {
            let here = action.is_none_or(|(file, method)| {
                site.file == file && site.caller.as_deref() == Some(method)
            });
            if !here {
                continue;
            }
            for prop in site.props.iter().flatten() {
                if !props.contains(prop) {
                    props.push(prop.clone());
                }
            }
        }
        props
    }

    /// `page Pages/Events/Show.vue  props: event, can` and `uses Stamp, Dialog`.
    fn page_lines(&self, page: &str, props: &[String], lines: &mut Vec<String>) {
        let link = self.inertia.and_then(|inertia| inertia.resolve(page));
        let Some(link) = link else {
            lines.push(format!("page {page}"));
            return;
        };
        let mut line = format!("page {}", link.short);
        if !link.exists {
            line.push_str(self.lang.pick("  ! file missing", "  ! a fájl hiányzik"));
        }
        if !props.is_empty() {
            line.push_str(&format!("  props: {}", capped(props, MAX_NAMES)));
        }
        lines.push(line);
        if let Some(used) = self.components.get(&link.rel) {
            lines.push(format!("uses {}", capped(used, MAX_NAMES)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn closest_ranks_by_shared_tokens_and_caps_at_five() {
        let all = names(&[
            "route::events.show",
            "route::events.index",
            "route::password.reset",
            "route::events.store",
            "route::events.update",
            "route::events.destroy",
            "route::events.edit",
        ]);
        let near = closest("route::event.show", &all);
        assert_eq!(near.first().map(String::as_str), Some("route::events.show"));
        assert!(near.len() <= MAX_HINTS);
        assert!(closest("zzz", &all).is_empty());
        assert!(closest("route::", &all).is_empty());
    }

    #[test]
    fn caps_lines_and_names_without_cutting_a_name() {
        let lines: Vec<String> = (0..40).map(|n| format!("line {n}")).collect();
        let capped_lines = cap_lines(lines);
        assert_eq!(capped_lines.len(), MAX_LINES);
        assert_eq!(capped_lines.last().map(String::as_str), Some("+11"));
        assert_eq!(capped(&names(&["a", "b", "c"]), 2), "a, b, +1");
    }

    #[test]
    fn unknown_target_is_one_line_with_a_hint() {
        let line = unknown_line(
            "route::shelf.store",
            &names(&["route::shelves.store"]),
            Lang::En,
        );
        assert_eq!(
            line,
            "? unknown target 'route::shelf.store', closest: route::shelves.store"
        );
        assert!(!unknown_line("nothing", &[], Lang::En).contains("closest"));
    }
}
