//! A `--grep --depth N` szintenkénti nézete (1. szint: a találatok metódusai; 2+. szint: hívónként egy sor).

use super::QueryResult;
use super::grep_text::{MethodRanges, dynamic_footer, guess_legend, header_line, skipped_footer};
use crate::i18n::Lang;
use crate::index::CallerRow;
use crate::notes::NoteMarks;
use crate::profile::{Count, LayerScope};
use crate::query::format::{call_lines, method_calls};
use crate::query::snippet::{self, SourceLines};
use crate::{t, tf};
use std::fmt::Write as _;

/// A mélységi nézet bemenete: az 1. szint sorai, a 2+. szint hívói és a kiíráshoz kellő források.
pub(super) struct DepthView<'a> {
    pub(super) rows: &'a [QueryResult],
    pub(super) pattern: &'a str,
    pub(super) depth: usize,
    pub(super) guess: bool,
    pub(super) callers: &'a [CallerRow],
    pub(super) skipped: &'a [usize],
    pub(super) ranges: &'a dyn MethodRanges,
    pub(super) source: &'a SourceLines,
    pub(super) lang: Lang,
    /// `--by-layer`: a hívók rétegek szerint (a hívotthoz legközelebbi elöl), szintenként rétegcsoportokban, a
    /// csoport előtt `[service]  2 callers` fejléccel; `None` = a régi sorrend és sor.
    pub(super) layers: Option<&'a LayerScope>,
    /// A jegyzet-jelölők (a hívó-sorok végén `  📝 N`).
    pub(super) marks: &'a NoteMarks,
    /// Az 1. szint „nincs hívó” sora(i): a cél létezésének megállapítása; üres = a szokásos „nincs további hívó”.
    pub(super) no_caller: &'a str,
    /// `--full`: a teszt- / story-hívók is soronként, és nincs sor-korlát.
    pub(super) full: bool,
    /// A kimenet sor-korlátja (`--full` nélkül; fölötte levágva, a teljes sorszámmal).
    pub(super) max: usize,
    /// `--compact`: the token-cheap file-grouped view (`grep_compact`) instead of the call-line view.
    pub(super) compact: bool,
}

/// Teszt- vagy story-fájl-e az út (a mélységi nézetben ezek egy összesítő sorba vonódnak).
pub(super) fn is_test_or_story(path: &str) -> bool {
    let lower = path.replace('\\', "/").to_ascii_lowercase();
    crate::locate::rank::is_test_path(path)
        || lower.contains(".story.")
        || lower.contains(".stories.")
        || lower.contains("/stories/")
}

/// A szintenkénti nézet: 1. szint = a találatok metódusai a hívás-soraikkal, 2+. szint = hívónként egy sor
/// (`← cél`, a hívás sora), a kihagyott név szerinti egyezések száma, a végén a tipp.
pub(super) fn render_depth(view: DepthView<'_>) -> String {
    if view.compact {
        return super::grep_compact::render_compact(&view);
    }
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
        full,
        max,
        compact: _,
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
        // egy hívó több különböző célja (egy fájl `__top__` útvonal-hivatkozásai) külön sor
        let mut listed = std::collections::HashSet::new();
        level_callers.retain(|row| {
            listed.insert((
                row.file.clone(),
                row.container.clone(),
                row.caller.clone(),
                row.target.clone(),
            ))
        });
        if let Some(scope) = layers {
            level_callers.sort_by_key(|row| scope.order_key(&row.file));
        }
        // a teszt- / story-hívók szintenként egy sorba (csak ha van más hívó is; `--full` mindet mutatja)
        let mut hidden: Vec<&CallerRow> = Vec::new();
        if !full && level > 1 && level_callers.iter().any(|row| !is_test_or_story(&row.file)) {
            let (tests, rest): (Vec<&CallerRow>, Vec<&CallerRow>) =
                std::mem::take(&mut level_callers)
                    .into_iter()
                    .partition(|row| is_test_or_story(&row.file));
            hidden = tests;
            level_callers = rest;
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
                snippet::sites_line_ref(
                    lang,
                    &[caller.line],
                    caller.reference,
                    caller.via.as_deref(),
                    code.as_deref()
                )
            )
            .unwrap();
        }
        if !hidden.is_empty() {
            let files: std::collections::BTreeSet<&str> =
                hidden.iter().map(|row| row.file.as_str()).collect();
            output.push_str(&tf!(
                "  tests/stories: {} callers in {} files — --full lists them\n",
                "  tesztek/story-k: {} hívó {} fájlban — a --full felsorolja\n",
                hidden.len(),
                files.len()
            ));
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
    let total = output.lines().count();
    if !full && total > max {
        output = output.lines().take(max).collect::<Vec<_>>().join("\n");
        output.push('\n');
        output.push_str(&tf!(
            "⚠ … truncated at {max} of {total} lines — narrow it down (a subfolder, a more exact pattern, a smaller --depth) or --full\n",
            "⚠ … levágva {max} sornál, összesen {total} — szűkíts (almappa, pontosabb minta, kisebb --depth) vagy --full\n"
        ));
    }
    output.push_str(guess_legend());
    output
}
