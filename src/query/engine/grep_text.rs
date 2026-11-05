//! A `--grep` szöveges kimenete: találatonkénti vagy mélységi (hívók hívói) nézet. A 2. szinttől a hívók a
//! projekt-gráf hívó-indexéből jönnek (`crate::graph`); gráf nélkül (`CALLGRAPH_NO_GRAPH=1`, írásvédett projekt) a
//! hatókör minden fájlja bejárva — a kimenet mindkét úton bájtra azonos.

use super::{QueryEngine, QueryResult};
use crate::error::Result;
use crate::graph::ProjectGraph;
use crate::i18n::Lang;
use crate::index::{CallerIndex, CallerRow};
use crate::model::FileGraph;
use crate::query::format::{call_lines, grep_outline, method_calls};
use crate::query::snippet::{self, SourceLines};
use crate::{t, tf};
use std::fmt::Write as _;
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
        let shared = self.project_graph();
        let mut project = shared.as_ref().map(crate::graph::lock);
        // a hívás-sorok a projekt-gráfból (a fájlok újraolvasása nélkül)
        let mut source = SourceLines::default();
        if let Some(graph) = project.as_deref() {
            for row in rows {
                if let Some(snippets) = graph.snippets(&row.graph.file) {
                    source.remember(&row.graph.file, snippets);
                }
            }
        }
        if depth <= 1 {
            return Ok(grep_flat(rows, &source, lang, full, limit));
        }
        let direct = direct_rows(rows);
        let whole_project = scope == self.depth_scope().as_slice();
        if let Some(graph) = project.as_deref_mut().filter(|_| whole_project) {
            // a 2. szinttől a teljes projektben keresünk: a gráf naprakészre hozva (csak a változott fájlok
            // elemződnek), a hívó-index a folyamatban megmarad, amíg nem változik semmi
            let files = self.walk(scope);
            let mut cache = self.cache();
            graph.sync(&files, true, &self.analyzer, &mut cache)?;
            let _ = graph.save();
            graph.ensure_callers();
            let graph: &ProjectGraph = graph;
            let (callers, skipped) = graph
                .caller_index()
                .parents_of_with_skips(&direct, depth, guess);
            for caller in &callers {
                if let Some(snippets) = graph.snippets(&caller.file) {
                    source.remember(&caller.file, snippets);
                }
            }
            return Ok(render_depth(DepthView {
                rows,
                pattern,
                depth,
                guess,
                callers: &callers,
                skipped: &skipped,
                ranges: graph,
                source: &source,
                lang,
            }));
        }
        // gráf nélkül: a lekérdezés teljes hatókörének minden fájlja (nem csak az 1. szint fájljainak mappái)
        let all = self.all_graphs(scope)?;
        let index = CallerIndex::build(all.iter().map(|(path, graph)| (path.as_str(), graph)));
        let (callers, skipped) = index.parents_of_with_skips(&direct, depth, guess);
        Ok(render_depth(DepthView {
            rows,
            pattern,
            depth,
            guess,
            callers: &callers,
            skipped: &skipped,
            ranges: &all,
            source: &source,
            lang,
        }))
    }
}

/// Egy hívó metódus sortartománya (a projekt-gráf szimbólumaiból, vagy a bejárt gráfokból).
trait MethodRanges {
    fn range(&self, file: &str, container: Option<&str>, name: &str) -> Option<(u32, u32)>;
}

impl MethodRanges for ProjectGraph {
    fn range(&self, file: &str, container: Option<&str>, name: &str) -> Option<(u32, u32)> {
        self.method_range(file, container, name)
    }
}

impl MethodRanges for Vec<(String, FileGraph)> {
    fn range(&self, file: &str, container: Option<&str>, name: &str) -> Option<(u32, u32)> {
        self.iter()
            .find(|(path, _)| path.as_str() == file)
            .and_then(|(_, graph)| {
                graph
                    .methods
                    .iter()
                    .find(|m| m.container.as_deref() == container && m.name == name)
                    .map(|m| (m.line, m.end_line.unwrap_or(m.line)))
            })
    }
}

/// A mélységi nézet bemenete: az 1. szint sorai, a 2+. szint hívói és a kiíráshoz kellő források.
struct DepthView<'a> {
    rows: &'a [QueryResult],
    pattern: &'a str,
    depth: usize,
    guess: bool,
    callers: &'a [CallerRow],
    skipped: &'a [usize],
    ranges: &'a dyn MethodRanges,
    source: &'a SourceLines,
    lang: Lang,
}

/// Az 1. szint hívásai a hívó-index lekérdezéséhez (`level: 1`).
fn direct_rows(rows: &[QueryResult]) -> Vec<CallerRow> {
    rows.iter()
        .flat_map(|row| {
            row.graph.calls.iter().map(|call| CallerRow {
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
        .collect()
}

/// `--grep` mélység nélkül: a grep-outline; ha túl hosszú, hívásonként egy sor (`flat_lines`).
fn grep_flat(
    rows: &[QueryResult],
    source: &SourceLines,
    lang: Lang,
    full: bool,
    limit: Option<usize>,
) -> String {
    let text = grep_outline(rows, lang, source);
    if rows.is_empty() {
        // Nincs találat: csak a záró tipp marad (nincs fejléc-elválasztó).
        return text.trim_start_matches("\n---\n").to_owned();
    }
    let max = limit.unwrap_or_else(|| crate::query::OutputLimit::from_env(full).limit);
    if full || text.lines().count() <= max {
        return text;
    }
    let header = tf!(
        "⚠ --grep outline would be {} lines — showing 1 line per call instead (file  caller  L<line>  ← target │ call line; identical lines merged, at most {} per caller). Narrow it down, or --full.\n\n",
        "⚠ A --grep outline {} sor lenne — helyette hívásonként 1 sor (fájl  hívó  L<sor>  ← cél │ a hívás sora; az azonos sorok összevonva, hívónként legfeljebb {}). Szűkíts, vagy --full.\n\n",
        text.lines().count(),
        snippet::MAX_PER_METHOD
    );
    flat_lines(header, rows, source, lang, max)
}

/// A szintenkénti nézet: 1. szint = a találatok metódusai a hívás-soraikkal, 2+. szint = hívónként egy sor
/// (`← cél`, a hívás sora), a kihagyott név szerinti egyezések száma, a végén a tipp.
fn render_depth(view: DepthView<'_>) -> String {
    let DepthView {
        rows,
        pattern,
        depth,
        guess,
        callers,
        skipped,
        ranges,
        source,
        lang,
    } = view;
    let mut output = String::new();
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
                call_lines(&mut output, row, &calls, source, lang, "    ");
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
            let range = ranges
                .range(&caller.file, caller.container.as_deref(), &caller.caller)
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
    output
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
