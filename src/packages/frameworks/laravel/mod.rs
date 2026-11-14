//! Laravel: a projekt route-táblája (`routes/*.php`) és a JS / Vue Ziggy `route('név')` hívások kötése hozzá.
//! Facade: `Laravel` — a route-tábla (`routes.rs`, a route-fájlok forrásából: csoportok, resource-ok, nevek), és ha a
//! kérés igényli, a projekt `route(..)` hivatkozásai. A kimenetek: az outline `route::név → POST /uri · X@m` linkje a
//! JS-hívásnál (hiányzó névnél `⚠ no route`), a controller-metódus `route:` sora, a json `route` mezője és a `ziggy`
//! nézet (`view.rs`: ismeretlen route-nevek). Az Inertia `useForm` űrlap-szerződés (`forms.rs` + `rules.rs`): a küldött
//! mezők ↔ a controller validációja — `form:` sor az outline-ban, `form` mező a json-ban, `forms` nézet. Nem új
//! szekció: link — ezért kevés token. Az Eloquent-modell összefoglaló (`models.rs` + `schema.rs`): a tábla oszlopai a
//! migrációkból, fillable / casts / relációk — `table:` sor az outline-ban, a reláció célja, json `model` mező és a
//! `models` nézet.

mod forms;
mod models;
pub(crate) mod preset;
mod routes;
mod rules;
mod schema;
mod view;

#[cfg(test)]
mod forms_tests;
#[cfg(test)]
mod models_tests;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::files::ProjectFiles;
use super::inertia::{Inertia, render};
use crate::i18n::Lang;
use crate::model::{Call, FileGraph};
use crate::packages::Packages;
use forms::Form;
use models::ModelData;
pub(crate) use models::extends_model_base;
use routes::{Action, Route};

/// A framework neve a `FRAMEWORKS` regiszterben (`laravel/framework`): ez kapcsolja be a stratégiát.
pub(crate) const NAME: &str = "laravel";

/// A route-fájlok mappája a gyökérhez relatívan.
const ROUTES_DIR: &str = "routes/";
/// A controller-metódus `route:` sorában legfeljebb ennyi route (a többi `+N`).
const MAX_ROUTES: usize = 3;

/// Egy `route('név')` hivatkozás a projektben (a ziggy nézethez).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Reference {
    /// A fájl a gyökérhez relatívan.
    file: String,
    line: u32,
    name: String,
    /// JS / Vue hívás (különben PHP: `route()`, `to_route()`, `redirect()->route()`).
    js: bool,
}

/// A Laravel-kötések egy projektre.
#[derive(Debug, Clone)]
pub(crate) struct Laravel {
    /// A projekt gyökere, és a kanonikus alakja (a sorok útja bármelyikkel kezdődhet).
    root: PathBuf,
    canonical: Option<PathBuf>,
    /// A route-tábla (a `scan` után; előtte üres).
    routes: Vec<Route>,
    /// Név → a `routes` indexe (az utolsó definíció nyer, mint a Laravelben).
    by_name: HashMap<String, usize>,
    /// A `route(..)` hivatkozások (csak a `references` kéréssel végzett `scan` után).
    refs: Vec<Reference>,
    /// Az ellenőrzött `useForm` küldések (csak a `scan_forms` után).
    forms: Vec<Form>,
    /// Az Eloquent-modellek és a migrációk sémája (csak a `scan_models` után).
    models: Option<ModelData>,
}

/// A hívás route-neve: Ziggy `route('név')` JS-hívás string-literál névvel (a gráf `resolved`-je `route::név`);
/// dinamikus névnél, `route()`-nál és a `route().current('x.*')` mintáknál `None`.
pub(crate) fn route_name(call: &Call) -> Option<&str> {
    if call.category != "route_helper" || call.target.contains('.') {
        return None;
    }
    let name = call.resolved.as_deref()?.strip_prefix("route::")?;
    (!name.is_empty() && !name.contains('*')).then_some(name)
}

/// PHP-oldali route-hivatkozás hívott neve: `route(..)`, `to_route(..)`, `redirect()->route(..)`,
/// `Redirect::route(..)` / `URL::route(..)`; a `$request->route('param')` nem (az paraméter-név, nem route-név).
fn php_reference_fn(call: &Call) -> Option<&'static str> {
    let target = call.target.trim_start_matches('\\');
    match call.kind.as_str() {
        "func_call" => match target.rsplit('\\').next()?.to_ascii_lowercase().as_str() {
            "route" => Some("route"),
            "to_route" => Some("to_route"),
            _ => None,
        },
        "method_call" => {
            (target == "route" && call.on.as_deref() == Some("<expr>")).then_some("route")
        }
        "static_call" => {
            let (class, method) = target.rsplit_once("::")?;
            let class = class.rsplit('\\').next()?;
            (method.eq_ignore_ascii_case("route")
                && (class.eq_ignore_ascii_case("redirect") || class.eq_ignore_ascii_case("url")))
            .then_some("route")
        }
        _ => None,
    }
}

impl Laravel {
    /// Csak ha a projekt Laravelt használ (a `Packages::frameworks()` szerint).
    pub(crate) fn detect(root: &Path, packages: &Packages) -> Option<Self> {
        packages
            .frameworks()
            .contains(&NAME)
            .then(|| Self::new(root))
    }

    pub(crate) fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            canonical: root.canonicalize().ok().filter(|c| c != root),
            routes: Vec::new(),
            by_name: HashMap::new(),
            refs: Vec::new(),
            forms: Vec::new(),
            models: None,
        }
    }

    /// A route-tábla a `routes/**/*.php` fájlokból (a forrásukból; az osztálynevek a fájl `use`-aiból). `references`
    /// esetén a projekt `route(..)` hivatkozásai is: a JS / Vue hívásoknál a gráf `resolved`-je, a PHP-nál a
    /// hívás sorának forrása.
    pub(crate) fn scan<'g>(
        &mut self,
        graphs: impl IntoIterator<Item = (&'g Path, &'g FileGraph)>,
        files: &dyn ProjectFiles,
        references: bool,
    ) {
        for (path, graph) in graphs {
            let Some(rel) = self.relative_path(path) else {
                continue;
            };
            if rel.starts_with(ROUTES_DIR) && rel.ends_with(".php") {
                if let Some(text) = files.read(path) {
                    self.routes
                        .extend(routes::parse_file(&rel, &text, &graph.imports));
                }
            } else if references {
                self.collect_references(&rel, path, graph, files);
            }
        }
        self.by_name.clear();
        for (index, route) in self.routes.iter().enumerate() {
            if let Some(name) = &route.name {
                self.by_name.insert(name.clone(), index);
            }
        }
    }

    fn collect_references(
        &mut self,
        rel: &str,
        path: &Path,
        graph: &FileGraph,
        files: &dyn ProjectFiles,
    ) {
        if graph.module.is_some() {
            for call in &graph.calls {
                if let Some(name) = route_name(call) {
                    self.refs.push(Reference {
                        file: rel.to_owned(),
                        line: call.line,
                        name: name.to_owned(),
                        js: true,
                    });
                }
            }
            return;
        }
        if !rel.ends_with(".php") {
            return;
        }
        let candidates: Vec<(&Call, &str)> = graph
            .calls
            .iter()
            .filter_map(|call| Some((call, php_reference_fn(call)?)))
            .collect();
        if candidates.is_empty() {
            return;
        }
        let Some(text) = files.read(path) else {
            return;
        };
        for (call, name) in candidates {
            let literal = render::source_args_named(&text, call.line, name)
                .and_then(|args| render::literal(args.first()?));
            if let Some(name) = literal.filter(|n| !n.contains('*')) {
                self.refs.push(Reference {
                    file: rel.to_owned(),
                    line: call.line,
                    name,
                    js: false,
                });
            }
        }
    }

    /// A (sorokban álló, abszolút vagy relatív) fájl útja a gyökérhez relatívan, `/`-rel.
    pub(crate) fn rel(&self, file: &str) -> Option<String> {
        self.relative_path(Path::new(file))
    }

    fn relative_path(&self, path: &Path) -> Option<String> {
        let rel = if path.is_absolute() {
            path.strip_prefix(&self.root)
                .ok()
                .or_else(|| path.strip_prefix(self.canonical.as_ref()?).ok())?
        } else {
            path
        };
        let text = rel.to_string_lossy().replace('\\', "/");
        let mut text = text.as_str();
        while let Some(rest) = text.strip_prefix("./") {
            text = rest;
        }
        Some(text.to_owned())
    }

    /// A tábla route-jai (a `scan` után).
    #[cfg(test)]
    pub(crate) fn routes(&self) -> &[Route] {
        &self.routes
    }

    /// Egy (teljes gráfbeli) hívás route-ja: `None`, ha nem literál nevű `route(..)` hívás vagy a tábla üres;
    /// `Some(None)`, ha a név nem létezik.
    pub(crate) fn lookup(&self, call: &Call) -> Option<Option<&Route>> {
        if self.routes.is_empty() {
            return None;
        }
        let name = route_name(call)?;
        Some(self.by_name.get(name).map(|&index| &self.routes[index]))
    }

    /// Az outline a hívás célja után fűzött link: ` → POST /register · RegisteredUserController@store`, hiányzó
    /// névnél ` ⚠ no route`. A page-akciónál a page-fájl (`Pages/Dashboard.vue`), ha az Inertia-kötések ismerik.
    ///
    /// Ha a `route(..)` egy küldés argumentuma (`form.post(route('register'))` ugyanazon a soron, `siblings`), és a
    /// nevesített route igéje más, az azonos URI-jú, a küldés igéjével egyező route jön (a Breeze a `GET /register`-t
    /// nevezi el, a `POST /register` névtelen — az űrlap viszont oda küld).
    pub(crate) fn link_text(
        &self,
        call: &Call,
        siblings: &[Call],
        inertia: Option<&Inertia>,
        lang: Lang,
    ) -> Option<String> {
        Some(match self.lookup(call)? {
            Some(route) => format!(
                " → {}",
                describe(self.by_submit(route, call, siblings), inertia)
            ),
            None => format!(" {}", lang.pick("⚠ no route", "⚠ nincs ilyen route")),
        })
    }

    /// A küldés igéjéhez illő route (lásd `link_text`); ha nincs küldés vagy nincs ilyen route, maga a `route`.
    fn by_submit<'a>(&'a self, route: &'a Route, call: &Call, siblings: &[Call]) -> &'a Route {
        match submit_verb(call, siblings) {
            Some(verb) => self.for_verb(route, verb),
            None => route,
        }
    }

    /// A route, vagy ha az igéje nem fogadja a `verb`-et, az azonos URI-jú, az igét fogadó route (ha nincs: a `route`).
    fn for_verb<'a>(&'a self, route: &'a Route, verb: &str) -> &'a Route {
        if has_verb(&route.verb, verb) {
            return route;
        }
        self.routes
            .iter()
            .find(|other| other.uri == route.uri && has_verb(&other.verb, verb))
            .unwrap_or(route)
    }

    /// A controller-metódus `route:` sora: `route: POST /register (register), GET /x +2` (legfeljebb 3 route, a többi
    /// `+N`); ha a metódus nem route-akció: `None`. `namespace` + `container`: a controller teljes neve.
    pub(crate) fn route_line(
        &self,
        namespace: Option<&str>,
        container: &str,
        method: &str,
    ) -> Option<String> {
        let full = match namespace.filter(|ns| !ns.is_empty()) {
            Some(ns) => format!("{}\\{container}", ns.trim_matches('\\')),
            None => container.to_owned(),
        };
        let hits: Vec<&Route> = self
            .routes
            .iter()
            .filter(|route| match &route.action {
                Action::Controller { class, method: m } => {
                    m == method && (*class == full || (!class.contains('\\') && class == container))
                }
                _ => false,
            })
            .collect();
        if hits.is_empty() {
            return None;
        }
        let shown: Vec<String> = hits
            .iter()
            .take(MAX_ROUTES)
            .map(|route| match &route.name {
                Some(name) => format!("{} {} ({name})", route.verb, route.uri),
                None => format!("{} {}", route.verb, route.uri),
            })
            .collect();
        let mut line = format!("route: {}", shown.join(", "));
        if hits.len() > MAX_ROUTES {
            line.push_str(&format!(", +{}", hits.len() - MAX_ROUTES));
        }
        Some(line)
    }
}

/// `POST /register · RegisteredUserController@store` (a page-akciónál a page-fájl rövid alakja, ha ismert).
fn describe(route: &Route, inertia: Option<&Inertia>) -> String {
    let action = match &route.action {
        Action::Page(page) => inertia
            .and_then(|inertia| inertia.resolve(page))
            .map_or_else(|| route.action.label(), |link| link.short),
        other => other.label(),
    };
    format!("{} {} · {action}", route.verb, route.uri)
}

/// A `route(..)` hívással azonos sorú küldés igéje (`form.post(..)`, `router.put(..)`, `useForm().delete(..)`).
fn submit_verb(call: &Call, siblings: &[Call]) -> Option<&'static str> {
    siblings
        .iter()
        .filter(|other| other.line == call.line && !std::ptr::eq(*other, call))
        .find_map(|other| {
            let target = other.resolved.as_deref().unwrap_or(&other.target);
            match target
                .rsplit(['.', ':'])
                .next()?
                .to_ascii_lowercase()
                .as_str()
            {
                "post" => Some("POST"),
                "put" => Some("PUT"),
                "patch" => Some("PATCH"),
                "delete" => Some("DELETE"),
                _ => None,
            }
        })
}

/// A route igéje (`PUT|PATCH`, `ANY`) elfogadja-e a küldés igéjét.
fn has_verb(route_verb: &str, verb: &str) -> bool {
    route_verb == "ANY" || route_verb.split('|').any(|v| v == verb)
}
