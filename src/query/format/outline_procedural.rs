//! Az outline procedurális ága (nincs class / interface / trait): a callgraph jq-szűrőjének mása —
//! kategóriánkénti célok, a felső szintű elágazások, végül a hívások soronként.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use super::selection::pad;
use crate::i18n::Lang;
use crate::query::QueryResult;
use crate::query::snippet::{self, SourceLines};

/// `source`: a `--grep` nézetben a sor + a forrássor (`L12  f │ f($x);`) a „hívások sorrendben” listában.
pub(super) fn procedural(
    row: &QueryResult,
    s: &mut String,
    lang: Lang,
    source: Option<&SourceLines>,
) {
    let g = &row.graph;
    let calls = source.map(|lines| (row, lines));
    if g.calls.is_empty() {
        s.push_str(lang.pick(
            "(no class/interface/trait, and no calls either)\n",
            "(nincs class/interface/trait, és nincs hívás sem)\n",
        ));
        return;
    }
    s.push_str(lang.pick(
        "(procedural file — no class/interface/trait)\n\n",
        "(procedurális fájl — nincs class/interface/trait)\n\n",
    ));
    let target = |c: &crate::model::Call| c.resolved.clone().unwrap_or_else(|| c.target.clone());
    // a csomaghoz rendelt külső hívások külön sorban, a csomag címkéjével (`guzzle: …`; csomagtérkép nélkül nincs)
    let mut groups: BTreeMap<(&str, Option<String>), BTreeSet<String>> = BTreeMap::new();
    for c in &g.calls {
        // Inertia render-hívásnál (`Route::inertia`) a page-fájllal: `→ Pages/About.vue`
        groups
            .entry((c.category.as_str(), super::outline::package_label(row, c)))
            .or_default()
            .insert(super::outline::shown_target(row, c, lang));
    }
    let mut ordered: Vec<_> = groups.iter().collect();
    ordered.sort_by_key(|((category, _), _)| crate::model::category_rank(category));
    for ((category, label), targets) in ordered {
        let list: Vec<&str> = targets.iter().map(String::as_str).collect();
        let label = label
            .as_deref()
            .map(|label| format!("{label}: "))
            .unwrap_or_default();
        writeln!(s, "  {}{label}{}", pad(category, 18), list.join(", ")).unwrap();
    }
    s.push('\n');
    let mut flows: Vec<_> = g
        .control_flow
        .iter()
        .filter(|cf| cf.caller.as_deref() == Some("__top__"))
        .collect();
    flows.sort_by_key(|cf| cf.line);
    if !flows.is_empty() {
        s.push_str(lang.pick("  conditionals:\n", "  elágazások:\n"));
        for cf in flows {
            let branches: Vec<String> = cf
                .branches
                .iter()
                .map(|b| format!("L{}-{}", b.start_line, b.end_line))
                .collect();
            writeln!(
                s,
                "    L{}  {} ({})  {} {}",
                cf.line,
                cf.kind,
                cf.cond.as_deref().unwrap_or(""),
                lang.pick("branches:", "ágak:"),
                branches.join(", ")
            )
            .unwrap();
        }
    }
    s.push_str(lang.pick("  calls in order:\n", "  hívások sorrendben:\n"));
    // jq: unique_by([.line, .t]) | group_by(.line) — soronként a célok rendezve, ismétlés nélkül
    let mut by_line: BTreeMap<u32, BTreeSet<String>> = BTreeMap::new();
    for c in &g.calls {
        by_line.entry(c.line).or_default().insert(target(c));
    }
    for (line, targets) in by_line {
        let list: Vec<&str> = targets.iter().map(String::as_str).collect();
        let Some((row, source)) = calls else {
            writeln!(s, "    L{line}  {}", list.join(", ")).unwrap();
            continue;
        };
        let via = g
            .calls
            .iter()
            .filter(|c| c.line == line)
            .find_map(|c| row.via_of(c));
        let code = source.get(&g.file, line);
        let marks = snippet::call_line(lang, line, via, code.as_deref());
        // `L12 via X │ kód` → `L12  célok via X │ kód` (a célok a sor után, mint a sima nézetben)
        let rest = marks.strip_prefix(&format!("L{line}")).unwrap_or("");
        writeln!(s, "    L{line}  {}{rest}", list.join(", ")).unwrap();
    }
}
