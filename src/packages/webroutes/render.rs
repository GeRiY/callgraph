//! The lines of the `routes` / `ziggy` views.

use std::collections::HashSet;

use regex::Regex;

use super::{ClientRoute, ServerRoute, UrlUse, WebRoutes};
use crate::i18n::Lang;

/// Client callers shown per server route.
const MAX_CALLERS: usize = 3;
/// Unknown route names / unmatched URLs listed.
const MAX_WARNINGS: usize = 6;

impl WebRoutes {
    /// Client table, server table (with the client callers) and the warnings, for the queried files.
    pub(crate) fn lines(&self, lang: Lang) -> Vec<String> {
        let mut lines = Vec::new();
        let in_scope = |file: &str| self.scope.contains(file);
        let mut client: Vec<&ClientRoute> = self
            .client
            .iter()
            .filter(|route| in_scope(&route.file))
            .collect();
        client.sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));
        for route in &client {
            lines.push(client_line(route));
        }
        lines.extend(self.unknown_names(lang, !client.is_empty()));
        let mut server: Vec<&ServerRoute> = self
            .server
            .iter()
            .filter(|route| in_scope(&route.file))
            .collect();
        // the scans find `app.get(..)` routes before the `route("GET", ..)` helper calls: back to source order
        server.sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));
        let mut matched: HashSet<usize> = HashSet::new();
        for route in &server {
            let matcher = route_matcher(route);
            let callers: Vec<&UrlUse> = matcher
                .as_ref()
                .map(|re| {
                    self.urls
                        .iter()
                        .enumerate()
                        .filter(|(_, url)| url_matches(route, re, url))
                        .map(|(at, url)| {
                            matched.insert(at);
                            url
                        })
                        .collect()
                })
                .unwrap_or_default();
            lines.push(server_line(route, &callers));
        }
        if !server.is_empty() {
            lines.extend(self.unmatched_urls(lang, &matched));
        }
        lines
    }

    /// `⚠ unknown route name 'x'  file L12` lines: literal names the client uses that no route table defines. Only
    /// when a router table was found, and only for the queried files (or all of them when a table is queried).
    pub(crate) fn unknown_names(&self, lang: Lang, whole_project: bool) -> Vec<String> {
        if self.client.is_empty() || self.dynamic_routes {
            return Vec::new();
        }
        let known: HashSet<&str> = self
            .client
            .iter()
            .filter_map(|route| route.name.as_deref())
            .collect();
        let unknown: Vec<String> = self
            .names
            .iter()
            .filter(|usage| whole_project || self.scope.contains(&usage.file))
            .filter(|usage| !known.contains(usage.name.as_str()))
            .map(|usage| {
                format!(
                    "⚠ {} '{}'  {} L{}",
                    lang.pick("unknown route name", "ismeretlen route-név"),
                    usage.name,
                    usage.file,
                    usage.line
                )
            })
            .collect();
        capped(unknown, MAX_WARNINGS)
    }

    /// One line for the client URLs that look like calls to the project's own API (they share the first path
    /// segment with a server route) but match no server route.
    fn unmatched_urls(&self, lang: Lang, matched: &HashSet<usize>) -> Option<String> {
        let roots: HashSet<String> = self
            .server
            .iter()
            .filter_map(|route| first_segment(&route.path))
            .collect();
        let mut seen = HashSet::new();
        let mut items: Vec<String> = Vec::new();
        for (at, url) in self.urls.iter().enumerate() {
            if matched.contains(&at)
                || !first_segment(&url.path).is_some_and(|s| roots.contains(&s))
            {
                continue;
            }
            // matched by a server route outside the queried files: not a missing one
            if self
                .server
                .iter()
                .any(|route| route_matcher(route).is_some_and(|re| url_matches(route, &re, url)))
            {
                continue;
            }
            let key = (url.method.clone(), url.path.clone());
            if !seen.insert(key) {
                continue;
            }
            let method = url
                .method
                .as_deref()
                .map_or(String::new(), |m| format!("{m} "));
            items.push(format!("{method}{} ({} L{})", url.path, url.file, url.line));
        }
        if items.is_empty() {
            return None;
        }
        let total = items.len();
        items.truncate(4);
        let more = if total > 4 {
            format!(" +{}", total - 4)
        } else {
            String::new()
        };
        Some(format!(
            "⚠ {}: {}{more}",
            lang.pick("no server route", "nincs szerver-route"),
            items.join(", ")
        ))
    }
}

impl WebRoutes {
    /// The chain for one target (`trace`): the server routes (`GET /api/chats`, a path, or a handler name) with the
    /// client calls that reach them, and the client routes (name, path or component). Empty: nothing matches.
    pub(crate) fn trace_lines(&self, target: &str) -> Vec<String> {
        let target = target.trim();
        let target = target.strip_prefix("route:").unwrap_or(target);
        let spaced = target.replacen(':', " ", 1);
        let mut lines = Vec::new();
        for route in &self.client {
            let hit = route.name.as_deref() == Some(target)
                || route.path == target
                || route
                    .target
                    .as_deref()
                    .is_some_and(|t| t == target || t.ends_with(target));
            if hit {
                lines.push(format!(
                    "{}  {}:{}",
                    client_line(route),
                    route.file,
                    route.line
                ));
            }
        }
        // `GET /api/chats` (or a bare `/api/chats`) also reaches a regex route that matches the path
        let (want_method, want_path) = match spaced.split_once(' ') {
            Some((method, path)) if path.starts_with('/') => (Some(method), path),
            _ => (None, target),
        };
        for route in &self.server {
            let named = format!("{} {}", route.method, route.path);
            let matcher = route_matcher(route);
            let regex_hit = route.regex
                && want_path.starts_with('/')
                && want_method.is_none_or(|m| m.eq_ignore_ascii_case(&route.method))
                && matcher.as_ref().is_some_and(|re| re.is_match(want_path));
            let hit = named == spaced
                || route.path == target
                || regex_hit
                || route.handler == target
                || route.handler.ends_with(&format!("::{target}"));
            if !hit {
                continue;
            }
            let callers: Vec<&UrlUse> = matcher
                .as_ref()
                .map(|re| {
                    self.urls
                        .iter()
                        .filter(|url| url_matches(route, re, url))
                        .collect()
                })
                .unwrap_or_default();
            lines.push(format!(
                "{}  {}:{}",
                server_line(route, &callers),
                route.file,
                route.line
            ));
        }
        capped(lines, MAX_WARNINGS)
    }

    /// Every target `trace_lines` understands (for the "did you mean" hint).
    pub(crate) fn trace_candidates(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for route in &self.client {
            out.extend(route.name.clone());
            out.push(route.path.clone());
        }
        for route in &self.server {
            out.push(format!("{} {}", route.method, route.path));
        }
        out.sort();
        out.dedup();
        out
    }
}

fn capped(mut lines: Vec<String>, max: usize) -> Vec<String> {
    if lines.len() > max {
        let rest = lines.len() - max;
        lines.truncate(max);
        lines.push(format!("+{rest}"));
    }
    lines
}

fn client_line(route: &ClientRoute) -> String {
    let mut line = route.path.clone();
    if let Some(target) = &route.target {
        line.push_str(" → ");
        line.push_str(target);
    }
    if let Some(name) = &route.name {
        line.push_str(&format!(" ({name})"));
    }
    line
}

fn server_line(route: &ServerRoute, callers: &[&UrlUse]) -> String {
    let mut line = format!("{} {} → {}", route.method, route.path, route.handler);
    if !callers.is_empty() {
        let shown: Vec<String> = callers
            .iter()
            .take(MAX_CALLERS)
            .map(|url| format!("{} L{}", url.file, url.line))
            .collect();
        line.push_str(&format!("  ← {}", shown.join(", ")));
        if callers.len() > MAX_CALLERS {
            line.push_str(&format!(" +{}", callers.len() - MAX_CALLERS));
        }
    }
    line
}

/// The first literal path segment (`/api/chats/:id` → `api`; `^/chats/(?<id>..)$` → `chats`).
fn first_segment(path: &str) -> Option<String> {
    let rest = path.trim_start_matches('^').strip_prefix('/')?;
    let segment: String = rest
        .chars()
        .take_while(|c| c.is_alphanumeric() || matches!(c, '-' | '_'))
        .collect();
    (!segment.is_empty()).then_some(segment)
}

/// The route as a regex over a request path: a regex route as written, a string route with `:name` / `{name}` /
/// `[name]` segments as `[^/]+` and `*` as anything.
fn route_matcher(route: &ServerRoute) -> Option<Regex> {
    if route.regex {
        return Regex::new(&route.path).ok();
    }
    let path = route.path.trim_end_matches('/');
    let mut pattern = String::from("^");
    let mut any = false;
    for segment in path.split('/').skip(1) {
        any = true;
        pattern.push('/');
        if segment.starts_with(':') || segment.starts_with('{') || segment.starts_with('[') {
            pattern.push_str("[^/]+");
        } else if segment.starts_with('*') {
            pattern.push_str(".*");
        } else {
            pattern.push_str(&regex::escape(segment));
        }
    }
    if !any {
        pattern.push('/');
    }
    pattern.push_str("/?$");
    Regex::new(&pattern).ok()
}

/// Does the client URL hit the route (the whole path; a `:param` position is sampled as `1`).
fn url_matches(route: &ServerRoute, matcher: &Regex, url: &UrlUse) -> bool {
    let method_ok = matches!(route.method.as_str(), "ALL" | "*")
        || url.method.as_deref().is_none_or(|m| m == route.method);
    if !method_ok {
        return false;
    }
    let sample = url.path.replace(":param", "1");
    matcher
        .find(&sample)
        .is_some_and(|found| found.start() == 0 && found.end() == sample.len())
}
