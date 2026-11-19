//! TypeScript / JavaScript route tables and the links between them (`--format routes`, `--format ziggy` without
//! Laravel):
//!
//! - client: `createRouter({ routes: [...] })` (vue-router; nested `children`, lazy `import()`, `redirect`) and the
//!   route names used by `router.push({ name })` / `<RouterLink :to="{ name }">` (unknown literal names are flagged);
//!   and react-router (`createBrowserRouter([...])`, `<Route path element>` trees, `useRoutes`), whose literal
//!   `<Link to>` / `<Navigate to>` / `navigate('/x')` paths are checked against the defined paths;
//! - server: Hono / Express / itty style `app.get('/x', handler)`, `app.route('/p', sub)` mounts and the plain
//!   `route("GET", /regex/, handler)` helper pattern; the handler is resolved to `file::function` through the
//!   file's imports, an inline function prints as `file:line`;
//! - "who calls this endpoint": client `fetch` / `axios` / `ofetch` URL literals (template literals and `+`
//!   concatenations become `:param`) matched against the server route patterns.
//!
//! The facts are read from the source text with the shared scanners (`core::srcscan`), once per query and only when
//! the view asks for them. Everything is conservative: only literal paths and names count, a non-literal part never
//! produces a warning, and the "no server route" line only appears when server routes were found.

mod render;
mod scan;
#[cfg(test)]
mod tests;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::io::FileWalker;
use crate::query::QueryResult;

/// The scanned source kinds.
const EXTS: [&str; 6] = ["vue", "js", "jsx", "ts", "tsx", "mjs"];

/// Files bigger than this are not read.
const MAX_BYTES: u64 = 1_500_000;

/// One client route (`path`, already joined with the parents' paths).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ClientRoute {
    pub(crate) file: String,
    pub(crate) line: u32,
    pub(crate) path: String,
    pub(crate) name: Option<String>,
    /// Component file (or the specifier as written), or `redirect ...`; `None` for a group / name-only record.
    pub(crate) target: Option<String>,
}

/// One server route.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ServerRoute {
    pub(crate) file: String,
    pub(crate) line: u32,
    /// Upper case (`GET`, ... or `ALL`).
    pub(crate) method: String,
    /// The path as written (a regex source when `regex`), with the mount prefix applied to string paths.
    pub(crate) path: String,
    pub(crate) regex: bool,
    /// `file::function`, `file:line` (inline function) or the expression as written.
    pub(crate) handler: String,
    /// The router variable (`app`, `api`, ...) the route was declared on.
    pub(crate) receiver: Option<String>,
}

/// A literal route name used by the client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NameUse {
    pub(crate) file: String,
    pub(crate) line: u32,
    pub(crate) name: String,
}

/// A client request URL (a literal path, `:param` for the dynamic parts).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UrlUse {
    pub(crate) file: String,
    pub(crate) line: u32,
    /// `None`: the method is not a literal.
    pub(crate) method: Option<String>,
    pub(crate) path: String,
}

/// A literal path the client links or navigates to (`<Link to="/x">`, `navigate("/x")`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PathUse {
    pub(crate) file: String,
    pub(crate) line: u32,
    pub(crate) path: String,
}

/// `app.route('/prefix', sub)` / `app.use('/prefix', router)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Mount {
    pub(crate) file: String,
    pub(crate) prefix: String,
    pub(crate) sub: String,
    /// The file the (imported) sub-router comes from; `None`: it is declared in `file`.
    pub(crate) sub_file: Option<String>,
}

/// What one source file contributes.
#[derive(Debug, Default)]
pub(crate) struct FileFacts {
    pub(crate) client: Vec<ClientRoute>,
    pub(crate) server: Vec<ServerRoute>,
    pub(crate) mounts: Vec<Mount>,
    pub(crate) names: Vec<NameUse>,
    pub(crate) urls: Vec<UrlUse>,
    /// `router.addRoute(...)` is used: route names can be added at run time.
    pub(crate) dynamic_routes: bool,
    /// Every client route path (also of routes without a component).
    pub(crate) paths: Vec<String>,
    /// The client route table holds a path that is not a literal.
    pub(crate) incomplete: bool,
    /// Literal client links / navigations (react-router).
    pub(crate) links: Vec<PathUse>,
}

/// The project-level route facts of the queried scope.
#[derive(Debug, Default)]
pub(crate) struct WebRoutes {
    /// Project-relative files of the queried rows (JS / TS / Vue).
    scope: HashSet<String>,
    client: Vec<ClientRoute>,
    server: Vec<ServerRoute>,
    names: Vec<NameUse>,
    urls: Vec<UrlUse>,
    dynamic_routes: bool,
    paths: Vec<String>,
    incomplete: bool,
    links: Vec<PathUse>,
}

impl WebRoutes {
    /// Scan the project below `root`.
    pub(crate) fn open(root: &Path, base: &Path, scope: HashSet<String>) -> Self {
        let walker = FileWalker::for_extensions(EXTS.to_vec());
        let files: Vec<(String, PathBuf)> = walker
            .collect(&[root.to_path_buf()], &mut |_| {})
            .into_iter()
            .filter_map(|path| {
                let rel = relative(&path, root, base)?;
                Some((rel, path))
            })
            .collect();
        let known: HashSet<String> = files.iter().map(|(rel, _)| rel.clone()).collect();
        let mut out = Self {
            scope,
            ..Self::default()
        };
        let mut mounts = Vec::new();
        for (rel, path) in &files {
            let Some(text) = read(path) else { continue };
            let facts = scan::scan_file(rel, &text, &known);
            out.client.extend(facts.client);
            out.server.extend(facts.server);
            out.names.extend(facts.names);
            out.urls.extend(facts.urls);
            out.dynamic_routes |= facts.dynamic_routes;
            out.paths.extend(facts.paths);
            out.incomplete |= facts.incomplete;
            out.links.extend(facts.links);
            mounts.extend(facts.mounts);
        }
        out.apply_mounts(&mounts);
        out
    }

    /// Nothing worth showing was found.
    pub(crate) fn is_empty(&self) -> bool {
        self.client.is_empty() && self.server.is_empty()
    }

    /// A mounted sub-router's string paths get the mount prefix.
    fn apply_mounts(&mut self, mounts: &[Mount]) {
        for mount in mounts {
            for route in self.server.iter_mut().filter(|route| !route.regex) {
                let hit = match &mount.sub_file {
                    Some(file) => route.file == *file,
                    None => {
                        route.file == mount.file
                            && route.receiver.as_deref() == Some(mount.sub.as_str())
                    }
                };
                if hit {
                    route.path = join_paths(&mount.prefix, &route.path);
                }
            }
        }
    }
}

/// `prefix` + `path` with exactly one `/` between them.
pub(crate) fn join_paths(prefix: &str, path: &str) -> String {
    let prefix = prefix.trim_end_matches('/');
    if path == "/" {
        return if prefix.is_empty() {
            "/".to_owned()
        } else {
            prefix.to_owned()
        };
    }
    format!("{prefix}/{}", path.trim_start_matches('/'))
}

fn read(path: &Path) -> Option<String> {
    if std::fs::metadata(path).ok()?.len() > MAX_BYTES {
        return None;
    }
    std::fs::read_to_string(path).ok()
}

/// The project-relative, `/`-separated path.
fn relative(path: &Path, root: &Path, base: &Path) -> Option<String> {
    let stripped = path
        .strip_prefix(base)
        .ok()
        .or_else(|| path.strip_prefix(root).ok());
    let rel = match stripped {
        Some(rel) => rel,
        None if path.is_absolute() => return None,
        None => path,
    };
    let text = rel.to_string_lossy().replace('\\', "/");
    Some(text.trim_start_matches("./").to_owned())
}

/// Is it a source kind the scan reads.
fn is_web_source(rel: &str) -> bool {
    rel.rsplit_once('.')
        .is_some_and(|(_, ext)| EXTS.contains(&ext.to_ascii_lowercase().as_str()))
}

/// The rows with the route facts, when the view needs them (`routes`, or `ziggy` without Laravel); nothing is
/// scanned unless a queried row is a JS / TS / Vue file.
pub(crate) fn attach(rows: &mut [QueryResult], root: &Path, format: &str) {
    if rows.is_empty() || !matches!(format, "routes" | "ziggy") {
        return;
    }
    let base = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let scope: HashSet<String> = rows
        .iter()
        .filter_map(|row| relative(Path::new(&row.graph.file), root, &base))
        .filter(|rel| is_web_source(rel))
        .collect();
    if scope.is_empty() {
        return;
    }
    let found = WebRoutes::open(root, &base, scope);
    if found.is_empty() {
        return;
    }
    let shared = Arc::new(found);
    for row in rows {
        row.web_routes = Some(Arc::clone(&shared));
    }
}
