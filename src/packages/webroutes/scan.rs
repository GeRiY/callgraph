//! Source-text extraction of the route facts of one file (see the parent module).

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use regex::Regex;

use super::{ClientRoute, FileFacts, Mount, NameUse, ServerRoute, UrlUse};
use crate::core::LineIndex;
use crate::core::srcscan::{Dialect, call_args, matching, split_args, split_top, strip_comments};

macro_rules! re {
    ($pattern:literal) => {{
        static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new($pattern).expect("valid regex"));
        &*RE
    }};
}

/// How far a wrapped handler call (`guarded(rest.list, {..})`) is read.
const MAX_CALL_SCAN: usize = 4000;

/// Receivers that are HTTP clients or plain maps, never routers.
const NOT_ROUTERS: [&str; 6] = ["axios", "http", "$http", "client", "req", "request"];

/// The imports of a file.
#[derive(Debug, Default)]
pub(super) struct Imports {
    /// local name → (resolved project file, imported name; `None` = default import)
    named: HashMap<String, (String, Option<String>)>,
    /// namespace import (`import * as rest from`) → resolved project file
    ns: HashMap<String, String>,
    /// local name → specifier as written
    specs: HashMap<String, String>,
}

/// Everything one file contributes.
pub(super) fn scan_file(rel: &str, raw: &str, known: &HashSet<String>) -> FileFacts {
    let mut facts = FileFacts::default();
    let is_vue = rel.ends_with(".vue");
    let text = if is_vue {
        raw.to_owned()
    } else {
        strip_comments(raw, Dialect::Js)
    };
    let lines = LineIndex::new(raw);
    let imports = parse_imports(&text, rel, known);
    facts.dynamic_routes = text.contains("router.addRoute(");
    scan_names(&text, rel, &lines, &mut facts.names);
    if text.contains("fetch")
        || text.contains("axios")
        || text.contains("http")
        || text.contains("WebSocket")
        || text.contains("EventSource")
    {
        scan_urls(&text, rel, &lines, &mut facts.urls);
    }
    if is_vue {
        return facts;
    }
    let stem = rel.rsplit('/').next().unwrap_or(rel);
    let react_router = text.contains("react-router")
        || text.contains("createBrowserRouter")
        || text.contains("createHashRouter");
    if text.contains("vue-router")
        || text.contains("createRouter")
        || text.contains("RouteRecordRaw")
        || react_router
        || stem.starts_with("routes.")
    {
        let ctx = Ctx {
            rel,
            imports: &imports,
            known,
            lines: &lines,
            incomplete: Cell::new(false),
            paths: RefCell::new(Vec::new()),
        };
        scan_client_routes(&text, &ctx, &mut facts.client);
        if react_router {
            react_router::scan(raw, &ctx, &mut facts);
        }
        facts.incomplete = ctx.incomplete.get();
        facts.paths = ctx.paths.into_inner();
    }
    scan_server(&text, rel, &imports, &lines, &mut facts);
    facts
}

struct Ctx<'a> {
    rel: &'a str,
    imports: &'a Imports,
    known: &'a HashSet<String>,
    lines: &'a LineIndex,
    /// A route path that is not a literal was seen: the table is not fully known.
    incomplete: Cell<bool>,
    /// Every route path (joined with the parents'), also of the routes that name no component.
    paths: RefCell<Vec<String>>,
}

mod react_router;

// ---------------------------------------------------------------------------------------------------------------
// imports and module resolution

fn is_ident(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '$')
}

/// `a.b.c` (identifiers separated by dots).
fn is_path(text: &str) -> bool {
    !text.is_empty() && text.split('.').all(is_ident)
}

fn parse_imports(text: &str, rel: &str, known: &HashSet<String>) -> Imports {
    let mut out = Imports::default();
    if !text.contains("import") {
        return out;
    }
    for caps in
        re!(r#"import\s+(?:type\s+)?([^'";]*?)\s*from\s*['"]([^'"]+)['"]"#).captures_iter(text)
    {
        let clause = caps[1].trim();
        let spec = &caps[2];
        let file = resolve_spec(rel, spec, known);
        if let Some(name) = clause.strip_prefix("* as ") {
            let name = name.trim();
            out.specs.insert(name.to_owned(), spec.to_owned());
            if let Some(file) = &file {
                out.ns.insert(name.to_owned(), file.clone());
            }
            continue;
        }
        let (default_part, braces) = match clause.find('{') {
            Some(i) => (
                clause[..i].trim().trim_end_matches(',').trim(),
                clause[i + 1..].split('}').next(),
            ),
            None => (clause, None),
        };
        if is_ident(default_part) {
            out.specs.insert(default_part.to_owned(), spec.to_owned());
            if let Some(file) = &file {
                out.named
                    .insert(default_part.to_owned(), (file.clone(), None));
            }
        }
        for item in braces.unwrap_or("").split(',') {
            let item = item.trim().trim_start_matches("type ").trim();
            let (orig, local) = match item.split_once(" as ") {
                Some((orig, local)) => (orig.trim(), local.trim()),
                None => (item, item),
            };
            if !is_ident(local) {
                continue;
            }
            out.specs.insert(local.to_owned(), spec.to_owned());
            if let Some(file) = &file {
                out.named
                    .insert(local.to_owned(), (file.clone(), Some(orig.to_owned())));
            }
        }
    }
    out
}

/// `a/b/../c` → `a/c`.
fn normalize(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

/// The directory part of a project-relative file.
fn dir_of(rel: &str) -> &str {
    rel.rsplit_once('/').map_or("", |(dir, _)| dir)
}

/// The deepest `src` directory above the file (`client/src/router/x.ts` → `client/src`).
fn nearest_src(rel: &str) -> Option<String> {
    let dir = dir_of(rel);
    let parts: Vec<&str> = dir.split('/').collect();
    let at = parts.iter().rposition(|part| *part == "src")?;
    Some(parts[..=at].join("/"))
}

/// A specifier to a project file: relative (`./x`, `../x`) or the `@/` / `~/` source-root alias; `None` for
/// packages and other aliases.
pub(super) fn resolve_spec(from: &str, spec: &str, known: &HashSet<String>) -> Option<String> {
    let base = if spec.starts_with("./") || spec.starts_with("../") {
        normalize(&format!("{}/{spec}", dir_of(from)))
    } else {
        let rest = spec
            .strip_prefix("@/")
            .or_else(|| spec.strip_prefix("~/"))?;
        normalize(&format!("{}/{rest}", nearest_src(from)?))
    };
    if known.contains(&base) {
        return Some(base);
    }
    // `./x.js` in TypeScript means `./x.ts`
    let stem = base
        .strip_suffix(".js")
        .or_else(|| base.strip_suffix(".mjs"))
        .unwrap_or(&base);
    for ext in ["ts", "tsx", "js", "jsx", "mjs", "vue"] {
        for candidate in [format!("{stem}.{ext}"), format!("{base}.{ext}")] {
            if known.contains(&candidate) {
                return Some(candidate);
            }
        }
    }
    for ext in ["ts", "tsx", "js", "jsx", "mjs"] {
        let candidate = format!("{base}/index.{ext}");
        if known.contains(&candidate) {
            return Some(candidate);
        }
    }
    None
}

// ---------------------------------------------------------------------------------------------------------------
// literals

/// The content of a plain string literal (no `${}` in a template literal).
fn string_lit(value: &str) -> Option<String> {
    let value = value.trim();
    let quote = value.chars().next()?;
    if !matches!(quote, '\'' | '"' | '`') || value.len() < 2 || !value.ends_with(quote) {
        return None;
    }
    let inner = &value[1..value.len() - 1];
    let bad = (inner.contains(quote) && !inner.contains(&format!("\\{quote}")))
        || (quote == '`' && inner.contains("${"));
    (!bad).then(|| inner.to_owned())
}

/// A regex literal `/.../flags` → its source (`\/` shown as `/`).
fn regex_lit(value: &str) -> Option<String> {
    let value = value.trim();
    let body = value.strip_prefix('/')?;
    let mut escaped = false;
    let mut in_class = false;
    for (i, ch) in body.char_indices() {
        if escaped {
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == '[' {
            in_class = true;
        } else if ch == ']' {
            in_class = false;
        } else if ch == '/' && !in_class {
            let flags = &body[i + 1..];
            if !flags.chars().all(|f| f.is_ascii_alphabetic()) {
                return None;
            }
            return Some(body[..i].replace("\\/", "/"));
        }
    }
    None
}

// ---------------------------------------------------------------------------------------------------------------
// client: vue-router / react-router tables

fn scan_client_routes(text: &str, ctx: &Ctx<'_>, out: &mut Vec<ClientRoute>) {
    let starts = [
        re!(r"\b(?:createBrowserRouter|createHashRouter|createMemoryRouter|useRoutes)\s*\(\s*\["),
        re!(r"\broutes\s*(?::\s*[\w<>\[\] ]+?)?\s*[:=]\s*\["),
        re!(r"\bexport\s+default\s*\["),
        re!(r"\bexport\s+const\s+\w+\s*(?::\s*[\w<>\[\] ]+?)?\s*=\s*\["),
    ];
    let mut seen = HashSet::new();
    for start in starts {
        for found in start.find_iter(text) {
            let open = found.end() - 1;
            if seen.insert(open) {
                parse_route_array(text, open, "", ctx, out);
            }
        }
    }
}

fn parse_route_array(
    text: &str,
    open: usize,
    parent: &str,
    ctx: &Ctx<'_>,
    out: &mut Vec<ClientRoute>,
) {
    let Some(close) = matching(text, open, text.len(), Dialect::Js) else {
        return;
    };
    for (start, end) in split_args(text, open + 1, close, Dialect::Js) {
        if text[start..end].starts_with('{') {
            parse_route_object(text, start, end, parent, ctx, out);
        }
    }
}

fn parse_route_object(
    text: &str,
    start: usize,
    end: usize,
    parent: &str,
    ctx: &Ctx<'_>,
    out: &mut Vec<ClientRoute>,
) {
    let mut path = None;
    let mut name = None;
    let mut target = None;
    let mut children = None;
    for (a, b) in split_args(text, start + 1, end.saturating_sub(1), Dialect::Js) {
        let entry = &text[a..b];
        let Some(key) = re!(r#"^(?:(\w+)|'([^']*)'|"([^"]*)")\s*:\s*"#).captures(entry) else {
            continue;
        };
        let key_name = key
            .get(1)
            .or_else(|| key.get(2))
            .or_else(|| key.get(3))
            .map_or("", |m| m.as_str());
        let value_at = key.get(0).map_or(0, |m| m.end());
        let value = &entry[value_at..];
        match key_name {
            "path" => {
                path = string_lit(value);
                if path.is_none() {
                    ctx.incomplete.set(true);
                }
            }
            "name" => name = string_lit(value),
            "component" | "Component" | "lazy" => target = component_text(value, ctx),
            "element" => target = element_text(value, ctx),
            "redirect" => target = Some(redirect_text(value)),
            "children" if value.starts_with('[') => children = Some(a + value_at),
            _ => {}
        }
    }
    let full = join_child(parent, path.as_deref().unwrap_or(""));
    ctx.paths.borrow_mut().push(if full.is_empty() {
        "/".to_owned()
    } else {
        full.clone()
    });
    if target.is_some() || name.is_some() {
        out.push(ClientRoute {
            file: ctx.rel.to_owned(),
            line: ctx.lines.line_of(start),
            path: if full.is_empty() {
                "/".to_owned()
            } else {
                full.clone()
            },
            name,
            target,
        });
    }
    if let Some(open) = children {
        parse_route_array(text, open, &full, ctx, out);
    }
}

/// A child path joined to the parent's (an absolute child path stands alone).
fn join_child(parent: &str, child: &str) -> String {
    if child.starts_with('/') || parent.is_empty() {
        child.to_owned()
    } else if child.is_empty() {
        parent.to_owned()
    } else {
        format!("{}/{child}", parent.trim_end_matches('/'))
    }
}

fn component_text(value: &str, ctx: &Ctx<'_>) -> Option<String> {
    let value = value.trim();
    if let Some(caps) = re!(r#"import\(\s*['"]([^'"]+)['"]\s*\)"#).captures(value) {
        let spec = &caps[1];
        return Some(resolve_spec(ctx.rel, spec, ctx.known).unwrap_or_else(|| spec.to_owned()));
    }
    if is_ident(value) {
        let shown = match ctx.imports.named.get(value) {
            Some((file, _)) => file.clone(),
            None => ctx
                .imports
                .specs
                .get(value)
                .cloned()
                .unwrap_or_else(|| value.to_owned()),
        };
        return Some(shown);
    }
    None
}

/// `<Home />` / `<Layout><Outlet /></Layout>` (react-router `element`): the component the tag names.
fn element_text(value: &str, ctx: &Ctx<'_>) -> Option<String> {
    let caps = re!(r"^\(?\s*<\s*([A-Z]\w*)").captures(value.trim())?;
    component_text(&caps[1], ctx)
}

fn redirect_text(value: &str) -> String {
    let value = value.trim();
    if let Some(path) = string_lit(value) {
        return format!("redirect {path}");
    }
    if value.starts_with('{')
        && let Some(caps) = re!(r#"\bname\s*:\s*['"]([^'"]+)['"]"#).captures(value)
    {
        return format!("redirect name:{}", &caps[1]);
    }
    "redirect (dynamic)".to_owned()
}

// ---------------------------------------------------------------------------------------------------------------
// client: route names used by the code

fn scan_names(text: &str, rel: &str, lines: &LineIndex, out: &mut Vec<NameUse>) {
    if !(text.contains("router") || text.contains("Router") || text.contains("to:")) {
        return;
    }
    let patterns = [
        re!(
            r#"\b(?:\$router|router|useRouter\(\))\s*\.\s*(?:push|replace|resolve)\s*\(\s*\{[^}]*?\bname\s*:\s*['"]([^'"]+)['"]\s*[,}]"#
        ),
        re!(
            r#"<(?:RouterLink|router-link)\b[^>]*?:to\s*=\s*"\{[^"]*?\bname\s*:\s*'([^']+)'\s*[,}]"#
        ),
        re!(
            r#"<(?:RouterLink|router-link)\b[^>]*?:to\s*=\s*'\{[^']*?\bname\s*:\s*"([^"]+)"\s*[,}]"#
        ),
        re!(r#"\bto\s*:\s*\{\s*name\s*:\s*['"]([^'"]+)['"]\s*[,}]"#),
    ];
    for pattern in patterns {
        for caps in pattern.captures_iter(text) {
            let at = caps.get(1).map_or(0, |m| m.start());
            out.push(NameUse {
                file: rel.to_owned(),
                line: lines.line_of(at),
                name: caps[1].to_owned(),
            });
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// client: request URLs

/// A URL expression as a path: string literals as written, template `${..}` parts and non-literal `+` operands as
/// `:param`; only a path that starts with a literal `/` (an internal API path) is kept, without the query string.
pub(super) fn url_pattern(arg: &str) -> Option<String> {
    let mut path = String::new();
    for part in split_top(arg.trim(), "+", Dialect::Js) {
        let part = part.trim();
        match part.chars().next() {
            Some('\'' | '"') => path.push_str(&string_lit(part)?),
            Some('`') if part.ends_with('`') && part.len() >= 2 => {
                path.push_str(&template_path(&part[1..part.len() - 1]));
            }
            _ => path.push_str(":param"),
        }
    }
    if !path.starts_with('/') || path.starts_with("//") || path.contains("://") {
        return None;
    }
    let path = path.split(['?', '#']).next().unwrap_or("");
    let path = if path.len() > 1 {
        path.trim_end_matches('/')
    } else {
        path
    };
    Some(path.to_owned())
}

/// A template literal body with every `${...}` replaced by `:param`.
fn template_path(body: &str) -> String {
    let mut out = String::new();
    let mut depth = 0usize;
    let mut chars = body.chars().peekable();
    while let Some(ch) = chars.next() {
        if depth > 0 {
            match ch {
                '{' => depth += 1,
                '}' => depth -= 1,
                _ => {}
            }
        } else if ch == '$' && chars.peek() == Some(&'{') {
            chars.next();
            depth = 1;
            out.push_str(":param");
        } else {
            out.push(ch);
        }
    }
    out
}

fn scan_urls(text: &str, rel: &str, lines: &LineIndex, out: &mut Vec<UrlUse>) {
    let callee = re!(
        r"\b(fetch|ofetch|\$fetch|useFetch|axios\.(get|post|put|patch|delete|head)|\$http\.(get|post|put|patch|delete)|http\.(get|post|put|patch|delete)|apiClient\.(get|post|put|patch|delete)|new\s+WebSocket|new\s+EventSource)\s*(\()"
    );
    for caps in callee.captures_iter(text) {
        let name = &caps[1];
        let Some(open_match) = caps.get(caps.len() - 1) else {
            continue;
        };
        let open = open_match.start();
        let Some(close) = matching(text, open, text.len(), Dialect::Js) else {
            continue;
        };
        let args = split_args(text, open + 1, close, Dialect::Js);
        let Some(&(a, b)) = args.first() else {
            continue;
        };
        let Some(path) = url_pattern(&text[a..b]) else {
            continue;
        };
        let method = if name.starts_with("new") {
            Some("GET".to_owned())
        } else if let Some((_, verb)) = name.rsplit_once('.') {
            Some(verb.to_ascii_uppercase())
        } else {
            fetch_method(args.get(1).map(|&(a, b)| &text[a..b]))
        };
        out.push(UrlUse {
            file: rel.to_owned(),
            line: lines.line_of(a),
            method,
            path,
        });
    }
}

/// The method of a `fetch(url, options)` call: `GET` without options, the literal `method`, else unknown.
fn fetch_method(options: Option<&str>) -> Option<String> {
    let Some(options) = options else {
        return Some("GET".to_owned());
    };
    if let Some(caps) = re!(r#"\bmethod\s*:\s*['"](\w+)['"]"#).captures(options) {
        return Some(caps[1].to_ascii_uppercase());
    }
    (!options.contains("method")).then(|| "GET".to_owned())
}

// ---------------------------------------------------------------------------------------------------------------
// server: route tables

fn scan_server(text: &str, rel: &str, imports: &Imports, lines: &LineIndex, facts: &mut FileFacts) {
    let server_like = re!(
        r#"(?:from\s*|require\(\s*)['"](?:hono|express|itty-router|fastify|@koa/router|koa-router|elysia|@hono/[^'"]*)['"]"#
    )
    .is_match(text);
    if server_like {
        let mut last: Option<String> = None;
        for caps in re!(
            r"(?:([A-Za-z_$][\w$]*)|(\)))\s*\.\s*(get|post|put|patch|delete|options|head|all)\s*\("
        )
        .captures_iter(text)
        {
            let receiver = match caps.get(1) {
                Some(name) if NOT_ROUTERS.contains(&name.as_str()) => continue,
                Some(name) => {
                    last = Some(name.as_str().to_owned());
                    last.clone()
                }
                None => match &last {
                    Some(name) => Some(name.clone()),
                    None => continue,
                },
            };
            let whole = caps.get(0).map_or(0..0, |m| m.range());
            let method = caps[3].to_ascii_uppercase();
            add_route(
                text,
                whole.end - 1,
                0,
                &method,
                receiver,
                (rel, imports, lines),
                &mut facts.server,
            );
        }
        for caps in re!(
            r#"([A-Za-z_$][\w$]*)\s*\.\s*(?:route|use)\s*\(\s*['"](/[^'"]*)['"]\s*,\s*([A-Za-z_$][\w$]*)\s*\)"#
        )
        .captures_iter(text)
        {
            if caps[1] == caps[3] {
                continue;
            }
            facts.mounts.push(Mount {
                file: rel.to_owned(),
                prefix: caps[2].to_owned(),
                sub: caps[3].to_owned(),
                sub_file: imports.named.get(&caps[3]).map(|(file, _)| file.clone()),
            });
        }
    }
    // the plain helper: route("GET", /regex/ | "/path", handler)
    for caps in re!(
        r#"([A-Za-z_$][\w$.]*)\s*\(\s*['"](GET|POST|PUT|PATCH|DELETE|OPTIONS|HEAD|ALL)['"]\s*,"#
    )
    .captures_iter(text)
    {
        let whole = caps.get(0).map_or(0..0, |m| m.range());
        let paren = whole.start + text[whole.clone()].find('(').unwrap_or(0);
        add_route(
            text,
            paren,
            1,
            &caps[2],
            None,
            (rel, imports, lines),
            &mut facts.server,
        );
    }
}

/// Reads the call whose `(` is at `open`: the path is argument `path_index`, the handler the last argument.
fn add_route(
    text: &str,
    open: usize,
    path_index: usize,
    method: &str,
    receiver: Option<String>,
    (rel, imports, lines): (&str, &Imports, &LineIndex),
    out: &mut Vec<ServerRoute>,
) {
    let Some(close) = matching(text, open, text.len(), Dialect::Js) else {
        return;
    };
    let args = split_args(text, open + 1, close, Dialect::Js);
    if args.len() <= path_index + 1 {
        return;
    }
    let (pa, pb) = args[path_index];
    let path_arg = &text[pa..pb];
    let (path, regex) = if let Some(source) = regex_lit(path_arg) {
        (source, true)
    } else if let Some(value) = string_lit(path_arg).filter(|v| v.starts_with('/')) {
        (value, false)
    } else {
        return;
    };
    let (ha, hb) = args[args.len() - 1];
    // `cache.get('/x', 1)` / `axios.get('/x', { params })`: the last argument is not a handler
    if text[ha..hb]
        .chars()
        .next()
        .is_none_or(|c| c.is_ascii_digit() || matches!(c, '\'' | '"' | '`' | '{' | '['))
    {
        return;
    }
    let handler = handler_text(&text[ha..hb], lines.line_of(ha), text, rel, imports, 0);
    let route = ServerRoute {
        file: rel.to_owned(),
        line: lines.line_of(open),
        method: method.to_owned(),
        path,
        regex,
        handler,
        receiver,
    };
    if !out.contains(&route) {
        out.push(route);
    }
}

/// The handler expression as `file::function`, `file:line` (inline function) or as written.
fn handler_text(
    arg: &str,
    line: u32,
    text: &str,
    rel: &str,
    imports: &Imports,
    depth: usize,
) -> String {
    let arg = arg.trim();
    if is_path(arg) {
        return resolve_handler(arg, text, rel, imports);
    }
    let inline = arg.starts_with("async")
        || arg.starts_with("function")
        || arg.starts_with('(')
        || arg.contains("=>");
    if !inline
        && depth < 2
        && let Some(open) = arg.find('(')
        && is_path(arg[..open].trim())
        && let Some(inner) = call_args(&arg[open + 1..], Dialect::Js, MAX_CALL_SCAN)
        && let Some(first) = inner.first()
        && !first.is_empty()
    {
        // a wrapper such as `guarded(handler, opts)`: the first argument is the handler
        return handler_text(first, line, text, rel, imports, depth + 1);
    }
    format!("{rel}:{line}")
}

fn resolve_handler(name: &str, text: &str, rel: &str, imports: &Imports) -> String {
    if let Some((owner, member)) = name.split_once('.') {
        if let Some(file) = imports.ns.get(owner) {
            return format!("{file}::{member}");
        }
        if let Some((file, _)) = imports.named.get(owner) {
            return format!("{file}::{name}");
        }
        return name.to_owned();
    }
    if let Some((file, orig)) = imports.named.get(name) {
        return match orig {
            Some(orig) => format!("{file}::{orig}"),
            None => file.clone(),
        };
    }
    let declared = Regex::new(&format!(
        r"(?:\bfunction\s+|\b(?:const|let|var)\s+){}\b",
        regex::escape(name)
    ))
    .is_ok_and(|re| re.is_match(text));
    if declared {
        format!("{rel}::{name}")
    } else {
        name.to_owned()
    }
}
