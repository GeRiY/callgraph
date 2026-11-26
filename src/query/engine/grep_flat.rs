//! A `--grep` outline kompakt (hívásonként egy sor) alakja, ha az outline túl hosszú.

use super::QueryResult;
use super::grep_text::{calls_in, header_line, row_runs, with_headers};
use crate::i18n::Lang;
use crate::notes::NoteMarks;
use crate::profile::{Count, LayerRun, LayerScope};
use crate::query::format::{grep_outline, shown_line};
use crate::query::snippet::{self, SourceLines};
use crate::tf;

/// `--grep` mélység nélkül: a grep-outline; ha túl hosszú, hívásonként egy sor (`flat_lines`). `--by-layer`-rel
/// (a sorok már rétegek szerint rendezve) mindkét alakban rétegenként egy `[service]  25 calls` fejléc.
pub(super) fn grep_flat(
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
        return text;
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
        "⚠ --grep outline would be {} lines — showing 1 line per call instead (file header, then caller  L<line>  ← target │ call line; the target is stated once when it is the same everywhere; identical lines merged, at most {} per caller). Narrow it down, or --full.\n",
        "⚠ A --grep outline {} sor lenne — helyette hívásonként 1 sor (fájl-fejsor, alatta hívó  L<sor>  ← cél │ a hívás sora; ha a cél mindenhol ugyanaz, egyszer áll; az azonos sorok összevonva, hívónként legfeljebb {}). Szűkíts, vagy --full.\n",
        text.lines().count(),
        snippet::MAX_PER_METHOD
    );
    flat_lines(header, rows, source, lang, max, &runs, marks)
}

/// Egy hívó metódus (konténer, név) hívásai.
type CallerCalls<'a> = (
    (Option<&'a str>, Option<&'a str>),
    Vec<&'a crate::model::Call>,
);

/// A túl hosszú `--grep` outline helyett: hívó metódusonként a hívások egy-egy sorban
/// fájlonként egy fejsor alatt (`  Osztály::metódus  L12, L40  ← cél │ kód`; ha a cél az egész kimenetben
/// ugyanaz, a `← cél` egyszer áll az elején), az azonos (cél, `via`, kód) hívások összevonva, legfeljebb
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
    // egyetlen cél esetén a `← cél` egyszer, a fejlécben
    let mut targets = rows
        .iter()
        .flat_map(|row| row.graph.calls.iter())
        .map(|call| call.resolved.as_deref().unwrap_or(&call.target));
    let single = targets
        .next()
        .filter(|first| targets.all(|target| target == *first));
    if let Some(target) = single {
        let line = format!("← {target}\n");
        if push_limited(&mut out, &line, max) {
            return out;
        }
    }
    for (at, row) in rows.iter().enumerate() {
        if let Some(run) = runs.iter().find(|run| run.range.start == at) {
            let n = calls_in(rows, run);
            let line = header_line("", &run.label, n, Count::Calls, lang);
            if push_limited(&mut out, &line, max) {
                return out;
            }
        }
        if !row.graph.calls.is_empty() {
            let line = format!("{}\n", row.graph.file);
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
                let line = shown_line(source, &row.graph.file, &calls, call);
                (
                    line,
                    (target, row.via_of(call), call.is_reference()),
                    source.get(&row.graph.file, line),
                )
            }));
            let (shown, rest) = snippet::cap(sites);
            let prefix = format!(
                "  {}::{}{}",
                container.unwrap_or("__top__"),
                caller.unwrap_or("__top__"),
                marks.caller_mark(&row.graph.file, container, caller)
            );
            for site in &shown {
                let (target, via, reference) = site.key;
                let arrow = if single.is_some() {
                    String::new()
                } else {
                    format!("  ← {target}")
                };
                let line = format!(
                    "{prefix}  {}{arrow}{}{}\n",
                    snippet::line_list(&site.lines),
                    if reference { snippet::REF_MARK } else { "" },
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
