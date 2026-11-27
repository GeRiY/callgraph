//! The `impact` output: text (the L1 style: `root:` first line, no tips, no padding, no blank lines), `--compact`
//! (one line per changed symbol) and `--format json`; text above the 400-line cap falls back to the compact form.

use std::collections::HashSet;
use std::fmt::Write as _;

use serde_json::{Value, json};

use super::diff::Status;
use super::links::RouteFact;
use super::map::Kind;
use super::{Caller, FileReport, Report, Request, SymbolReport, by_level};
use crate::i18n::{Lang, lang};
use crate::query::OutputLimit;

/// Names in one list (callers per level, tests, rules, other files) before `+N`.
const MAX_LIST: usize = 8;
/// Routes listed under one symbol before `+N`.
const MAX_ROUTES: usize = 3;
/// Validated fields listed before `+N`.
const MAX_FIELDS: usize = 6;

/// The counts of the header line.
struct Totals {
    symbols: usize,
    files: usize,
    callers: usize,
    routes: usize,
    pages: usize,
    test_files: usize,
}

fn totals(report: &Report) -> Totals {
    let mut callers: HashSet<(String, Option<String>, String)> = HashSet::new();
    let mut routes: HashSet<RouteFact> = HashSet::new();
    let mut pages: HashSet<String> = HashSet::new();
    let mut tests: HashSet<String> = HashSet::new();
    let mut symbols = 0;
    for file in &report.files {
        if let Some(page) = &file.page {
            pages.insert(page.name.clone());
            routes.extend(page.routes.iter().cloned());
        }
        for symbol in &file.symbols {
            if symbol.touched.kind != Kind::File {
                symbols += 1;
            }
            for caller in &symbol.callers {
                callers.insert((
                    caller.file.clone(),
                    caller.container.clone(),
                    caller.name.clone(),
                ));
            }
            for test in &symbol.tests {
                tests.insert(test.file.clone());
            }
            routes.extend(symbol.facts.routes.iter().cloned());
            pages.extend(symbol.facts.pages.iter().cloned());
        }
    }
    Totals {
        symbols,
        files: report.files.len(),
        callers: callers.len(),
        routes: routes.len(),
        pages: pages.len(),
        test_files: tests.len(),
    }
}

/// `3 routes` / `1 route` / (hu) `3 route`.
fn noun(n: usize, one: &str, many: &str, hu: &str) -> String {
    match lang() {
        Lang::En => format!("{n} {}", if n == 1 { one } else { many }),
        Lang::Hu => format!("{n} {hu}"),
    }
}

fn header(report: &Report) -> String {
    if report.files.is_empty() && report.other.is_empty() && report.deleted.is_empty() {
        return crate::t!("impact: no changes\n", "impact: nincs változás\n").to_owned();
    }
    let t = totals(report);
    let mut parts = vec![
        match lang() {
            Lang::En => format!(
                "{} in {}",
                noun(t.symbols, "changed symbol", "changed symbols", ""),
                noun(t.files, "file", "files", "")
            ),
            Lang::Hu => format!("{} módosult szimbólum {} fájlban", t.symbols, t.files),
        },
        noun(t.callers, "caller", "callers", "hívó"),
    ];
    if t.routes > 0 {
        parts.push(noun(t.routes, "route", "routes", "route"));
    }
    if t.pages > 0 {
        parts.push(noun(t.pages, "page", "pages", "oldal"));
    }
    parts.push(if t.test_files > 0 {
        match lang() {
            Lang::En => format!("tests: {}", noun(t.test_files, "file", "files", "")),
            Lang::Hu => format!("tesztek: {} fájl", t.test_files),
        }
    } else {
        crate::t!("tests: none", "tesztek: nincs").to_owned()
    });
    format!("impact: {}\n", parts.join(" · "))
}

fn root_line(report: &Report) -> String {
    let root = report.root.to_string_lossy();
    let shown = crate::query::strip_verbatim(root.trim_end_matches(std::path::MAIN_SEPARATOR));
    let branch = crate::query::branch::root_suffix(&report.root);
    crate::tf!("root: {shown}{branch}\n", "gyökér: {shown}{branch}\n")
}

/// `a · b · c · +2`
fn capped(items: &[String], max: usize) -> String {
    let mut text = items
        .iter()
        .take(max)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(" · ");
    if items.len() > max {
        let _ = write!(text, " · +{}", items.len() - max);
    }
    text
}

fn caller_text(caller: &Caller) -> String {
    let mark = if caller.uncertain { "?" } else { "" };
    if caller.name == "__top__" {
        format!("{} L{}{mark}", caller.file, caller.line)
    } else {
        format!("{} L{}{mark}", caller.who(), caller.line)
    }
}

fn test_text(test: &Caller) -> String {
    let mark = if test.uncertain { "?" } else { "" };
    format!("{}{mark}", test.who_in_file())
}

fn file_head(file: &FileReport) -> String {
    let mut line = file.path.clone();
    match (file.status, &file.old_path) {
        (Status::Added, _) => line.push_str(&crate::tf!("  (new file)", "  (új fájl)")),
        (Status::Renamed, Some(old)) => {
            line.push_str(&crate::tf!(
                "  (renamed from {old})",
                "  (átnevezve: {old})"
            ));
        }
        _ => {}
    }
    if let Some(at) = file.syntax_error {
        line.push_str(&crate::tf!(
            "  ⚠ syntax error L{at}: the map may be partial",
            "  ⚠ szintaktikai hiba L{at}: a térkép hiányos lehet"
        ));
    }
    line
}

fn routes_text(routes: &[RouteFact], indent: &str, out: &mut String) {
    for route in routes.iter().take(MAX_ROUTES) {
        let _ = writeln!(out, "{indent}route {}", route.text());
    }
    if routes.len() > MAX_ROUTES {
        let rest = routes.len() - MAX_ROUTES;
        let _ = writeln!(out, "{indent}+{}", noun(rest, "route", "routes", "route"));
    }
}

fn symbol_text(symbol: &SymbolReport, out: &mut String) {
    let touched = &symbol.touched;
    let mut head = vec![touched.label()];
    if !touched.span().is_empty() {
        head.push(touched.span());
    }
    let changed = touched.changed_text();
    if !changed.is_empty() {
        head.push(format!("({changed})"));
    }
    let _ = writeln!(out, "  {}", head.join("  "));
    if touched.kind == Kind::File {
        return;
    }
    let facts = &symbol.facts;
    routes_text(&facts.routes, "    ", out);
    for page in &facts.pages {
        let _ = writeln!(out, "    page {page}");
    }
    if !facts.rules.is_empty() {
        let from = if facts.requests.is_empty() {
            "inline".to_owned()
        } else {
            facts.requests.join("+")
        };
        let mut fields: Vec<String> = facts.rules.iter().take(MAX_FIELDS).cloned().collect();
        if facts.rules.len() > MAX_FIELDS {
            fields.push(format!("+{}", facts.rules.len() - MAX_FIELDS));
        }
        let _ = writeln!(out, "    rules {from}: {}", fields.join(", "));
    }
    let levels = by_level(&symbol.callers);
    if levels.is_empty() {
        let entry = if facts.routes.is_empty() && symbol.route_entries == 0 {
            ""
        } else {
            crate::t!(" (route entry)", " (route-belépő)")
        };
        let _ = writeln!(
            out,
            "    {}{entry}",
            crate::t!("callers: none in code", "hívók: nincs a kódban")
        );
    }
    for (level, callers) in levels {
        let items: Vec<String> = callers.iter().map(|caller| caller_text(caller)).collect();
        if level == 1 {
            let _ = writeln!(
                out,
                "    {}: {}",
                crate::t!("callers", "hívók"),
                capped(&items, MAX_LIST)
            );
        } else {
            let _ = writeln!(out, "    depth {level}: {}", capped(&items, MAX_LIST));
        }
    }
    if !symbol.tests.is_empty() {
        let items: Vec<String> = symbol.tests.iter().map(test_text).collect();
        let _ = writeln!(out, "    tests: {}", capped(&items, MAX_LIST));
    }
}

fn file_page_text(file: &FileReport, out: &mut String) {
    let Some(page) = &file.page else { return };
    let mut line = format!("  page {}", page.name);
    if let Some(by) = &page.rendered_by {
        let _ = write!(
            line,
            "  {}",
            crate::tf!("rendered by {by}", "renderelő: {by}")
        );
    }
    let _ = writeln!(out, "{line}");
    routes_text(&page.routes, "  ", out);
}

fn tail_text(report: &Report, out: &mut String) {
    if !report.deleted.is_empty() {
        let _ = writeln!(
            out,
            "{}: {}",
            crate::t!("deleted", "törölve"),
            capped(&report.deleted, MAX_LIST)
        );
    }
    if !report.other.is_empty() {
        let n = report.other.len();
        let names = capped(&report.other, 3);
        let _ = writeln!(
            out,
            "{}",
            crate::tf!(
                "also changed: {n} other files ({names})",
                "még módosult: {n} másik fájl ({names})"
            )
        );
    }
}

/// The full text.
fn text(report: &Report) -> String {
    let mut out = root_line(report);
    out.push_str(&header(report));
    for file in &report.files {
        out.push_str(&file_head(file));
        out.push('\n');
        file_page_text(file, &mut out);
        for symbol in &file.symbols {
            symbol_text(symbol, &mut out);
        }
    }
    tail_text(report, &mut out);
    out
}

/// One line per changed symbol: `file  label  L20-23  callers 3 · routes 1 · tests 2`.
fn compact(report: &Report) -> String {
    let mut out = root_line(report);
    out.push_str(&header(report));
    for file in &report.files {
        if let Some(page) = &file.page {
            let _ = writeln!(
                out,
                "{}  page {}  {} {}",
                file.path,
                page.name,
                crate::t!("routes", "route"),
                page.routes.len()
            );
        }
        for symbol in &file.symbols {
            let touched = &symbol.touched;
            if touched.kind == Kind::File {
                continue;
            }
            let mut counts: Vec<String> = vec![format!(
                "{} {}",
                crate::t!("callers", "hívók"),
                symbol
                    .callers
                    .iter()
                    .map(|caller| (&caller.file, &caller.container, &caller.name))
                    .collect::<HashSet<_>>()
                    .len()
            )];
            if !symbol.facts.routes.is_empty() {
                counts.push(format!(
                    "{} {}",
                    crate::t!("routes", "route"),
                    symbol.facts.routes.len()
                ));
            }
            if !symbol.facts.pages.is_empty() {
                counts.push(format!(
                    "{} {}",
                    crate::t!("pages", "oldal"),
                    symbol.facts.pages.len()
                ));
            }
            let test_files: HashSet<&String> = symbol.tests.iter().map(|test| &test.file).collect();
            if !test_files.is_empty() {
                counts.push(format!(
                    "{} {}",
                    crate::t!("tests", "tesztek"),
                    test_files.len()
                ));
            }
            let _ = writeln!(
                out,
                "{}  {}  {}  {}",
                file.path,
                touched.label(),
                touched.span(),
                counts.join(" · ")
            );
        }
    }
    tail_text(report, &mut out);
    out
}

fn caller_json(caller: &Caller) -> Value {
    json!({
        "level": caller.level,
        "file": caller.file,
        "container": caller.container,
        "caller": caller.name,
        "line": caller.line,
        "uncertain": caller.uncertain,
    })
}

fn route_json(route: &RouteFact) -> Value {
    json!({"verb": route.verb, "uri": route.uri, "name": route.name})
}

fn symbol_json(symbol: &SymbolReport) -> Value {
    let touched = &symbol.touched;
    json!({
        "kind": match touched.kind {
            Kind::Method => "method",
            Kind::Class => "class",
            Kind::File => "file",
        },
        "container": touched.container,
        "name": touched.name,
        "start": touched.start,
        "end": touched.end,
        "changed": touched.changed.iter().map(|(a, b)| json!([a, b])).collect::<Vec<_>>(),
        "removedAfter": touched.removed_after,
        "routes": symbol.facts.routes.iter().map(route_json).collect::<Vec<_>>(),
        "pages": symbol.facts.pages,
        "requests": symbol.facts.requests,
        "rules": symbol.facts.rules,
        "routeEntries": symbol.route_entries,
        "callers": symbol.callers.iter().map(caller_json).collect::<Vec<_>>(),
        "tests": symbol.tests.iter().map(caller_json).collect::<Vec<_>>(),
    })
}

/// The structured object of `--format json`.
fn to_json(report: &Report) -> String {
    let t = totals(report);
    let files: Vec<Value> = report
        .files
        .iter()
        .map(|file| {
            json!({
                "file": file.path,
                "oldPath": file.old_path,
                "status": match file.status {
                    Status::Modified => "modified",
                    Status::Added => "added",
                    Status::Deleted => "deleted",
                    Status::Renamed => "renamed",
                },
                "syntaxErrorLine": file.syntax_error,
                "page": file.page.as_ref().map(|page| json!({
                    "name": page.name,
                    "renderedBy": page.rendered_by,
                    "routes": page.routes.iter().map(route_json).collect::<Vec<_>>(),
                })),
                "symbols": file.symbols.iter().map(symbol_json).collect::<Vec<_>>(),
            })
        })
        .collect();
    let value = json!({
        "root": report.root.to_string_lossy(),
        "depth": report.depth,
        "summary": {
            "symbols": t.symbols,
            "files": t.files,
            "callers": t.callers,
            "routes": t.routes,
            "pages": t.pages,
            "testFiles": t.test_files,
        },
        "files": files,
        "deleted": report.deleted,
        "other": report.other,
    });
    let mut text = serde_json::to_string_pretty(&value).unwrap_or_default();
    text.push('\n');
    text
}

/// The rendered answer: JSON, compact, or text; text above the line cap becomes compact, and compact above it is cut.
pub(crate) fn render(report: &Report, request: &Request) -> String {
    if request.json {
        return to_json(report);
    }
    let limit = OutputLimit::from_env(request.full).limit;
    if request.compact {
        return cap_at(compact(report), limit, request.full);
    }
    let full = text(report);
    let lines = full.lines().count();
    if request.full || lines <= limit {
        return full;
    }
    let short = compact(report);
    let (head, rest) = short.split_once('\n').unwrap_or((short.as_str(), ""));
    let note = crate::tf!(
        "⚠ impact output would be {lines} lines — one line per changed symbol instead. Narrow it: a revision range or --depth 1; full output: --full.",
        "⚠ az impact kimenete {lines} sor lenne — helyette szimbólumonként egy sor. Szűkíts: revízió-tartomány vagy --depth 1; a teljes kimenet: --full."
    );
    cap_at(format!("{head}\n{note}\n{rest}"), limit, request.full)
}

/// At most `limit` lines; the rest is one `… +N` line (`full`: never cut).
fn cap_at(text: String, limit: usize, full: bool) -> String {
    let lines = text.lines().count();
    if full || lines <= limit {
        return text;
    }
    let keep = limit.saturating_sub(1);
    let mut out: String = text.lines().take(keep).collect::<Vec<_>>().join("\n");
    let rest = lines - keep;
    let _ = write!(
        out,
        "\n{}\n",
        crate::tf!(
            "… +{rest} more lines (narrow the range or --depth; --full lifts the cap)",
            "… +{rest} további sor (szűkíts a tartományon vagy a --depth-en; a --full feloldja a korlátot)"
        )
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::impact::links::Facts;
    use crate::impact::map::Touched;
    use std::path::PathBuf;

    fn caller(
        file: &str,
        container: &str,
        name: &str,
        line: u32,
        level: usize,
        uncertain: bool,
    ) -> Caller {
        Caller {
            file: file.to_owned(),
            container: Some(container.to_owned()),
            name: name.to_owned(),
            line,
            level,
            uncertain,
        }
    }

    fn sample() -> Report {
        let touched = Touched {
            kind: Kind::Method,
            container: Some("OrganizationsController".into()),
            name: "destroy".into(),
            start: 94,
            end: 99,
            changed: vec![(96, 96)],
            removed_after: Vec::new(),
        };
        let symbol = SymbolReport {
            touched,
            callers: vec![
                caller("app/A.php", "A", "edit", 71, 1, false),
                caller("app/B.php", "B", "index", 20, 1, true),
                caller("app/C.php", "C", "run", 3, 2, false),
            ],
            tests: vec![caller(
                "tests/OrgTest.php",
                "OrgTest",
                "test_delete",
                12,
                1,
                false,
            )],
            route_entries: 1,
            facts: Facts {
                routes: vec![RouteFact {
                    verb: "DELETE".into(),
                    uri: "/organizations/{organization}".into(),
                    name: Some("organizations.destroy".into()),
                }],
                pages: Vec::new(),
                requests: vec!["DestroyRequest".into()],
                rules: vec!["reason*".into(), "note".into()],
            },
        };
        Report {
            root: PathBuf::from("/nonexistent/project"),
            files: vec![FileReport {
                path: "app/Http/Controllers/OrganizationsController.php".into(),
                old_path: None,
                status: Status::Modified,
                syntax_error: None,
                symbols: vec![symbol],
                page: None,
            }],
            other: vec!["README.md".into(), "composer.json".into()],
            deleted: Vec::new(),
            depth: 2,
        }
    }

    fn en() -> Request {
        Request::default()
    }

    #[test]
    fn json_has_the_summary_and_the_callers() {
        let value: Value =
            serde_json::from_str(&render(&sample(), &Request { json: true, ..en() })).unwrap();
        assert_eq!(value["summary"]["symbols"], 1);
        assert_eq!(value["summary"]["callers"], 3);
        assert_eq!(value["summary"]["testFiles"], 1);
        let symbol = &value["files"][0]["symbols"][0];
        assert_eq!(symbol["kind"], "method");
        assert_eq!(symbol["routes"][0]["verb"], "DELETE");
        assert_eq!(symbol["callers"][1]["uncertain"], true);
        assert_eq!(symbol["tests"][0]["file"], "tests/OrgTest.php");
        assert_eq!(value["other"][0], "README.md");
    }

    #[test]
    fn text_lists_route_rules_callers_by_level_and_tests() {
        let out = text(&sample());
        assert!(
            out.lines().next().unwrap().contains("/nonexistent/project"),
            "{out}"
        );
        assert!(out.contains("impact: 1 "), "{out}");
        assert!(
            out.contains("app/Http/Controllers/OrganizationsController.php\n"),
            "{out}"
        );
        assert!(
            out.contains("OrganizationsController::destroy()  L94-99  ("),
            "{out}"
        );
        assert!(
            out.contains("route DELETE /organizations/{organization} (organizations.destroy)"),
            "{out}"
        );
        assert!(out.contains("rules DestroyRequest: reason*, note"), "{out}");
        assert!(out.contains("A::edit L71 · B::index L20?"), "{out}");
        assert!(out.contains("depth 2: C::run L3"), "{out}");
        assert!(
            out.contains("tests: tests/OrgTest.php::test_delete"),
            "{out}"
        );
        assert!(out.contains("README.md · composer.json"), "{out}");
        assert!(!out.contains("\n\n"), "no blank lines: {out}");
    }

    #[test]
    fn compact_is_one_line_per_symbol() {
        let out = compact(&sample());
        let symbol_lines: Vec<&str> = out.lines().filter(|l| l.contains("destroy()")).collect();
        assert_eq!(symbol_lines.len(), 1, "{out}");
        assert!(symbol_lines[0].contains(" 3 · "), "{out}");
        assert!(symbol_lines[0].ends_with(" 1"), "{out}");
    }

    #[test]
    fn an_empty_report_says_so() {
        let report = Report {
            root: PathBuf::from("/p"),
            files: Vec::new(),
            other: Vec::new(),
            deleted: Vec::new(),
            depth: 1,
        };
        let out = text(&report);
        assert!(out.contains("impact: "), "{out}");
        assert_eq!(out.lines().count(), 2, "{out}");
    }

    #[test]
    fn a_long_compact_list_is_cut_with_a_count() {
        let long: String = (0..30).map(|n| format!("line {n}\n")).collect();
        let out = cap_at(long.clone(), 10, false);
        assert_eq!(out.lines().count(), 10);
        assert!(out.contains("+21"), "{out}");
        assert_eq!(cap_at(long.clone(), 10, true), long);
    }
}
