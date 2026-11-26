//! A `--grep` szöveges kimenete: találatonkénti vagy mélységi (hívók hívói) nézet. A 2. szinttől a hívók a
//! projekt-gráf hívó-indexéből jönnek (`crate::graph`); gráf nélkül (`CALLGRAPH_NO_GRAPH=1`, írásvédett projekt) a
//! hatókör minden fájlja bejárva — a kimenet mindkét úton bájtra azonos.

use super::grep_depth::{DepthView, render_depth};
use super::grep_flat::grep_flat;
use super::{QueryEngine, QueryResult};
use crate::error::Result;
use crate::graph::ProjectGraph;
use crate::i18n::Lang;
use crate::index::{CallerIndex, CallerRow};
use crate::model::FileGraph;
use crate::notes::{NoteMarks, strip_mark};
use crate::profile::{Count, LayerRun, LayerScope, group_header};
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
            let mut base = if rows.is_empty() {
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
                let direct = direct_rows(rows);
                let mut wired = graph.caller_index().wire_invokers(&direct);
                wired.extend(rewritten_rows(graph.caller_index(), pattern, &direct));
                for caller in &wired {
                    if let Some(snippets) = graph.snippets(&caller.file) {
                        source.remember(&caller.file, snippets);
                    }
                }
                let base = with_wired(base, rows.is_empty(), &wired, graph, &source, lang, &marks);
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
                    let direct = direct_rows(rows);
                    let mut wired = index.wire_invokers(&direct);
                    wired.extend(rewritten_rows(&index, pattern, &direct));
                    base = with_wired(base, rows.is_empty(), &wired, &all, &source, lang, &marks);
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
                full,
                max: limit.unwrap_or_else(|| crate::query::OutputLimit::from_env(full).limit),
                compact: self.compact,
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
            full,
            max: limit.unwrap_or_else(|| crate::query::OutputLimit::from_env(full).limit),
            compact: self.compact,
        }))
    }
}

/// Az index által átírt célú (fájlok közötti lánc / trait / útvonal) hívók, amelyek a grep-sorok között még nincsenek.
fn rewritten_rows(index: &CallerIndex, pattern: &str, direct: &[CallerRow]) -> Vec<CallerRow> {
    index
        .rewritten(pattern)
        .into_iter()
        .filter(|row| {
            !direct.iter().any(|d| {
                d.file == row.file
                    && d.container == row.container
                    && d.caller == row.caller
                    && d.line == row.line
            })
        })
        .collect()
}

/// A bekötött closure-t meghívó metódusok szakasza (`== via stored closure`) a kimenet végére
/// szúrva; bekötés nélkül a kimenet változatlan.
fn with_wired(
    mut base: String,
    no_direct_rows: bool,
    wired: &[CallerRow],
    ranges: &dyn MethodRanges,
    source: &SourceLines,
    lang: Lang,
    marks: &NoteMarks,
) -> String {
    if wired.is_empty() {
        return base;
    }
    // nincs közvetlen találat, de van feloldott (átírt / bekötött) hívó: a „nincs találat” tipp félrevezető lenne
    if no_direct_rows {
        base.clear();
    }
    let mut add = String::from(t!(
        "== called through a stored closure or resolved across files\n",
        "== tárolt closure-on át vagy fájlok között feloldva\n"
    ));
    for caller in wired {
        let range = ranges
            .range(&caller.file, caller.container.as_deref(), &caller.caller)
            .unwrap_or((caller.line, caller.line));
        let code = source.get(&caller.file, caller.line);
        let _ = writeln!(
            add,
            "  {}  {}::{}()  L{}-{}{}  ← {}\n    {}",
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
            caller.target,
            snippet::sites_line_ref(
                lang,
                &[caller.line],
                false,
                caller.via.as_deref(),
                code.as_deref()
            )
        );
    }
    format!("{base}{add}")
}

/// A `--depth` 1. szintjének üzenete, ha nincs hívó: létezik-e a cél (a minta a teljes katalógusban), és mire hasonlít.
fn no_caller_hint(lang: Lang, pattern: &str, catalog: &Catalog) -> String {
    let presence = crate::query::filter::pattern_regex(pattern)
        .map_or(hint::Presence::Missing, |regex| catalog.presence(&regex));
    hint::no_caller(lang, pattern, presence, catalog)
}

/// Egy hívó metódus sortartománya (a projekt-gráf szimbólumaiból, vagy a bejárt gráfokból).
pub(super) trait MethodRanges {
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

/// A `--by-layer` csoport-fejléce egy sorban (`behúzás` + `[service]  3 calls` + újsor).
pub(super) fn header_line(indent: &str, label: &str, n: usize, count: Count, lang: Lang) -> String {
    format!("{indent}{}\n", group_header(label, n, count, lang))
}

/// A rendezett sorok csoportjai (`--by-layer` nélkül üres).
pub(super) fn row_runs(rows: &[QueryResult], layers: Option<&LayerScope>) -> Vec<LayerRun> {
    layers.map_or_else(Vec::new, |scope| {
        scope.runs(rows.iter().map(|row| row.graph.file.as_str()))
    })
}

/// A csoport találat-hívásainak száma.
pub(super) fn calls_in(rows: &[QueryResult], run: &LayerRun) -> usize {
    rows[run.range.clone()]
        .iter()
        .map(|row| row.graph.calls.len())
        .sum()
}

/// A grep-outline-ba a csoportok első `FILE` sora elé a fejléc.
pub(super) fn with_headers(
    text: &str,
    rows: &[QueryResult],
    runs: &[LayerRun],
    lang: Lang,
) -> String {
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
pub(super) fn direct_rows(rows: &[QueryResult]) -> Vec<CallerRow> {
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
                reference: call.is_reference(),
            })
        })
        .collect()
}

/// A kihagyott, csak név szerint illeszkedő hívók lábléc-sora (a mélységi és az egyszintű nézetben azonos).
pub(super) fn skipped_footer(count: usize) -> String {
    tf!(
        "  (+{count} unresolved, name-only matches skipped — --guess shows them)\n",
        "  (+{count} feloldatlan, csak név szerinti egyezés kihagyva — --guess mutatja)\n"
    )
}

/// A dinamikus hívást tartalmazó hívók lábléc-sora (a mélységi és az egyszintű nézetben azonos).
pub(super) fn dynamic_footer(dynamic: usize) -> String {
    tf!(
        "  ({dynamic} of these callers contain dynamic calls — check them by hand)\n",
        "  ({dynamic} hívó dinamikus hívást tartalmaz — kézzel nézd át)\n"
    )
}

/// A `?` jelölés magyarázata a kimenet végén.
pub(super) fn guess_legend() -> &'static str {
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
                snippet::sites_line_ref(
                    lang,
                    &[caller.line],
                    caller.reference,
                    None,
                    code.as_deref()
                )
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
    let add = if guessed.is_empty() { footer } else { guessed };
    format!("{base}{add}")
}
