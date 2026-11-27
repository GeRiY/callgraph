//! Framework links of a changed symbol, from the tables the `routes`, `inertia`, `forms` and `trace` views already
//! build (route table, Inertia render sites, validation rules): the routes whose handler is the changed controller
//! method, the Inertia pages it renders, the FormRequests / fields it validates; and for a changed Inertia page file
//! the render sites and their routes. Nothing is parsed in a new way; without Laravel / Inertia the facts are empty.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::analysis::Analyzer;
use crate::locate::ProjectSource;
use crate::packages::frameworks::laravel::Action;
use crate::packages::frameworks::{DiskProjectFiles, Inertia, Laravel};

/// A route in the output: `DELETE /organizations/{organization} (organizations.destroy)`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct RouteFact {
    pub verb: String,
    pub uri: String,
    pub name: Option<String>,
}

impl RouteFact {
    pub(crate) fn text(&self) -> String {
        match &self.name {
            Some(name) => format!("{} {} ({name})", self.verb, self.uri),
            None => format!("{} {}", self.verb, self.uri),
        }
    }
}

/// What the framework adds to one changed controller method.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Facts {
    pub routes: Vec<RouteFact>,
    /// Inertia pages the method renders (`Organizations/Edit`).
    pub pages: Vec<String>,
    /// FormRequest classes (short names) of the action.
    pub requests: Vec<String>,
    /// The validated fields, `name*` = required.
    pub rules: Vec<String>,
}

/// A changed Inertia page file: where it is rendered and which routes lead to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PageFact {
    pub name: String,
    pub rendered_by: Option<String>,
    pub routes: Vec<RouteFact>,
}

/// The scanned framework tables of the project.
pub(crate) struct Links {
    laravel: Option<Laravel>,
    inertia: Option<Inertia>,
    /// Class (full name) → project-relative file.
    classes: HashMap<String, String>,
}

/// The project-relative, `/`-separated path of a file.
fn relative(path: &Path, root: &Path, base: &Path) -> Option<String> {
    let stripped = path
        .strip_prefix(root)
        .ok()
        .or_else(|| path.strip_prefix(base).ok())?;
    Some(stripped.to_string_lossy().replace('\\', "/"))
}

fn route_fact(route: &crate::packages::frameworks::laravel::Route) -> RouteFact {
    RouteFact {
        verb: route.verb.clone(),
        uri: route.uri.clone(),
        name: route.name.clone(),
    }
}

impl Links {
    /// Scans the project (one pass over the project graph's slim facts); a project without Laravel and Inertia
    /// costs a detection only.
    pub(crate) fn load(root: &Path) -> Self {
        let packages = crate::packages::open(root);
        let mut inertia = Inertia::detect(root, &packages, &DiskProjectFiles);
        let mut laravel = Laravel::detect(root, &packages);
        let base = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        let mut classes: HashMap<String, String> = HashMap::new();
        if laravel.is_some() || inertia.is_some() {
            let analyzer = Analyzer::with_defaults();
            // a failed project graph only leaves the tables empty
            let _ = ProjectSource::new(root, &analyzer).with_files(|files| {
                let code = || {
                    files
                        .iter()
                        .filter(|file| file.code)
                        .map(|file| (file.path, file.graph))
                };
                if let Some(laravel) = laravel.as_mut() {
                    laravel.scan(code(), &DiskProjectFiles, false);
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
                    if !rel.ends_with(".php") {
                        continue;
                    }
                    if let Some(namespace) = file.graph.namespace.as_deref() {
                        for container in &file.graph.containers {
                            classes.insert(
                                format!("{}\\{}", namespace.trim_matches('\\'), container.name),
                                rel.clone(),
                            );
                        }
                    }
                }
            });
        }
        Self {
            laravel,
            inertia,
            classes,
        }
    }

    /// Is any framework table there at all?
    pub(crate) fn active(&self) -> bool {
        self.laravel.is_some() || self.inertia.is_some()
    }

    /// The facts of the method `class::method` defined in `file` (project-relative).
    pub(crate) fn of_method(&self, file: &str, class: &str, method: &str) -> Facts {
        let mut facts = Facts::default();
        if let Some(laravel) = &self.laravel {
            let routes = laravel.routes_of_action(class, method);
            // the full class name (the table keeps it) finds the controller file and its FormRequests
            let full = routes.iter().find_map(|route| match &route.action {
                Action::Controller { class: full, .. } => Some(full.clone()),
                _ => None,
            });
            for route in routes {
                let fact = route_fact(route);
                if !facts.routes.contains(&fact) {
                    facts.routes.push(fact);
                }
            }
            // validation matters for an action a route reaches
            if let Some(full) = full
                && let Some(action) =
                    laravel.action_facts(&full, method, &self.classes, &DiskProjectFiles)
            {
                facts.requests = action.requests;
                facts.rules = action
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
            }
        }
        if let Some(inertia) = &self.inertia {
            for site in inertia.sites() {
                let here = site.file == file
                    && site.caller.as_deref() == Some(method)
                    && site.container.as_deref().is_none_or(|c| c == class);
                if here && !facts.pages.contains(&site.page) {
                    facts.pages.push(site.page.clone());
                }
            }
        }
        facts
    }

    /// The Inertia page behind a changed file (`resources/js/Pages/Users/Index.vue`): its render sites and routes.
    pub(crate) fn of_page(&self, file: &str) -> Option<PageFact> {
        let inertia = self.inertia.as_ref()?;
        let name = inertia.page_name(file)?;
        let mut routes: Vec<RouteFact> = Vec::new();
        if let Some(laravel) = &self.laravel {
            let mut seen = HashSet::new();
            let mut add = |route: &crate::packages::frameworks::laravel::Route| {
                let fact = route_fact(route);
                if seen.insert(fact.clone()) {
                    routes.push(fact);
                }
            };
            for site in inertia.sites().iter().filter(|site| site.page == name) {
                if let (Some(class), Some(method)) = (&site.container, &site.caller) {
                    for route in laravel.routes_of_action(class, method) {
                        add(route);
                    }
                }
            }
            for route in laravel.routes_of_page(&name) {
                add(route);
            }
        }
        Some(PageFact {
            rendered_by: inertia.rendered_by(file),
            name,
            routes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_route_reads_verb_uri_and_name() {
        let named = RouteFact {
            verb: "DELETE".into(),
            uri: "/organizations/{organization}".into(),
            name: Some("organizations.destroy".into()),
        };
        assert_eq!(
            named.text(),
            "DELETE /organizations/{organization} (organizations.destroy)"
        );
        let bare = RouteFact {
            verb: "GET".into(),
            uri: "/".into(),
            name: None,
        };
        assert_eq!(bare.text(), "GET /");
    }

    #[test]
    fn a_project_without_frameworks_has_no_facts() {
        let dir =
            std::env::temp_dir().join(format!("callgraph-impact-links-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let links = Links::load(&dir);
        assert!(!links.active());
        assert_eq!(links.of_method("a.php", "A", "b"), Facts::default());
        assert!(links.of_page("a.vue").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
