//! `--compact`: the `--grep --depth N` view as a token-cheap impact list. Per level, one line per file
//! (`path: Type::a()  L12, b()  L40 ← target`), no call-site code lines, except for top-level (`__top__`) callers,
//! which show their trimmed call-site code (a route registration has no method name).

use super::grep_depth::{DepthView, call_target, is_ambiguous, is_test_or_story, targets_of};
use crate::index::CallerRow;
use crate::model::Call;
use crate::query::format::method_calls;
use crate::{t, tf};
use std::fmt::Write as _;

/// A file line wraps (continuation indented) past this many characters.
const WRAP: usize = 160;
/// Call-site code of a top-level caller is cut to this many characters.
const CODE_MAX: usize = 90;

/// One caller of a level: `text` is `Type::name()  L<start>` (or `L<line> <code>` for top-level), `detail` the
/// ` ← target [via X]` part (empty on level 1 and for top-level callers).
struct Entry {
    file: String,
    line: u32,
    text: String,
    detail: String,
    guessed: bool,
}

/// A call made outside any method (a route file, a script): no method name to show.
fn is_top_level(call: &Call) -> bool {
    call.caller.as_deref().is_none_or(|name| name == "__top__")
}

fn trimmed_code(code: &str) -> String {
    let joined = code.split_whitespace().collect::<Vec<_>>().join(" ");
    // the statement terminator would read as part of the `, ` list separator
    let flat = joined.trim_end_matches(';').to_string();
    if flat.chars().count() <= CODE_MAX {
        return flat;
    }
    let cut: String = flat.chars().take(CODE_MAX - 1).collect();
    format!("{cut}…")
}

/// Appends `piece` to the file line, wrapping onto an indented continuation line past `WRAP` characters.
fn push_piece(output: &mut String, current: &mut String, piece: &str, first: bool) {
    if !first {
        if current.chars().count() + 2 + piece.chars().count() + 1 > WRAP {
            current.push(',');
            output.push_str(current);
            output.push('\n');
            current.clear();
            current.push_str("    ");
        } else {
            current.push_str(", ");
        }
    }
    current.push_str(piece);
}

/// One line per file: the entries in order; the `← target` part is printed once at the end when every entry of the
/// file shares it, otherwise after an entry only when it differs from the previous entry's.
fn write_level(output: &mut String, entries: &[Entry]) {
    let mut at = 0;
    while at < entries.len() {
        let file = entries[at].file.as_str();
        let end = entries[at..]
            .iter()
            .position(|entry| entry.file != file)
            .map_or(entries.len(), |n| at + n);
        let group = &entries[at..end];
        let first_detail = group
            .iter()
            .map(|e| e.detail.as_str())
            .find(|d| !d.is_empty());
        let shared = first_detail.filter(|detail| {
            group.len() > 1
                && group
                    .iter()
                    .all(|e| e.detail.is_empty() || e.detail == *detail)
        });
        let mut current = format!("{file}: ");
        let mut previous = "";
        for (n, entry) in group.iter().enumerate() {
            let mut piece = entry.text.clone();
            if shared.is_none() && !entry.detail.is_empty() && entry.detail != previous {
                piece.push_str(&entry.detail);
            }
            if entry.guessed {
                piece.push_str(" ?");
            }
            if !entry.detail.is_empty() {
                previous = entry.detail.as_str();
            }
            push_piece(output, &mut current, &piece, n == 0);
        }
        if let Some(detail) = shared {
            current.push_str(detail);
        }
        output.push_str(&current);
        output.push('\n');
        at = end;
    }
}

fn owner_prefix(container: Option<&str>) -> String {
    container.map_or_else(String::new, |c| format!("{c}::"))
}

/// The compact per-level view. Level 1 lists the methods that call the pattern, level 2+ the callers of the
/// previous level; `?` marks name-only (guessed) callers.
pub(super) fn render_compact(view: &DepthView<'_>) -> String {
    let mut output = String::new();
    for level in 1..=view.depth {
        if level == 1 {
            output.push_str(&tf!(
                "== L1 calls /{}/\n",
                "== L1 hívja: /{}/\n",
                view.pattern
            ));
        } else {
            output.push_str(&tf!(
                "== L{} calls L{}\n",
                "== L{} hívja: L{}\n",
                level,
                level - 1
            ));
        }
        let mut entries: Vec<Entry> = Vec::new();
        let mut hidden: Vec<&CallerRow> = Vec::new();
        let mut dynamic = 0usize;
        if level == 1 {
            // a short pattern matched several fully qualified targets: each entry names its own
            let ambiguous = is_ambiguous(view.rows);
            let mut direct = view
                .rows
                .iter()
                .flat_map(|row| row.graph.methods.iter().map(move |method| (row, method)))
                .collect::<Vec<_>>();
            direct.sort_by(|(a_row, a), (b_row, b)| {
                (&a_row.graph.file, a.line).cmp(&(&b_row.graph.file, b.line))
            });
            for (row, method) in &direct {
                let detail = if ambiguous {
                    let calls = method_calls(&row.graph, method);
                    format!(" ← {}", targets_of(&calls))
                } else {
                    String::new()
                };
                entries.push(Entry {
                    file: row.graph.file.clone(),
                    line: method.line,
                    text: format!(
                        "{}{}()  L{}",
                        owner_prefix(method.container.as_deref()),
                        method.name,
                        method.line
                    ),
                    detail,
                    guessed: false,
                });
                if view.ranges.has_dynamic(
                    &row.graph.file,
                    method.container.as_deref(),
                    &method.name,
                ) {
                    dynamic += 1;
                }
            }
            // top-level calls (routes, scripts): the trimmed call-site code stands for the missing method name
            for row in view.rows {
                for call in row.graph.calls.iter().filter(|call| is_top_level(call)) {
                    let code = view
                        .source
                        .get(&row.graph.file, call.line)
                        .map(|code| trimmed_code(&code))
                        .unwrap_or_default();
                    entries.push(Entry {
                        file: row.graph.file.clone(),
                        line: call.line,
                        text: format!("L{} {code}", call.line).trim_end().to_owned(),
                        detail: if ambiguous {
                            format!(" ← {}", call_target(call))
                        } else {
                            String::new()
                        },
                        guessed: false,
                    });
                }
            }
            entries.sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));
        } else {
            let mut callers = view
                .callers
                .iter()
                .filter(|row| row.level == level)
                .collect::<Vec<_>>();
            callers.sort_by(|a, b| {
                (&a.file, &a.container, &a.caller, a.line).cmp(&(
                    &b.file,
                    &b.container,
                    &b.caller,
                    b.line,
                ))
            });
            // top-level callers stay distinct per call line (several routes in one file)
            callers.dedup_by(|b, a| {
                a.file == b.file
                    && a.container == b.container
                    && a.caller == b.caller
                    && (a.caller != "__top__" || a.line == b.line)
            });
            if !view.full && callers.iter().any(|row| !is_test_or_story(&row.file)) {
                let (tests, rest): (Vec<&CallerRow>, Vec<&CallerRow>) = callers
                    .into_iter()
                    .partition(|row| is_test_or_story(&row.file));
                hidden = tests;
                callers = rest;
            }
            for caller in &callers {
                if caller.caller == "__top__" {
                    let code = view
                        .source
                        .get(&caller.file, caller.line)
                        .map(|code| trimmed_code(&code))
                        .unwrap_or_default();
                    // the code line already names the call; without it the called target is the only clue
                    let detail = if code.is_empty() {
                        format!(" ← {}", caller.target)
                    } else {
                        String::new()
                    };
                    entries.push(Entry {
                        file: caller.file.clone(),
                        line: caller.line,
                        text: format!("L{} {code}", caller.line).trim_end().to_owned(),
                        detail,
                        guessed: caller.guessed,
                    });
                    continue;
                }
                let start = view
                    .ranges
                    .range(&caller.file, caller.container.as_deref(), &caller.caller)
                    .map_or(caller.line, |range| range.0);
                let mut detail = format!(" ← {}", caller.target);
                if let Some(via) = caller.via.as_deref() {
                    write!(detail, " via {via}").unwrap();
                }
                entries.push(Entry {
                    file: caller.file.clone(),
                    line: start,
                    text: format!(
                        "{}{}()  L{start}",
                        owner_prefix(caller.container.as_deref()),
                        caller.caller
                    ),
                    detail,
                    guessed: caller.guessed,
                });
                if view.ranges.has_dynamic(
                    &caller.file,
                    caller.container.as_deref(),
                    &caller.caller,
                ) {
                    dynamic += 1;
                }
            }
        }
        write_level(&mut output, &entries);
        if !hidden.is_empty() {
            let files: std::collections::BTreeSet<&str> =
                hidden.iter().map(|row| row.file.as_str()).collect();
            output.push_str(&tf!(
                "  tests/stories: {} callers in {} files (--full)\n",
                "  tesztek/story-k: {} hívó {} fájlban (--full)\n",
                hidden.len(),
                files.len()
            ));
        }
        if entries.is_empty() && hidden.is_empty() {
            if level == 1 && !view.no_caller.is_empty() {
                output.push_str(view.no_caller);
            } else {
                output.push_str(t!("  (no more callers)\n", "  (nincs további hívó)\n"));
            }
        }
        if level > 1
            && !view.guess
            && let Some(count) = view
                .skipped
                .get(level - 2)
                .copied()
                .filter(|count| *count > 0)
        {
            output.push_str(&tf!(
                "  (+{count} name-only skipped, --guess)\n",
                "  (+{count} csak név szerinti kihagyva, --guess)\n"
            ));
        }
        if dynamic > 0 {
            output.push_str(&tf!(
                "  ({dynamic} with dynamic calls, check by hand)\n",
                "  ({dynamic} dinamikus hívással, kézzel nézd át)\n"
            ));
        }
    }
    let total = output.lines().count();
    if !view.full && total > view.max {
        output = output.lines().take(view.max).collect::<Vec<_>>().join("\n");
        output.push('\n');
        output.push_str(&tf!(
            "⚠ … truncated at {} of {total} lines — narrow it down or --full\n",
            "⚠ … levágva {} sornál, összesen {total} — szűkíts vagy --full\n",
            view.max
        ));
    }
    output.push_str(t!(
        "💡 \"?\" = name-only match. Body: <file>::<method> --body\n",
        "💡 \"?\" = csak név szerinti egyezés. Törzs: <fájl>::<metódus> --body\n"
    ));
    output
}

#[cfg(test)]
mod tests {
    use super::super::grep_text::MethodRanges;
    use super::*;
    use crate::i18n::Lang;
    use crate::model::{FileGraph, FileGraphBuilder, Method};
    use crate::notes::NoteMarks;
    use crate::query::QueryResult;
    use crate::query::snippet::SourceLines;

    struct Ranges;
    impl MethodRanges for Ranges {
        fn range(&self, _: &str, _: Option<&str>, name: &str) -> Option<(u32, u32)> {
            (name == "execute_c").then_some((1084, 1100))
        }
        fn has_dynamic(&self, _: &str, _: Option<&str>, _: &str) -> bool {
            false
        }
    }

    fn method(container: &str, name: &str, line: u32) -> Method {
        Method {
            container: Some(container.into()),
            name: name.into(),
            line,
            end_line: Some(line + 5),
            visibility: None,
            is_static: false,
            is_abstract: false,
            kind: None,
            params: Vec::new(),
            return_type: None,
            doc_types: None,
            doc: None,
            body: None,
        }
    }

    fn caller(name: &str, line: u32, level: usize, guessed: bool) -> CallerRow {
        CallerRow {
            file: "command.go".into(),
            container: Some("Command".into()),
            caller: name.into(),
            line,
            target: "execute".into(),
            level,
            via: None,
            guessed,
            reference: false,
        }
    }

    fn graph() -> FileGraph {
        let mut builder = FileGraphBuilder::new("command.go");
        builder.method(method("Command", "execute", 905));
        builder.build()
    }

    #[test]
    fn depth_two_is_grouped_by_level_and_file_without_code_lines() {
        let rows = [QueryResult::new(graph(), None)];
        let callers = [
            caller("execute_c", 1148, 2, false),
            caller("run", 40, 2, true),
        ];
        let (source, marks) = (SourceLines::default(), NoteMarks::default());
        let text = render_compact(&DepthView {
            rows: &rows,
            pattern: "execute",
            depth: 2,
            guess: true,
            callers: &callers,
            skipped: &[],
            ranges: &Ranges,
            source: &source,
            lang: Lang::En,
            layers: None,
            marks: &marks,
            no_caller: "",
            full: false,
            max: 400,
            compact: true,
        });
        let lines: Vec<&str> = text.lines().collect();
        assert!(lines[0].starts_with("== L1"), "{text}");
        assert_eq!(lines[1], "command.go: Command::execute()  L905");
        assert!(lines[2].starts_with("== L2"), "{text}");
        // one line per file: the method start (not the call line), the shared target once at the end, `?` kept,
        // and no `L1148 │ code` line
        assert_eq!(
            lines[3],
            "command.go: Command::execute_c()  L1084, Command::run()  L40 ? ← execute"
        );
        assert!(!text.contains('│'), "{text}");
        assert!(lines.last().unwrap().starts_with("💡"), "{text}");
    }

    #[test]
    fn route_references_are_level_one_callers_of_the_action() {
        let mut builder = FileGraphBuilder::new("routes/web.php");
        for line in [5, 6] {
            builder.call(
                Call::new("static_call", "SongController::destroy", "external_call")
                    .at(line)
                    .within(None, Some("__top__".to_owned()))
                    .reference(),
            );
        }
        let rows = [QueryResult::new(builder.build(), None)];
        let (source, marks) = (SourceLines::default(), NoteMarks::default());
        let text = render_compact(&DepthView {
            rows: &rows,
            pattern: "SongController::destroy",
            depth: 1,
            guess: false,
            callers: &[],
            skipped: &[],
            ranges: &Ranges,
            source: &source,
            lang: Lang::En,
            layers: None,
            marks: &marks,
            no_caller: "",
            full: false,
            max: 400,
            compact: true,
        });
        assert!(text.contains("routes/web.php: L5, L6\n"), "{text}");
        assert!(!text.contains("no more callers"), "{text}");
    }

    #[test]
    fn one_top_level_caller_lists_each_referenced_target() {
        let rows = [QueryResult::new(graph(), None)];
        let route = |line: u32, target: &str| CallerRow {
            file: "routes.php".into(),
            container: None,
            caller: "__top__".into(),
            line,
            target: target.into(),
            level: 2,
            via: None,
            guessed: false,
            reference: true,
        };
        let callers = [
            route(40, "HomeController::__invoke"),
            route(41, "BlogController::list"),
            route(42, "BlogController::show"),
        ];
        let (source, marks) = (SourceLines::default(), NoteMarks::default());
        let text = render_compact(&DepthView {
            rows: &rows,
            pattern: "execute",
            depth: 2,
            guess: false,
            callers: &callers,
            skipped: &[],
            ranges: &Ranges,
            source: &source,
            lang: Lang::En,
            layers: None,
            marks: &marks,
            no_caller: "",
            full: false,
            max: 400,
            compact: true,
        });
        for target in [
            "HomeController::__invoke",
            "BlogController::list",
            "BlogController::show",
        ] {
            assert!(text.contains(&format!("← {target}")), "{text}");
        }
    }

    #[test]
    fn file_lines_show_changing_targets_and_wrap_long_lines() {
        let entry = |n: u32, detail: &str| Entry {
            file: "a.rs".into(),
            line: n,
            text: format!("T::method_number_{n}()  L{n}"),
            detail: detail.into(),
            guessed: false,
        };
        let mut out = String::new();
        write_level(
            &mut out,
            &[entry(1, " ← x"), entry(2, " ← x"), entry(3, " ← y")],
        );
        assert_eq!(
            out,
            "a.rs: T::method_number_1()  L1 ← x, T::method_number_2()  L2, T::method_number_3()  L3 ← y\n"
        );
        let many: Vec<Entry> = (1..=12).map(|n| entry(n, "")).collect();
        let mut out = String::new();
        write_level(&mut out, &many);
        assert!(out.lines().count() > 1, "{out}");
        assert!(out.lines().all(|line| line.chars().count() <= 160), "{out}");
        assert!(
            out.lines().skip(1).all(|line| line.starts_with("    ")),
            "{out}"
        );
        assert_eq!(
            trimmed_code(&format!("  {}  ", "x".repeat(200)))
                .chars()
                .count(),
            90
        );
    }
}
