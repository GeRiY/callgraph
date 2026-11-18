//! `--compact`: the `--grep --depth N` view as a token-cheap impact list. Per level, grouped by file, one line per
//! caller (`Type::name()  L<start>  ← target`), no call-site code lines.

use super::grep_depth::{DepthView, is_test_or_story};
use crate::index::CallerRow;
use crate::{t, tf};
use std::fmt::Write as _;

/// One method line under its file heading. `detail` is the `← target` / `via` suffix (empty on level 1).
fn push_entry(
    output: &mut String,
    last_file: &mut String,
    file: &str,
    container: Option<&str>,
    name: &str,
    line: u32,
    detail: &str,
) {
    if last_file != file {
        writeln!(output, "{file}").unwrap();
        last_file.clear();
        last_file.push_str(file);
    }
    let owner = container.map_or_else(String::new, |c| format!("{c}::"));
    writeln!(output, "  {owner}{name}()  L{line}{detail}").unwrap();
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
        let mut last_file = String::new();
        let mut shown = 0usize;
        let mut hidden: Vec<&CallerRow> = Vec::new();
        let mut dynamic = 0usize;
        if level == 1 {
            let mut direct = view
                .rows
                .iter()
                .flat_map(|row| row.graph.methods.iter().map(move |method| (row, method)))
                .collect::<Vec<_>>();
            direct.sort_by(|(a_row, a), (b_row, b)| {
                (&a_row.graph.file, a.line).cmp(&(&b_row.graph.file, b.line))
            });
            for (row, method) in &direct {
                push_entry(
                    &mut output,
                    &mut last_file,
                    &row.graph.file,
                    method.container.as_deref(),
                    &method.name,
                    method.line,
                    "",
                );
                shown += 1;
                if view.ranges.has_dynamic(
                    &row.graph.file,
                    method.container.as_deref(),
                    &method.name,
                ) {
                    dynamic += 1;
                }
            }
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
            // one row per caller and target: a file's `__top__` lists each route reference it makes
            let mut listed = std::collections::HashSet::new();
            callers.retain(|row| {
                listed.insert((
                    row.file.clone(),
                    row.container.clone(),
                    row.caller.clone(),
                    row.target.clone(),
                ))
            });
            if !view.full && callers.iter().any(|row| !is_test_or_story(&row.file)) {
                let (tests, rest): (Vec<&CallerRow>, Vec<&CallerRow>) = callers
                    .into_iter()
                    .partition(|row| is_test_or_story(&row.file));
                hidden = tests;
                callers = rest;
            }
            for caller in &callers {
                let start = view
                    .ranges
                    .range(&caller.file, caller.container.as_deref(), &caller.caller)
                    .map_or(caller.line, |range| range.0);
                let mut detail = format!(" ← {}", caller.target);
                if let Some(via) = caller.via.as_deref() {
                    write!(detail, " via {via}").unwrap();
                }
                if caller.guessed {
                    detail.push_str(" ?");
                }
                push_entry(
                    &mut output,
                    &mut last_file,
                    &caller.file,
                    caller.container.as_deref(),
                    &caller.caller,
                    start,
                    &detail,
                );
                shown += 1;
                if view.ranges.has_dynamic(
                    &caller.file,
                    caller.container.as_deref(),
                    &caller.caller,
                ) {
                    dynamic += 1;
                }
            }
        }
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
        if shown == 0 && hidden.is_empty() {
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
        assert_eq!(lines[1], "command.go");
        assert_eq!(lines[2], "  Command::execute()  L905");
        assert!(lines[3].starts_with("== L2"), "{text}");
        assert_eq!(lines[4], "command.go");
        // the method start (not the call line), the called target, and no `L1148 │ code` line
        assert_eq!(lines[5], "  Command::execute_c()  L1084 ← execute");
        assert_eq!(lines[6], "  Command::run()  L40 ← execute ?");
        assert!(!text.contains('│'), "{text}");
        assert!(lines.last().unwrap().starts_with("💡"), "{text}");
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
}
