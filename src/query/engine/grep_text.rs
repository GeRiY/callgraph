//! A `--grep` szöveges kimenete: találatonkénti vagy mélységi (hívók hívói) nézet. A 2. szinttől a hívók a
//! projekt-gráf hívó-indexéből jönnek (`crate::graph`); gráf nélkül (`CALLGRAPH_NO_GRAPH=1`, írásvédett projekt) a
//! hatókör minden fájlja bejárva — a kimenet mindkét úton bájtra azonos.

use super::{QueryEngine, QueryResult};
use crate::error::Result;
use crate::graph::ProjectGraph;
use crate::i18n::Lang;
use crate::index::{CallerIndex, CallerRow};
use crate::model::FileGraph;
use crate::notes::{NoteMarks, strip_mark};
use crate::profile::{Count, LayerRun, LayerScope, group_header};
use crate::query::format::{call_lines, grep_outline, method_calls};
use crate::query::hint::{self, Catalog};
use crate::query::snippet::{self, SourceLines};
use crate::{t, tf};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

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
        // jegyzet-jelölők (`  📝 N`): egyszer betöltve; jegyzetfájl nélkül üres
        let marks = NoteMarks::load(self.root());
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
            // nulla találat: a generikus „törzs-logika” tipp helyett a helyreállító üzenet
            let base = if rows.is_empty() {
                hint::empty_grep(lang, pattern, &self.take_scope_catalog())
            } else {
                let layers = self.layers.as_ref().filter(|scope| scope.by_layer());
                grep_flat(rows, &source, lang, full, limit, layers, &marks)
            };
            // `Típus::metódus` mintánál a név szerinti (feloldatlan fogadójú) hívók is szóba jönnek
            let wanted = crate::index::pattern_name(pattern);
            let whole_project = scope == self.depth_scope().as_slice();
            if let Some(graph) = project
                .as_deref_mut()
                .filter(|_| wanted.is_some() && whole_project)
            {
                let files = self.walk(scope);
                graph.refresh(&files, true, &self.analyzer)?;
                graph.ensure_callers(&self.analyzer)?;
                let _ = graph.save();
                let graph: &ProjectGraph = graph;
                let name_only = wanted.map_or_else(Vec::new, |name| {
                    graph.caller_index().name_only_callers(name)
                });
                for caller in &name_only {
                    if let Some(snippets) = graph.snippets(&caller.file) {
                        source.remember(&caller.file, snippets);
                    }
                }
                return Ok(render_single(SingleView {
                    rows,
                    base,
                    guess,
                    name_only: &name_only,
                    ranges: graph,
                    source: &source,
                    lang,
                    marks: &marks,
                }));
            }
            if let Some(graph) = project.as_deref().filter(|_| wanted.is_none()) {
                return Ok(render_single(SingleView {
                    rows,
                    base,
                    guess,
                    name_only: &[],
                    ranges: graph,
                    source: &source,
                    lang,
                    marks: &marks,
                }));
            }
            // gráf nélkül (vagy részhatókörön): a hatókör gráfjai, vagy csak a találat-fájlok
            let (name_only, owned) = match wanted {
                Some(name) => {
                    let all = self.all_graphs(scope)?;
                    let index =
                        CallerIndex::build(all.iter().map(|(path, graph)| (path.as_str(), graph)));
                    (index.name_only_callers(name), all)
                }
                None => (
                    Vec::new(),
                    rows.iter()
                        .filter_map(|row| {
                            let path = row.graph.file.as_str();
                            Some((path.to_owned(), self.analyze(Path::new(path)).ok()?))
                        })
                        .collect::<Vec<_>>(),
                ),
            };
            return Ok(render_single(SingleView {
                rows,
                base,
                guess,
                name_only: &name_only,
                ranges: &owned,
                source: &source,
                lang,
                marks: &marks,
            }));
        }
        let direct = direct_rows(rows);
        let whole_project = scope == self.depth_scope().as_slice();
        if let Some(graph) = project.as_deref_mut().filter(|_| whole_project) {
            // a 2. szinttől a teljes projektben keresünk: minden fájl bélyege ellenőrizve (csak a változottak
            // elemződnek), a hívók a `facts.bin`-ből — a packet csak a változott fájlokhoz kell olvasni; a
            // hívó-index a folyamatban megmarad, amíg nem változik semmi
            let files = self.walk(scope);
            graph.refresh(&files, true, &self.analyzer)?;
            graph.ensure_callers(&self.analyzer)?;
            let _ = graph.save();
            let graph: &ProjectGraph = graph;
            // a 2+. szint a teljes hatókört olvasta: a hibás fájlai is a záró sorba
            for file in &files {
                if let Some(stored) = graph.slim_graph(&file.to_string_lossy()) {
                    self.note_parse_issue(file, stored);
                }
            }
            let (callers, skipped) = graph
                .caller_index()
                .parents_of_with_skips(&direct, depth, guess);
            for caller in &callers {
                if let Some(snippets) = graph.snippets(&caller.file) {
                    source.remember(&caller.file, snippets);
                }
            }
            // az 1. szinten nincs hívó: létezik-e a cél (a teljes projekt gráfjából)
            let no_caller = if rows.is_empty() {
                let catalog = Catalog::from_graphs(
                    files
                        .iter()
                        .filter_map(|file| graph.slim_graph(&file.to_string_lossy())),
                );
                no_caller_hint(lang, pattern, &catalog)
            } else {
                String::new()
            };
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
                layers: self.layers.as_ref().filter(|scope| scope.by_layer()),
                marks: &marks,
                no_caller: &no_caller,
            }));
        }
        // gráf nélkül: a lekérdezés teljes hatókörének minden fájlja (nem csak az 1. szint fájljainak mappái)
        let all = self.all_graphs(scope)?;
        for (path, graph) in &all {
            self.note_parse_issue(Path::new(path), graph);
        }
        let index = CallerIndex::build(all.iter().map(|(path, graph)| (path.as_str(), graph)));
        let (callers, skipped) = index.parents_of_with_skips(&direct, depth, guess);
        let no_caller = if rows.is_empty() {
            let catalog = Catalog::from_graphs(all.iter().map(|(_, graph)| graph));
            no_caller_hint(lang, pattern, &catalog)
        } else {
            String::new()
        };
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
            layers: self.layers.as_ref().filter(|scope| scope.by_layer()),
            marks: &marks,
            no_caller: &no_caller,
        }))
    }
}

/// A `--depth` 1. szintjének üzenete, ha nincs hívó: létezik-e a cél (a minta a teljes katalógusban), és mire hasonlít.
fn no_caller_hint(lang: Lang, pattern: &str, catalog: &Catalog) -> String {
    let presence = crate::query::filter::pattern_regex(pattern)
        .map_or(hint::Presence::Missing, |regex| catalog.presence(&regex));
    hint::no_caller(lang, pattern, presence, catalog)
}

/// Egy hívó metódus sortartománya (a projekt-gráf szimbólumaiból, vagy a bejárt gráfokból).
trait MethodRanges {
    fn range(&self, file: &str, container: Option<&str>, name: &str) -> Option<(u32, u32)>;
    /// Van-e a metódusnak dinamikus (`dynamic_call`) hívása — a `--grep` szűrő előtti, teljes gráfból.
    fn has_dynamic(&self, file: &str, container: Option<&str>, name: &str) -> bool;
}

/// Egy hívás a megadott (konténer, metódus) hívója-e (`__top__` = nincs hívó metódus).
fn call_in(call: &crate::model::Call, container: Option<&str>, name: &str) -> bool {
    call.container.as_deref() == container && call.caller.as_deref().unwrap_or("__top__") == name
}

impl MethodRanges for ProjectGraph {
    fn range(&self, file: &str, container: Option<&str>, name: &str) -> Option<(u32, u32)> {
        self.method_range(file, container, name)
    }
    fn has_dynamic(&self, file: &str, container: Option<&str>, name: &str) -> bool {
        self.has_dynamic_call(file, container, name)
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
    fn has_dynamic(&self, file: &str, container: Option<&str>, name: &str) -> bool {
        self.iter()
            .find(|(path, _)| path.as_str() == file)
            .is_some_and(|(_, graph)| {
                graph
                    .calls
                    .iter()
                    .any(|call| call.is_dynamic() && call_in(call, container, name))
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
    /// `--by-layer`: a hívók rétegek szerint (a hívotthoz legközelebbi elöl), szintenként rétegcsoportokban, a
    /// csoport előtt `[service]  2 callers` fejléccel; `None` = a régi sorrend és sor.
    layers: Option<&'a LayerScope>,
    /// A jegyzet-jelölők (a hívó-sorok végén `  📝 N`).
    marks: &'a NoteMarks,
    /// Az 1. szint „nincs hívó” sora(i): a cél létezésének megállapítása; üres = a szokásos „nincs további hívó”.
    no_caller: &'a str,
}

/// A `--by-layer` csoport-fejléce egy sorban (`behúzás` + `[service]  3 calls` + újsor).
fn header_line(indent: &str, label: &str, n: usize, count: Count, lang: Lang) -> String {
    format!("{indent}{}\n", group_header(label, n, count, lang))
}

/// A rendezett sorok csoportjai (`--by-layer` nélkül üres).
fn row_runs(rows: &[QueryResult], layers: Option<&LayerScope>) -> Vec<LayerRun> {
    layers.map_or_else(Vec::new, |scope| {
        scope.runs(rows.iter().map(|row| row.graph.file.as_str()))
    })
}

/// A csoport találat-hívásainak száma.
fn calls_in(rows: &[QueryResult], run: &LayerRun) -> usize {
    rows[run.range.clone()]
        .iter()
        .map(|row| row.graph.calls.len())
        .sum()
}

/// A grep-outline-ba a csoportok első `FILE` sora elé a fejléc.
fn with_headers(text: &str, rows: &[QueryResult], runs: &[LayerRun], lang: Lang) -> String {
    let mut out = String::with_capacity(text.len() + runs.len() * 32);
    let mut next = runs.iter().peekable();
    for line in text.split_inclusive('\n') {
        if let Some(run) = next.peek()
            && let Some(rest) = line
                .trim_end_matches('\n')
                .strip_prefix("FILE  ")
                .and_then(|rest| rest.strip_prefix(rows[run.range.start].graph.file.as_str()))
            && (rest.is_empty() || rest.starts_with("  NS:") || strip_mark(rest).is_empty())
        {
            let n = calls_in(rows, run);
            out.push_str(&header_line("", &run.label, n, Count::Calls, lang));
            next.next();
        }
        out.push_str(line);
    }
    out
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

/// `--grep` mélység nélkül: a grep-outline; ha túl hosszú, hívásonként egy sor (`flat_lines`). `--by-layer`-rel
/// (a sorok már rétegek szerint rendezve) mindkét alakban rétegenként egy `[service]  25 calls` fejléc.
fn grep_flat(
    rows: &[QueryResult],
    source: &SourceLines,
    lang: Lang,
    full: bool,
    limit: Option<usize>,
    layers: Option<&LayerScope>,
    marks: &NoteMarks,
) -> String {
    let text = grep_outline(rows, lang, source, Some(marks));
    if rows.is_empty() {
        // Nincs találat: csak a záró tipp marad (nincs fejléc-elválasztó).
        return text.trim_start_matches("\n---\n").to_owned();
    }
    let runs = row_runs(rows, layers);
    let text = if runs.is_empty() {
        text
    } else {
        with_headers(&text, rows, &runs, lang)
    };
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
    flat_lines(header, rows, source, lang, max, &runs, marks)
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
        layers,
        marks,
        no_caller,
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
        if let Some(scope) = layers {
            direct.sort_by_key(|(row, _)| scope.order_key(&row.graph.file));
        }
        if level == 1 {
            let runs = layers.map_or_else(Vec::new, |scope| {
                scope.runs(direct.iter().map(|(row, _)| row.graph.file.as_str()))
            });
            for (at, (row, method)) in direct.iter().enumerate() {
                if let Some(run) = runs.iter().find(|run| run.range.start == at) {
                    let n: usize = direct[run.range.clone()]
                        .iter()
                        .map(|(row, method)| method_calls(&row.graph, method).len())
                        .sum();
                    output.push_str(&header_line("  ", &run.label, n, Count::Calls, lang));
                }
                writeln!(
                    output,
                    "  {}  {}::{}()  L{}-{}{}",
                    row.graph.file,
                    method.container.as_deref().unwrap_or("__top__"),
                    method.name,
                    method.line,
                    method.end_line.unwrap_or(method.line),
                    marks.caller_mark(
                        &row.graph.file,
                        method.container.as_deref(),
                        Some(method.name.as_str())
                    )
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
        if let Some(scope) = layers {
            level_callers.sort_by_key(|row| scope.order_key(&row.file));
        }
        let runs = layers.map_or_else(Vec::new, |scope| {
            scope.runs(level_callers.iter().map(|row| row.file.as_str()))
        });
        for (at, caller) in level_callers.iter().enumerate() {
            if let Some(run) = runs.iter().find(|run| run.range.start == at) {
                let n = run.range.len();
                output.push_str(&header_line("  ", &run.label, n, Count::Callers, lang));
            }
            let range = ranges
                .range(&caller.file, caller.container.as_deref(), &caller.caller)
                .unwrap_or((caller.line, caller.line));
            writeln!(
                output,
                "  {}  {}::{}()  L{}-{}{}{}",
                caller.file,
                caller.container.as_deref().unwrap_or("__top__"),
                caller.caller,
                range.0,
                range.1,
                marks.caller_mark(
                    &caller.file,
                    caller.container.as_deref(),
                    Some(caller.caller.as_str())
                ),
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
            if level == 1 && !no_caller.is_empty() {
                output.push_str(no_caller);
            } else {
                output.push_str(t!("  (no more callers)\n", "  (nincs további hívó)\n"));
            }
        }
        if level > 1
            && !guess
            && let Some(count) = skipped.get(level - 2).copied().filter(|count| *count > 0)
        {
            output.push_str(&skipped_footer(count));
        }
        // a szint hívó-metódusai közül hánynak van dinamikus (`dynamic_call`) hívása: azok kézzel nézendők
        let mut level_methods: Vec<(&str, Option<&str>, &str)> = if level == 1 {
            direct
                .iter()
                .map(|(row, method)| {
                    (
                        row.graph.file.as_str(),
                        method.container.as_deref(),
                        method.name.as_str(),
                    )
                })
                .collect()
        } else {
            level_callers
                .iter()
                .map(|c| (c.file.as_str(), c.container.as_deref(), c.caller.as_str()))
                .collect()
        };
        level_methods.sort_unstable();
        level_methods.dedup();
        let dynamic = level_methods
            .iter()
            .filter(|(file, container, name)| ranges.has_dynamic(file, *container, name))
            .count();
        if dynamic > 0 {
            output.push_str(&dynamic_footer(dynamic));
        }
        output.push('\n');
    }
    output.push_str(guess_legend());
    output
}

/// A kihagyott, csak név szerint illeszkedő hívók lábléc-sora (a mélységi és az egyszintű nézetben azonos).
fn skipped_footer(count: usize) -> String {
    tf!(
        "  (+{count} unresolved, name-only matches skipped — --guess shows them)\n",
        "  (+{count} feloldatlan, csak név szerinti egyezés kihagyva — --guess mutatja)\n"
    )
}

/// A dinamikus hívást tartalmazó hívók lábléc-sora (a mélységi és az egyszintű nézetben azonos).
fn dynamic_footer(dynamic: usize) -> String {
    tf!(
        "  ({dynamic} of these callers contain dynamic calls — check them by hand)\n",
        "  ({dynamic} hívó dinamikus hívást tartalmaz — kézzel nézd át)\n"
    )
}

/// A `?` jelölés magyarázata a kimenet végén.
fn guess_legend() -> &'static str {
    t!(
        "💡 \"?\" = unresolved, name-only match (may be wrong). A caller's body: <file>::<method> --body\n",
        "💡 \"?\" = feloldatlan, csak név szerinti egyezés (lehet hamis). Egy hívó törzse: <fájl>::<metódus> --body\n"
    )
}

/// Az egyszintű `--grep` bemenete: a megszokott kimenet (`base`), és a név szerinti (feloldatlan) hívók.
struct SingleView<'a> {
    rows: &'a [QueryResult],
    base: String,
    guess: bool,
    name_only: &'a [CallerRow],
    ranges: &'a dyn MethodRanges,
    source: &'a SourceLines,
    lang: Lang,
    marks: &'a NoteMarks,
}

/// Az egyszintű nézet: `--guess` nélkül a régi kimenet + a két lábléc-sor (ha van mit mondani); `--guess`-szel a
/// név szerinti hívók is, `?` jelzéssel (mint a mélységi nézetben).
fn render_single(view: SingleView<'_>) -> String {
    let SingleView {
        rows,
        base,
        guess,
        name_only,
        ranges,
        source,
        lang,
        marks,
    } = view;
    // a már (pontosan) találatként látható hívó metódusok nem számítanak újra név szerinti hívónak
    let shown = |row: &CallerRow| {
        rows.iter().any(|found| {
            found.graph.file == row.file
                && found.graph.calls.iter().any(|call| {
                    call.container == row.container
                        && call.caller.as_deref().unwrap_or("__top__") == row.caller
                })
        })
    };
    let mut extra: Vec<&CallerRow> = name_only.iter().filter(|row| !shown(row)).collect();
    extra.sort_by(|a, b| {
        (&a.file, &a.container, &a.caller, a.line).cmp(&(&b.file, &b.container, &b.caller, b.line))
    });
    extra.dedup_by(|b, a| a.file == b.file && a.container == b.container && a.caller == b.caller);
    // a lábléc-sorok: a kihagyott név szerinti hívók, és a dinamikus hívást tartalmazó hívók
    let mut footer = String::new();
    if !guess && !extra.is_empty() {
        footer.push_str(&skipped_footer(extra.len()));
    }
    let mut methods: Vec<(&str, Option<&str>, &str)> = rows
        .iter()
        .flat_map(|row| {
            row.graph.methods.iter().map(move |method| {
                (
                    row.graph.file.as_str(),
                    method.container.as_deref(),
                    method.name.as_str(),
                )
            })
        })
        .collect();
    if guess {
        methods.extend(extra.iter().map(|row| {
            (
                row.file.as_str(),
                row.container.as_deref(),
                row.caller.as_str(),
            )
        }));
    }
    methods.sort_unstable();
    methods.dedup();
    let dynamic = methods
        .iter()
        .filter(|(file, container, name)| ranges.has_dynamic(file, *container, name))
        .count();
    if dynamic > 0 {
        footer.push_str(&dynamic_footer(dynamic));
    }
    // `--guess`: a név szerinti hívók, hívónként egy sor (`← cél  ?`) és a hívás sora
    let mut guessed = String::new();
    if guess && !extra.is_empty() {
        guessed.push_str(t!(
            "== unresolved, name-only matches (may be wrong)\n",
            "== feloldatlan, csak név szerinti egyezések (lehetnek hamisak)\n"
        ));
        for caller in &extra {
            let range = ranges
                .range(&caller.file, caller.container.as_deref(), &caller.caller)
                .unwrap_or((caller.line, caller.line));
            writeln!(
                guessed,
                "  {}  {}::{}()  L{}-{}{}  ← {}  ?",
                caller.file,
                caller.container.as_deref().unwrap_or("__top__"),
                caller.caller,
                range.0,
                range.1,
                marks.caller_mark(
                    &caller.file,
                    caller.container.as_deref(),
                    Some(caller.caller.as_str())
                ),
                caller.target
            )
            .unwrap();
            let code = source.get(&caller.file, caller.line);
            writeln!(
                guessed,
                "    {}",
                snippet::call_line(lang, caller.line, None, code.as_deref())
            )
            .unwrap();
        }
        guessed.push_str(&footer);
        guessed.push_str(guess_legend());
        footer.clear();
    }
    if rows.is_empty() {
        // nincs pontos találat: a név szerintiek (ha vannak) a „nincs találat” tipp helyett, különben a lábléc a tipp elé
        return if guessed.is_empty() {
            format!("{footer}{base}")
        } else {
            guessed
        };
    }
    // a lábléc-sorok és a `?` szakasz a záró tipp (`---` + `💡 …`) elé kerülnek
    let add = if guessed.is_empty() {
        footer
    } else {
        format!("\n{guessed}")
    };
    match base.rfind("\n---\n💡") {
        Some(at) if !add.is_empty() => {
            let mut out = base[..at].to_owned();
            if !out.ends_with('\n') {
                out.push('\n');
            }
            out.push_str(&add);
            out.push_str(&base[at..]);
            out
        }
        _ => format!("{base}{add}"),
    }
}

/// Egy hívó metódus (konténer, név) hívásai.
type CallerCalls<'a> = (
    (Option<&'a str>, Option<&'a str>),
    Vec<&'a crate::model::Call>,
);

/// A túl hosszú `--grep` outline helyett: hívó metódusonként a hívások egy-egy sorban
/// (`fájl  Osztály::metódus  L12, L40  ← cél │ kód`), az azonos (cél, `via`, kód) hívások összevonva, legfeljebb
/// `snippet::MAX_PER_METHOD` sor, a többi `… +N more: L55, L61`; `max` sor után levágva. `runs` (`--by-layer`):
/// a rétegcsoportok első sora előtt a `[service]  25 calls` fejléc.
fn flat_lines(
    mut out: String,
    rows: &[QueryResult],
    source: &SourceLines,
    lang: crate::i18n::Lang,
    max: usize,
    runs: &[LayerRun],
    marks: &NoteMarks,
) -> String {
    for (at, row) in rows.iter().enumerate() {
        if let Some(run) = runs.iter().find(|run| run.range.start == at) {
            let n = calls_in(rows, run);
            let line = header_line("", &run.label, n, Count::Calls, lang);
            if push_limited(&mut out, &line, max) {
                return out;
            }
        }
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
                "{}  {}::{}{}",
                row.graph.file,
                container.unwrap_or("__top__"),
                caller.unwrap_or("__top__"),
                marks.caller_mark(&row.graph.file, container, caller)
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
