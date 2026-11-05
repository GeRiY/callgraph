//! A `--grep` szöveges kimenete: találatonkénti vagy mélységi (hívók hívói) nézet.

use super::{QueryEngine, QueryResult};
use crate::error::Result;
use crate::index::CallerIndex;
use crate::query::format::{FormatterRegistry, call_lines, method_calls};
use crate::query::snippet::{self, SourceLines};
use crate::{t, tf};
use std::path::PathBuf;

impl QueryEngine {
    // a ShowCommand kapcsolói egy az egyben (egyetlen hívóhely)
    #[allow(clippy::too_many_arguments)]
    pub fn grep_text(
        &self,
        rows: &[QueryResult],
        scope: &[PathBuf],
        pattern: &str,
        depth: usize,
        guess: bool,
        full: bool,
        limit: Option<usize>,
    ) -> Result<String> {
        let lang = crate::i18n::lang();
        let source = SourceLines::default();
        if depth <= 1 {
            let text = FormatterRegistry::format("grep-outline", rows)?;
            if rows.is_empty() {
                // Nincs találat: csak a záró tipp marad (nincs fejléc-elválasztó).
                return Ok(text.trim_start_matches("\n---\n").to_owned());
            }
            let max = limit.unwrap_or_else(|| crate::query::OutputLimit::from_env(full).limit);
            if !full && text.lines().count() > max {
                let header = tf!(
                    "⚠ --grep outline would be {} lines — showing 1 line per call instead (file  caller  L<line>  ← target │ call line; identical lines merged, at most {} per caller). Narrow it down, or --full.\n\n",
                    "⚠ A --grep outline {} sor lenne — helyette hívásonként 1 sor (fájl  hívó  L<sor>  ← cél │ a hívás sora; az azonos sorok összevonva, hívónként legfeljebb {}). Szűkíts, vagy --full.\n\n",
                    text.lines().count(),
                    snippet::MAX_PER_METHOD
                );
                return Ok(flat_lines(header, rows, &source, lang, max));
            }
            return Ok(text);
        }
        // a 2. szinttől a lekérdezés teljes hatókörében keresünk (nem csak az 1. szint fájljainak mappáiban)
        let all = self.all_graphs(scope)?;
        let index = CallerIndex::build(all.iter().map(|(path, graph)| (path.as_str(), graph)));
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
                    via: row.via_of(call).map(str::to_owned),
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
                .flat_map(|row| row.graph.methods.iter().map(move |method| (row, method)))
                .collect::<Vec<_>>();
            direct.sort_by(|(left_row, left), (right_row, right)| {
                left_row
                    .graph
                    .file
                    .cmp(&right_row.graph.file)
                    .then_with(|| left.line.cmp(&right.line))
            });
            if level == 1 {
                for (row, method) in &direct {
                    writeln!(
                        output,
                        "  {}  {}::{}()  L{}-{}",
                        row.graph.file,
                        method.container.as_deref().unwrap_or("__top__"),
                        method.name,
                        method.line,
                        method.end_line.unwrap_or(method.line)
                    )
                    .unwrap();
                    let calls = method_calls(&row.graph, method);
                    call_lines(&mut output, row, &calls, &source, lang, "    ");
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
                let code = source.get(&caller.file, caller.line);
                writeln!(
                    output,
                    "    {}",
                    snippet::call_line(lang, caller.line, caller.via.as_deref(), code.as_deref())
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

/// Egy hívó metódus (konténer, név) hívásai.
type CallerCalls<'a> = (
    (Option<&'a str>, Option<&'a str>),
    Vec<&'a crate::model::Call>,
);

/// A túl hosszú `--grep` outline helyett: hívó metódusonként a hívások egy-egy sorban
/// (`fájl  Osztály::metódus  L12, L40  ← cél │ kód`), az azonos (cél, `via`, kód) hívások összevonva, legfeljebb
/// `snippet::MAX_PER_METHOD` sor, a többi `… +N more: L55, L61`; `max` sor után levágva.
fn flat_lines(
    mut out: String,
    rows: &[QueryResult],
    source: &SourceLines,
    lang: crate::i18n::Lang,
    max: usize,
) -> String {
    for row in rows {
        // hívó metódusonként, az első előfordulás sorrendjében
        let mut groups: Vec<CallerCalls<'_>> = Vec::new();
        for call in &row.graph.calls {
            let key = (call.container.as_deref(), call.caller.as_deref());
            match groups.iter_mut().find(|(group, _)| *group == key) {
                Some((_, calls)) => calls.push(call),
                None => groups.push((key, vec![call])),
            }
        }
        for ((container, caller), calls) in groups {
            let sites = snippet::merge(calls.iter().map(|call| {
                let target = call.resolved.as_deref().unwrap_or(&call.target);
                (
                    call.line,
                    (target, row.via_of(call)),
                    source.get(&row.graph.file, call.line),
                )
            }));
            let (shown, rest) = snippet::cap(sites);
            let prefix = format!(
                "{}  {}::{}",
                row.graph.file,
                container.unwrap_or("__top__"),
                caller.unwrap_or("__top__")
            );
            for site in &shown {
                let (target, via) = site.key;
                let line = format!(
                    "{prefix}  {}  ← {target}{}\n",
                    snippet::line_list(&site.lines),
                    snippet::marks(lang, via, site.code.as_deref())
                );
                if push_limited(&mut out, &line, max) {
                    return out;
                }
            }
            if !rest.is_empty() {
                let line = format!("{prefix}  {}\n", snippet::more(lang, &rest));
                if push_limited(&mut out, &line, max) {
                    return out;
                }
            }
        }
    }
    out
}

/// Hozzáfűz egy sort; ha a kimenet elérte a korlátot, a levágás jelzésével `true`.
fn push_limited(out: &mut String, line: &str, max: usize) -> bool {
    out.push_str(line);
    if out.lines().count() < max + 2 {
        return false;
    }
    out.push_str(&tf!(
        "⚠ … truncated at {max} lines\n",
        "⚠ … levágva {max} sornál\n"
    ));
    true
}
