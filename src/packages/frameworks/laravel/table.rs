//! A route-tábla kimenetei: a `routes` nézet sorai (ige, teljes URI, kezelő, név, middleware, `fájl:sor`), a
//! `middleware` nézet route-oszlopa és a konfigból generált route-ok Inertia-oldalai. Egy route-forrás: ugyanaz a
//! tábla, amelyből a `ziggy`, az `inertia` és az `forms` nézet is dolgozik.

use std::collections::{BTreeMap, BTreeSet};

use super::super::inertia::RenderSite;
use super::Laravel;
use super::routes::{Action, Route, short_class};

/// A `middleware` nézet egy middleware-jénél legfeljebb ennyi route-ot sorol (a többi `+N`).
const MAX_LISTED: usize = 3;

impl Laravel {
    /// Van-e route a táblában.
    pub(crate) fn has_routes(&self) -> bool {
        !self.table.routes.is_empty()
    }

    /// A route-fájlok, amelyekből a tábla épült (gyökérhez relatív utak).
    pub(crate) fn route_files(&self) -> BTreeSet<&str> {
        self.table
            .routes
            .iter()
            .map(|route| route.file.as_str())
            .collect()
    }

    /// A `routes` nézet sorai: azok a route-ok, amelyek fájlja a hatókörben van (`files`: a gyökérhez relatív
    /// utak), vagy amelyek kezelője a hatókör egyik osztálya (`classes`: teljes nevek).
    pub(crate) fn route_lines(&self, files: &[String], classes: &[String]) -> Vec<String> {
        self.table.routes
            .iter()
            .filter(|route| {
                files.contains(&route.file)
                    || matches!(&route.action, Action::Controller { class, .. } if classes.contains(class))
            })
            .map(route_line)
            .collect()
    }

    /// A `middleware` nézet route-oszlopa: middleware-enként a route-ok száma és az első néhány (`GET /uri`).
    /// Csak a hatókörbeli route-okból (lásd [`Self::route_lines`]).
    pub(crate) fn middleware_lines(&self, files: &[String], classes: &[String]) -> Vec<String> {
        let mut by_name: BTreeMap<&str, Vec<&Route>> = BTreeMap::new();
        for route in &self.table.routes {
            let in_scope = files.contains(&route.file)
                || matches!(&route.action, Action::Controller { class, .. } if classes.contains(class));
            if !in_scope {
                continue;
            }
            for name in &route.middleware {
                by_name.entry(name.as_str()).or_default().push(route);
            }
        }
        by_name
            .into_iter()
            .map(|(name, routes)| {
                let shown: Vec<String> = routes
                    .iter()
                    .take(MAX_LISTED)
                    .map(|route| format!("{} {}", route.verb, route.uri))
                    .collect();
                let more = match routes.len().saturating_sub(MAX_LISTED) {
                    0 => String::new(),
                    n => format!(", +{n}"),
                };
                format!(
                    "routes    {}  ×{}  {}{more}",
                    short_class(name),
                    routes.len(),
                    shown.join(", ")
                )
            })
            .collect()
    }

    /// A konfigból generált route-ok Inertia-oldalai (`'component' => 'Welcome'` a konfig-tömbben): render-helyként
    /// (a `routes/…` fájl és a route sora) kerülnek az `inertia` nézetbe, hogy az oldal ne legyen „nem renderelt”.
    pub(crate) fn page_sites(&self) -> Vec<RenderSite> {
        let mut out: Vec<RenderSite> = Vec::new();
        for route in &self.table.routes {
            let Some(page) = &route.page else {
                continue;
            };
            if out.iter().any(|site| &site.page == page) {
                continue;
            }
            out.push(RenderSite {
                file: route.file.clone(),
                container: None,
                caller: Some("__top__".to_owned()),
                line: route.line,
                page: page.clone(),
                props: None,
            });
        }
        out
    }
}

/// `route     GET  /orders/{order}  →  OrderController@show  name=orders.show  [auth, verified]  routes/web.php:12`
fn route_line(route: &Route) -> String {
    let action = match &route.action {
        Action::Controller { class, method } if method == "__invoke" => {
            short_class(class).to_owned()
        }
        other => other.label(),
    };
    let mut line = format!("route     {}  {}  →  {action}", route.verb, route.uri);
    if let Some(name) = &route.name {
        line.push_str(&format!("  name={name}"));
    }
    if !route.middleware.is_empty() {
        let names: Vec<&str> = route
            .middleware
            .iter()
            .map(|name| short_class(name))
            .collect();
        line.push_str(&format!("  [{}]", names.join(", ")));
    }
    line.push_str(&format!("  {}:{}", route.file, route.line));
    line
}
