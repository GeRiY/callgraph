//! Az outline procedurális ága (nincs class / interface / trait): a callgraph jq-szűrőjének mása —
//! kategóriánkénti célok, a felső szintű elágazások, végül a hívások soronként.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use super::selection::pad;
use crate::i18n::Lang;
use crate::model::FileGraph;

pub(super) fn procedural(g: &FileGraph, s: &mut String, lang: Lang) {
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
    let mut groups: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
    for c in &g.calls {
        groups.entry(&c.category).or_default().insert(target(c));
    }
    for (category, targets) in &groups {
        let list: Vec<&str> = targets.iter().map(String::as_str).collect();
        writeln!(s, "  {}{}", pad(category, 18), list.join(", ")).unwrap();
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
        writeln!(s, "    L{line}  {}", list.join(", ")).unwrap();
    }
}
