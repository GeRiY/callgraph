//! `callgraph impact`: what does the current change affect? The git diff (working tree + index vs `HEAD`, `--staged`,
//! a revision range, or a unified diff on stdin) is mapped to the innermost changed symbols through the project
//! graph's line ranges; each symbol gets its callers (the `--grep '(^|::|\.|->)name$'` search, restricted to the
//! container, `--depth` levels), the framework links the index already knows (routes, Inertia pages, FormRequests)
//! and the tests among the callers. Output: `render.rs`; diff input: `diff.rs`; line → symbol: `map.rs`.

mod diff;
mod links;
mod map;
mod render;

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::analysis::Analyzer;
use crate::error::Result;
use crate::graph::ProjectGraph;
use crate::index::CallerRow;
use crate::io::FileWalker;
use crate::lang::LanguageRole;
use crate::project::ProjectStore;

pub use diff::DiffSource;
use diff::{FileDiff, Status};
use map::{Kind, Touched};

/// The deepest caller level (`--depth`).
pub const MAX_DEPTH: usize = 3;

/// At most this many symbols get a caller search (a refactor touching hundreds must not take minutes).
const MAX_SEARCHED: usize = 80;

/// At most this many name-only callers are added per changed method.
const MAX_NAME_ONLY: usize = 10;

/// At most this many untracked files are taken as new files.
const MAX_UNTRACKED: usize = 200;

/// What to report and how.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub source: DiffSource,
    /// Caller levels, 1..=`MAX_DEPTH`.
    pub depth: usize,
    /// One line per changed symbol.
    pub compact: bool,
    /// A structured object instead of text.
    pub json: bool,
    /// Lifts the 400-line cap.
    pub full: bool,
}

impl Default for Request {
    fn default() -> Self {
        Self {
            source: DiffSource::WorkingTree,
            depth: 1,
            compact: false,
            json: false,
            full: false,
        }
    }
}

/// A caller (or a test) of a changed symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Caller {
    /// Project-relative file.
    pub file: String,
    pub container: Option<String>,
    pub name: String,
    /// The line of the call.
    pub line: u32,
    /// 1 = calls the symbol, 2 = calls a level-1 caller, …
    pub level: usize,
    /// A name-only match (receiver type unknown): shown with `?`.
    pub uncertain: bool,
}

impl Caller {
    /// `Type::method`, `function`, or the file for top-level code.
    pub(crate) fn who(&self) -> String {
        if self.name == "__top__" {
            return self.file.clone();
        }
        match &self.container {
            Some(container) => format!("{container}::{}", self.name),
            None => self.name.clone(),
        }
    }

    /// `file::name` (a test), `file` for top-level code.
    pub(crate) fn who_in_file(&self) -> String {
        if self.name == "__top__" {
            return self.file.clone();
        }
        format!("{}::{}", self.file, self.name)
    }
}

/// A changed symbol with everything the report says about it.
pub(crate) struct SymbolReport {
    pub touched: Touched,
    /// Callers outside the test files, by level.
    pub callers: Vec<Caller>,
    /// Callers in test files (any level).
    pub tests: Vec<Caller>,
    /// References from route files (`Route::get(.., [X::class, 'm'])`): the route entry, not a caller.
    pub route_entries: usize,
    pub facts: links::Facts,
}

/// A changed file the index knows.
pub(crate) struct FileReport {
    pub path: String,
    pub old_path: Option<String>,
    pub status: Status,
    /// First syntax error line: the map of the file may be partial.
    pub syntax_error: Option<u32>,
    pub symbols: Vec<SymbolReport>,
    pub page: Option<links::PageFact>,
}

/// The whole answer.
pub(crate) struct Report {
    pub root: PathBuf,
    pub files: Vec<FileReport>,
    /// Changed files callgraph cannot map (documents, data, binary, ignored).
    pub other: Vec<String>,
    /// Deleted code files (their callers are not searched: the old symbols are gone from the index).
    pub deleted: Vec<String>,
    pub depth: usize,
}

/// The project-relative, `/`-separated path of an indexed file.
fn relative(root: &Path, file: &str) -> String {
    let path = Path::new(file);
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// The caller-search pattern of a symbol: calls of `Container::name` (a `?::name` receiver-less call matches by name
/// too), of a function by its name wherever it is called; for a class its instantiations and references.
fn pattern_of(symbol: &Touched) -> Option<String> {
    let name = regex::escape(&symbol.name);
    match (symbol.kind, &symbol.container) {
        (Kind::File, _) => None,
        (Kind::Class, _) => Some(format!(r"(^|\W){name}$")),
        // a constructor is reached through the class itself (`new X`, `X(…)`)
        (Kind::Method, Some(class))
            if matches!(symbol.name.as_str(), "__construct" | "__init__") =>
        {
            let class = regex::escape(class);
            Some(format!(r"(^|\W){class}(::{name})?$"))
        }
        (Kind::Method, Some(class)) => {
            let class = regex::escape(class);
            Some(format!(r"(^|\W){class}::{name}$"))
        }
        (Kind::Method, None) => Some(format!(r"(^|::|\.|->|\\){name}$")),
    }
}

/// The callers of a symbol, split into code callers and tests; self-recursion and route-file registrations dropped.
fn sort_rows(
    root: &Path,
    rows: Vec<CallerRow>,
    own_file: &str,
    symbol: &Touched,
) -> (Vec<Caller>, Vec<Caller>, usize) {
    let mut callers: Vec<Caller> = Vec::new();
    let mut tests: Vec<Caller> = Vec::new();
    let mut route_entries = 0;
    for row in rows {
        let file = relative(root, &row.file);
        let own = row.file == own_file
            && symbol.kind == Kind::Method
            && row.caller == symbol.name
            && row.container == symbol.container;
        if own {
            continue;
        }
        // `Route::post('/x', [X::class, 'm'])`: the route entry (shown as the route), not a caller
        if row.caller == "__top__" && (file.starts_with("routes/") || file.contains("/routes/")) {
            route_entries += 1;
            continue;
        }
        let entry = Caller {
            file,
            container: row.container.clone(),
            name: row.caller.clone(),
            line: row.line,
            level: row.level,
            uncertain: row.guessed,
        };
        let list = if crate::locate::rank::is_test_path(&entry.file) {
            &mut tests
        } else {
            &mut callers
        };
        // one entry per caller and level: the first call line; a certain call beats a name-only one
        let same = list.iter().position(|seen| {
            seen.file == entry.file
                && seen.container == entry.container
                && seen.name == entry.name
                && seen.level == entry.level
        });
        match same {
            Some(at) => list[at].uncertain &= entry.uncertain,
            None => list.push(entry),
        }
    }
    (callers, tests, route_entries)
}

/// The mapped files of the diff with their callers (one pass over the project graph; the lock is released on return).
fn analyse(
    root: &Path,
    analyzer: &Analyzer,
    code: &[FileDiff],
    depth: usize,
) -> Result<(Vec<FileReport>, Vec<String>)> {
    let store = ProjectStore::at_root(root);
    store.ensure()?;
    let files = crate::graph::legacy_order(
        analyzer.registry(),
        FileWalker::for_registry(analyzer.registry()).collect(&[root.to_path_buf()], &mut |_| {}),
    );
    let shared = crate::graph::shared(root);
    let mut guard = shared.as_ref().map(crate::graph::lock);
    let mut owned = None;
    let graph: &mut ProjectGraph = match guard.as_deref_mut() {
        Some(graph) => graph,
        None => owned.insert(ProjectGraph::open(store.dir().to_path_buf())),
    };
    graph.refresh(&files, true, analyzer)?;
    graph.ensure_callers(analyzer)?;
    // a failed save (read-only store) does not stop the answer
    let _ = graph.save();
    let mut reports = Vec::new();
    let mut unmapped = Vec::new();
    let mut searched = 0;
    for file in code {
        let key = root.join(&file.path).to_string_lossy().into_owned();
        let Some(file_graph) = graph.slim_graph(&key) else {
            unmapped.push(file.path.clone());
            continue;
        };
        if file_graph.binary {
            unmapped.push(file.path.clone());
            continue;
        }
        let mut symbols = Vec::new();
        for touched in map::touched(file_graph, &file.added, &file.removed_after) {
            let rows = match pattern_of(&touched) {
                Some(pattern) if searched < MAX_SEARCHED => {
                    searched += 1;
                    let index = graph.caller_index();
                    let mut rows = index.callers(&pattern, depth).unwrap_or_default();
                    // a method call whose receiver type is unknown: a name-only match, shown with `?`
                    if touched.kind == Kind::Method
                        && touched.container.is_some()
                        && !touched.name.starts_with("__")
                    {
                        let seen: std::collections::HashSet<(String, u32)> =
                            rows.iter().map(|r| (r.file.clone(), r.line)).collect();
                        let by_name = format!(r"(^|::|\.|->){}$", regex::escape(&touched.name));
                        let receiver_unknown = index
                            .callers(&by_name, 1)
                            .unwrap_or_default()
                            .into_iter()
                            .filter(|row| unknown_receiver(&row.target, &touched.name));
                        let mut extra: Vec<CallerRow> = index
                            .name_only_callers(&touched.name)
                            .into_iter()
                            .chain(receiver_unknown)
                            .filter(|row| !seen.contains(&(row.file.clone(), row.line)))
                            // a name-only match only counts inside the same language family (a Vue template's
                            // own `destroy()` is not a call of the PHP controller's `destroy`)
                            .filter(|row| same_family(&row.file, &file.path))
                            .collect();
                        extra.sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));
                        extra.dedup_by(|a, b| a.file == b.file && a.line == b.line);
                        rows.extend(extra.into_iter().take(MAX_NAME_ONLY).map(|mut row| {
                            row.level = 1;
                            row.guessed = true;
                            row
                        }));
                    }
                    rows
                }
                _ => Vec::new(),
            };
            let (callers, tests, route_entries) = sort_rows(root, rows, &key, &touched);
            symbols.push(SymbolReport {
                touched,
                callers,
                tests,
                route_entries,
                facts: links::Facts::default(),
            });
        }
        reports.push(FileReport {
            path: file.path.clone(),
            old_path: file.old_path.clone(),
            status: file.status,
            syntax_error: file_graph.parse_errors.first().map(|error| error.line),
            symbols,
            page: None,
        });
    }
    Ok((reports, unmapped))
}

/// Number of lines of an untracked file; `None` for a binary file.
fn count_lines(bytes: &[u8]) -> Option<u32> {
    if bytes.contains(&0) {
        return None;
    }
    let newlines = bytes.iter().filter(|&&b| b == b'\n').count();
    let tail = usize::from(!bytes.is_empty() && bytes.last() != Some(&b'\n'));
    u32::try_from(newlines + tail).ok()
}

/// The report of the diff of `request.source` in the project at `root`, rendered. Errors (`Usage`): git missing, not a
/// repository, a refused range.
pub fn run(root: &Path, request: &Request, stdin: &mut dyn Read) -> Result<String> {
    let depth = request.depth.clamp(1, MAX_DEPTH);
    let loaded = diff::load(root, &request.source, stdin)?;
    let mut files = diff::parse(&loaded.diff);
    for path in loaded.untracked.iter().take(MAX_UNTRACKED) {
        if files.iter().any(|file| &file.path == path) {
            continue;
        }
        match std::fs::read(root.join(path)).map(|bytes| count_lines(&bytes)) {
            Ok(Some(lines)) => files.push(FileDiff::whole_file(path.clone(), lines)),
            // binary or unreadable: listed as another changed file
            _ => files.push(FileDiff::whole_file(path.clone(), 0)),
        }
    }
    let analyzer = Analyzer::with_defaults();
    let mut code: Vec<FileDiff> = Vec::new();
    let mut other: Vec<String> = Vec::new();
    let mut deleted: Vec<String> = Vec::new();
    for file in files {
        let analysable = analyzer
            .registry()
            .for_path(Path::new(&file.path))
            .is_some_and(|language| language.role() != LanguageRole::Document);
        if file.binary || !analysable {
            other.push(file.path);
        } else if file.status == Status::Deleted {
            deleted.push(file.path);
        } else {
            code.push(file);
        }
    }
    let (mut reports, unmapped) = if code.is_empty() {
        (Vec::new(), Vec::new())
    } else {
        analyse(root, &analyzer, &code, depth)?
    };
    other.extend(unmapped);
    // the framework tables (routes, pages, validation) once, after the graph lock is gone
    if reports.iter().any(|file| !file.symbols.is_empty()) {
        let links = links::Links::load(root);
        if links.active() {
            for file in &mut reports {
                file.page = links.of_page(&file.path);
                for symbol in &mut file.symbols {
                    if let (Kind::Method, Some(class)) =
                        (symbol.touched.kind, symbol.touched.container.as_deref())
                    {
                        symbol.facts = links.of_method(&file.path, class, &symbol.touched.name);
                    }
                }
            }
        }
    }
    let report = Report {
        root: root.to_path_buf(),
        files: reports,
        other,
        deleted,
        depth,
    };
    Ok(render::render(&report, request))
}

/// Caller entries grouped by level, for the renderers.
pub(crate) fn by_level(callers: &[Caller]) -> Vec<(usize, Vec<&Caller>)> {
    let mut levels: HashMap<usize, Vec<&Caller>> = HashMap::new();
    for caller in callers {
        levels.entry(caller.level).or_default().push(caller);
    }
    let mut out: Vec<(usize, Vec<&Caller>)> = levels.into_iter().collect();
    out.sort_by_key(|(level, _)| *level);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn symbol(kind: Kind, container: Option<&str>, name: &str) -> Touched {
        Touched {
            kind,
            container: container.map(str::to_owned),
            name: name.to_owned(),
            start: 1,
            end: 2,
            changed: vec![(1, 1)],
            removed_after: Vec::new(),
        }
    }

    #[test]
    fn patterns_follow_the_container() {
        let method = symbol(Kind::Method, Some("Order"), "cancel");
        assert_eq!(
            pattern_of(&method).as_deref(),
            Some(r"(^|\W)Order::cancel$")
        );
        let function = symbol(Kind::Method, None, "run");
        assert_eq!(
            pattern_of(&function).as_deref(),
            Some(r"(^|::|\.|->|\\)run$")
        );
        let class = symbol(Kind::Class, None, "Order");
        assert_eq!(pattern_of(&class).as_deref(), Some(r"(^|\W)Order$"));
        let ctor = symbol(Kind::Method, Some("Order"), "__construct");
        assert_eq!(
            pattern_of(&ctor).as_deref(),
            Some(r"(^|\W)Order(::__construct)?$")
        );
        assert_eq!(pattern_of(&symbol(Kind::File, None, "")), None);
    }

    #[test]
    fn untracked_files_count_their_lines() {
        assert_eq!(count_lines(b""), Some(0));
        assert_eq!(count_lines(b"a\nb\n"), Some(2));
        assert_eq!(count_lines(b"a\nb"), Some(2));
        assert_eq!(count_lines(b"a\0b"), None);
    }

    #[test]
    fn rows_split_into_callers_tests_and_route_entries() {
        let root = Path::new("/p");
        let row = |file: &str, container: Option<&str>, caller: &str, reference: bool, guessed| {
            CallerRow {
                file: format!("/p/{file}"),
                container: container.map(str::to_owned),
                caller: caller.to_owned(),
                line: 7,
                target: "T::m".to_owned(),
                level: 1,
                via: None,
                guessed,
                reference,
            }
        };
        let rows = vec![
            row("app/B.php", Some("B"), "index", false, false),
            row("app/B.php", Some("B"), "index", false, true),
            row("tests/BTest.php", Some("BTest"), "test_it", false, false),
            row("routes/web.php", None, "__top__", false, false),
            row("app/A.php", Some("A"), "m", false, false),
        ];
        let me = symbol(Kind::Method, Some("A"), "m");
        let (callers, tests, entries) = sort_rows(root, rows, "/p/app/A.php", &me);
        assert_eq!(entries, 1);
        assert_eq!(tests.len(), 1);
        assert_eq!(tests[0].who_in_file(), "tests/BTest.php::test_it");
        // the duplicate is one caller, certain because one of the calls was; the recursive call is gone
        assert_eq!(callers.len(), 1);
        assert_eq!(callers[0].who(), "B::index");
        assert!(!callers[0].uncertain);
    }
}

/// Same source-language family by extension (JS / TS / Vue / Svelte count as one; Blade is `.php`).
fn same_family(a: &str, b: &str) -> bool {
    fn family(path: &str) -> &str {
        let ext = path.rsplit('.').next().unwrap_or("");
        match ext {
            "js" | "mjs" | "cjs" | "jsx" | "ts" | "tsx" | "vue" | "svelte" => "js",
            "php" => "php",
            other => other,
        }
    }
    family(a) == family(b)
}

/// A call whose receiver type is unknown: `name`, `?::name`, or a variable receiver such as `cart.name` /
/// `$cart->name` / `self.repo.name` (lower-case qualifier). A call resolved to another class (`Other::name`,
/// `Other.name`) is not a caller of the changed method and stays out.
fn unknown_receiver(target: &str, name: &str) -> bool {
    if target.eq_ignore_ascii_case(name) {
        return true;
    }
    let Some(qualifier) = ["::", "->", "."]
        .iter()
        .find_map(|sep| target.strip_suffix(name).and_then(|t| t.strip_suffix(sep)))
    else {
        return false;
    };
    let last = qualifier
        .rsplit(['.', ':', '\\', '>', '-', '$'])
        .find(|p| !p.is_empty())
        .unwrap_or(qualifier);
    qualifier == "?"
        || last
            .chars()
            .next()
            .is_some_and(|c| c.is_lowercase() || c == '_')
}

#[cfg(test)]
mod receiver_tests {
    use super::unknown_receiver;

    #[test]
    fn variable_and_unknown_receivers_are_name_only_matches() {
        assert!(unknown_receiver("total", "total"));
        assert!(unknown_receiver("?::total", "total"));
        assert!(unknown_receiver("cart.total", "total"));
        assert!(unknown_receiver("$cart->total", "total"));
        assert!(unknown_receiver("self.repo.total", "total"));
        assert!(!unknown_receiver("Other::total", "total"));
        assert!(!unknown_receiver("App\\Models\\Other::total", "total"));
        assert!(!unknown_receiver("cart.subtotal", "total"));
    }
}
