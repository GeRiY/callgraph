use std::collections::HashSet;

use super::scan::{resolve_spec, scan_file, url_pattern};
use super::{ServerRoute, UrlUse, WebRoutes, join_paths};
use crate::i18n::Lang;

fn known(files: &[&str]) -> HashSet<String> {
    files.iter().map(|file| (*file).to_owned()).collect()
}

#[test]
fn router_table_joins_child_paths_and_resolves_components() {
    let src = "import Home from '../pages/Home.vue';\n\
        const routes = [\n\
          { path: '/', name: 'home', component: Home },\n\
          { path: '/web', component: () => import('@/layouts/Web.vue'), children: [\n\
            { path: ':id', name: 'web', component: () => import('../pages/Web.vue') },\n\
            { path: '', redirect: { name: 'home' } },\n\
          ] },\n\
        ];\n\
        export const router = createRouter({ history, routes });\n";
    let files = known(&[
        "client/src/pages/Home.vue",
        "client/src/pages/Web.vue",
        "client/src/layouts/Web.vue",
    ]);
    let facts = scan_file("client/src/router/index.ts", src, &files);
    let shown: Vec<String> = facts
        .client
        .iter()
        .map(|r| {
            format!(
                "{} {} {}",
                r.path,
                r.target.as_deref().unwrap_or("-"),
                r.name.as_deref().unwrap_or("-")
            )
        })
        .collect();
    assert_eq!(
        shown,
        [
            "/ client/src/pages/Home.vue home",
            "/web client/src/layouts/Web.vue -",
            "/web/:id client/src/pages/Web.vue web",
            "/web redirect name:home -",
        ]
    );
}

#[test]
fn route_names_come_from_push_replace_and_router_links() {
    let src = "<template>\n\
        <RouterLink :to=\"{ name: 'a' }\">a</RouterLink>\n\
        <router-link :to='{ name: \"b\", params: { id } }'>b</router-link>\n\
        </template>\n\
        <script setup>\n\
        router.push({ name: 'c' });\n\
        router.replace({ name: dynamic });\n\
        </script>\n";
    let facts = scan_file("x/Page.vue", src, &known(&[]));
    let names: Vec<&str> = facts.names.iter().map(|n| n.name.as_str()).collect();
    assert!(names.contains(&"a") && names.contains(&"b") && names.contains(&"c"));
    assert_eq!(names.len(), 3, "non-literal names are not collected");
}

#[test]
fn server_routes_name_inline_and_wrapped_handlers() {
    let src = "import { Hono } from 'hono';\n\
        import { list } from './chats';\n\
        import * as admin from './admin';\n\
        const app = new Hono();\n\
        app.get('/api/chats', list);\n\
        app.post('/api/chats', async (c) => c.json({}));\n\
        app.get('/api/admin', guarded(admin.stats, { role: 1 }));\n\
        const hit = cache.get('/not-a-route', 1);\n";
    let files = known(&["s/chats.ts", "s/admin.ts"]);
    let facts = scan_file("s/api.ts", src, &files);
    let routes: Vec<(String, String, String)> = facts
        .server
        .iter()
        .map(|r| (r.method.clone(), r.path.clone(), r.handler.clone()))
        .collect();
    assert_eq!(
        routes,
        [
            ("GET".into(), "/api/chats".into(), "s/chats.ts::list".into()),
            ("POST".into(), "/api/chats".into(), "s/api.ts:6".into()),
            (
                "GET".into(),
                "/api/admin".into(),
                "s/admin.ts::stats".into()
            ),
        ]
    );
}

#[test]
fn regex_helper_route_keeps_the_pattern_as_written() {
    let src = "route('GET', /^\\/chats\\/(?<chatId>[^/]+)\\/ws$/, chatSocket);\n\
        function chatSocket() {}\n";
    let facts = scan_file("s/routes.ts", src, &known(&[]));
    assert_eq!(facts.server.len(), 1);
    let route = &facts.server[0];
    assert!(route.regex);
    assert_eq!(route.path, "^/chats/(?<chatId>[^/]+)/ws$");
    assert_eq!(route.handler, "s/routes.ts::chatSocket");
}

#[test]
fn client_urls_become_param_patterns() {
    assert_eq!(url_pattern("'/api/chats'").as_deref(), Some("/api/chats"));
    assert_eq!(
        url_pattern("`/api/chats/${id}/x?y=1`").as_deref(),
        Some("/api/chats/:param/x")
    );
    assert_eq!(
        url_pattern("'/api/chats/' + id").as_deref(),
        Some("/api/chats/:param")
    );
    assert_eq!(url_pattern("`${base}/chats`"), None);
    assert_eq!(url_pattern("'https://x.test/api'"), None);
    assert_eq!(url_pattern("url"), None);
}

#[test]
fn fetch_method_comes_from_the_call() {
    let src = "fetch('/a');\n\
        fetch('/b', { method: 'post' });\n\
        axios.delete('/c');\n\
        fetch('/d', { method });\n";
    let facts = scan_file("c/api.ts", src, &known(&[]));
    let got: Vec<(&str, Option<&str>)> = facts
        .urls
        .iter()
        .map(|u| (u.path.as_str(), u.method.as_deref()))
        .collect();
    assert_eq!(
        got,
        [
            ("/a", Some("GET")),
            ("/b", Some("POST")),
            ("/c", Some("DELETE")),
            ("/d", None)
        ]
    );
}

#[test]
fn specifiers_resolve_relative_alias_and_index_files() {
    let files = known(&["a/src/x/y.ts", "a/src/z/index.ts", "a/src/w.vue"]);
    let from = "a/src/x/page.ts";
    assert_eq!(
        resolve_spec(from, "./y", &files).as_deref(),
        Some("a/src/x/y.ts")
    );
    assert_eq!(
        resolve_spec(from, "./y.js", &files).as_deref(),
        Some("a/src/x/y.ts")
    );
    assert_eq!(
        resolve_spec(from, "../z", &files).as_deref(),
        Some("a/src/z/index.ts")
    );
    assert_eq!(
        resolve_spec(from, "@/w.vue", &files).as_deref(),
        Some("a/src/w.vue")
    );
    assert_eq!(resolve_spec(from, "vue", &files), None);
}

#[test]
fn mount_prefix_joins_with_one_slash() {
    assert_eq!(join_paths("/api/", "/x"), "/api/x");
    assert_eq!(join_paths("/api", "/"), "/api");
}

fn web(server: Vec<ServerRoute>, urls: Vec<UrlUse>, file: &str) -> WebRoutes {
    WebRoutes {
        scope: HashSet::from([file.to_owned()]),
        server,
        urls,
        ..WebRoutes::default()
    }
}

fn route(method: &str, path: &str, regex: bool) -> ServerRoute {
    ServerRoute {
        file: "s/api.ts".into(),
        line: 1,
        method: method.into(),
        path: path.into(),
        regex,
        handler: "s/api.ts::h".into(),
        receiver: None,
    }
}

fn url(method: Option<&str>, path: &str, line: u32) -> UrlUse {
    UrlUse {
        file: "c/api.ts".into(),
        line,
        method: method.map(str::to_owned),
        path: path.into(),
    }
}

#[test]
fn routes_view_lists_callers_and_only_unmatched_own_api_urls() {
    let found = web(
        vec![
            route("GET", "/api/chats/:id", false),
            route("GET", "^/api/ws/(?<id>[^/]+)$", true),
        ],
        vec![
            url(Some("GET"), "/api/chats/:param", 7),
            url(Some("POST"), "/api/chats/:param", 8),
            url(Some("GET"), "/api/ws/:param", 9),
            url(Some("GET"), "/api/missing", 10),
            url(Some("GET"), "/elsewhere/x", 11),
        ],
        "s/api.ts",
    );
    let lines = found.lines(Lang::En);
    assert_eq!(lines[0], "GET /api/chats/:id → s/api.ts::h  ← c/api.ts L7");
    assert_eq!(
        lines[1],
        "GET ^/api/ws/(?<id>[^/]+)$ → s/api.ts::h  ← c/api.ts L9"
    );
    let missing = lines.last().unwrap();
    assert!(missing.contains("no server route"), "{lines:?}");
    assert!(missing.contains("POST /api/chats/:param") && missing.contains("GET /api/missing"));
    assert!(!missing.contains("/elsewhere"), "{missing}");
}

#[test]
fn no_missing_line_without_server_routes_in_scope() {
    let found = web(Vec::new(), vec![url(Some("GET"), "/api/x", 1)], "s/api.ts");
    assert!(found.lines(Lang::En).is_empty());
}
