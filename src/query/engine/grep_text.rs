//! A `--grep` szöveges kimenete: találatonkénti vagy mélységi (hívók hívói) nézet.

use super::{QueryEngine, QueryResult};
use crate::error::Result;
use crate::index::{CallerIndex, FileGraphRecord};
use crate::query::format::FormatterRegistry;
use crate::{t, tf};
use std::path::Path;

impl QueryEngine {
    pub fn grep_text(
        &self,
        rows: &[QueryResult],
        pattern: &str,
        depth: usize,
        guess: bool,
        full: bool,
        limit: Option<usize>,
    ) -> Result<String> {
        if depth <= 1 {
            let text = FormatterRegistry::format("outline", rows)?;
            if rows.is_empty() {
                // Nincs találat: csak a záró tipp marad (nincs fejléc-elválasztó).
                return Ok(text.trim_start_matches("\n---\n").to_owned());
            }
            let max = limit.unwrap_or_else(|| crate::query::OutputLimit::from_env(full).limit);
            if !full && text.lines().count() > max {
                let mut lines = tf!(
                    "⚠ --grep outline would be {} lines — showing 1 line per match instead (file  caller  L<line>  ← target). Narrow it down, or --full.\n\n",
                    "⚠ A --grep outline {} sor lenne — helyette találatonként 1 sor (fájl  hívó  L<sor>  ← cél). Szűkíts, vagy --full.\n\n",
                    text.lines().count()
                );
                for row in rows {
                    for call in &row.graph.calls {
                        lines.push_str(&format!(
                            "{}  {}::{}  L{}  ← {}\n",
                            row.graph.file,
                            call.container.as_deref().unwrap_or("__top__"),
                            call.caller.as_deref().unwrap_or("__top__"),
                            call.line,
                            call.resolved.as_deref().unwrap_or(&call.target)
                        ));
                        if lines.lines().count() >= max + 2 {
                            lines.push_str(&tf!(
                                "⚠ … truncated at {max} lines\n",
                                "⚠ … levágva {max} sornál\n"
                            ));
                            return Ok(lines);
                        }
                    }
                }
                return Ok(lines);
            }
            return Ok(text);
        }
        let scope = rows
            .iter()
            .filter_map(|row| Path::new(&row.graph.file).parent().map(Path::to_path_buf))
            .collect::<Vec<_>>();
        let all = self.all_graphs(&scope)?;
        let index = CallerIndex::from_graphs(all.iter().map(|(path, graph)| FileGraphRecord {
            path: path.clone(),
            graph: graph.clone(),
        }));
        let direct_rows = rows
            .iter()
            .flat_map(|row| {
                row.graph.calls.iter().map(|call| crate::index::CallerRow {
                    file: row.graph.file.clone(),
                    container: call.container.clone(),
                    caller: call.caller.clone().unwrap_or_else(|| "__top__".to_owned()),
                    line: call.line,
                    target: call.resolved.clone().unwrap_or_else(|| call.target.clone()),
                    level: 1,
                    guessed: false,
                })
            })
            .collect::<Vec<_>>();
        let (callers, skipped) = index.parents_of_with_skips(&direct_rows, depth, guess);
        let mut output = String::new();
        use std::fmt::Write;
        for level in 1..=depth {
            let what = if level == 1 {
                tf!(
                    "directly calls: /{pattern}/",
                    "közvetlenül hívja: /{pattern}/"
                )
            } else {
                t!("calls the previous level", "az előző szintet hívja").to_owned()
            };
            output.push_str(&tf!(
                "== level {level} — {what}\n",
                "== {level}. szint — {what}\n"
            ));
            let mut direct = rows
                .iter()
                .flat_map(|row| {
                    row.graph
                        .methods
                        .iter()
                        .map(move |method| (row.graph.file.clone(), method.clone()))
                })
                .collect::<Vec<_>>();
            direct.sort_by(|(left_file, left), (right_file, right)| {
                left_file
                    .cmp(right_file)
                    .then_with(|| left.line.cmp(&right.line))
            });
            if level == 1 {
                for (file, method) in &direct {
                    writeln!(
                        output,
                        "  {}  {}::{}()  L{}-{}",
                        file,
                        method.container.as_deref().unwrap_or("__top__"),
                        method.name,
                        method.line,
                        method.end_line.unwrap_or(method.line)
                    )
                    .unwrap();
                }
            }
            let mut level_callers = callers
                .iter()
                .filter(|row| row.level == level && level > 1)
                .collect::<Vec<_>>();
            // Régi sorrend: fájl, konténer, metódus; hívónként az első (legkisebb sorú) hívás a `←` cél.
            level_callers.sort_by(|a, b| {
                (&a.file, &a.container, &a.caller, a.line).cmp(&(
                    &b.file,
                    &b.container,
                    &b.caller,
                    b.line,
                ))
            });
            level_callers.dedup_by(|b, a| {
                a.file == b.file && a.container == b.container && a.caller == b.caller
            });
            for caller in level_callers {
                let range = all
                    .iter()
                    .find(|(path, _)| path == &caller.file)
                    .and_then(|(_, graph)| {
                        graph
                            .methods
                            .iter()
                            .find(|m| m.container == caller.container && m.name == caller.caller)
                            .map(|m| (m.line, m.end_line.unwrap_or(m.line)))
                    })
                    .unwrap_or((caller.line, caller.line));
                writeln!(
                    output,
                    "  {}  {}::{}()  L{}-{}{}",
                    caller.file,
                    caller.container.as_deref().unwrap_or("__top__"),
                    caller.caller,
                    range.0,
                    range.1,
                    if level > 1 {
                        format!(
                            "  ← {}{}",
                            caller.target,
                            if caller.guessed { "  ?" } else { "" }
                        )
                    } else {
                        String::new()
                    }
                )
                .unwrap();
            }
            if (level == 1 && direct.is_empty())
                || (level > 1 && !callers.iter().any(|row| row.level == level))
            {
                output.push_str(t!("  (no more callers)\n", "  (nincs további hívó)\n"));
            }
            if level > 1
                && !guess
                && let Some(count) = skipped.get(level - 2).copied().filter(|count| *count > 0)
            {
                output.push_str(&tf!(
                    "  (+{count} unresolved, name-only matches skipped — --guess shows them)\n",
                    "  (+{count} feloldatlan, csak név szerinti egyezés kihagyva — --guess mutatja)\n"
                ));
            }
            output.push('\n');
        }
        output.push_str(t!(
            "💡 \"?\" = unresolved, name-only match (may be wrong). A caller's body: <file>::<method> --body\n",
            "💡 \"?\" = feloldatlan, csak név szerinti egyezés (lehet hamis). Egy hívó törzse: <fájl>::<metódus> --body\n"
        ));
        Ok(output)
    }
}
