//! A rétegek közti hívások iránya (C jel): a fájlok feloldott hívásaiból (`resolved`) melyik réteg melyiket hívja.
//! A cél fájlja: PHP / Rust / Swift-nél a típus teljes neve (névtér + konténer), JS / TS / Vue-nál a modul útja
//! (`src/stores/user.ts::useUserStore`). A várt irány a szerepek rangja szerint lefelé (vagy azonos rangon) halad.

use std::collections::HashMap;

use crate::index::normalize_type;
use crate::model::FileGraph;

use super::role::Role;

/// Egy fájl a számításhoz: relatív út, szerep és a (szűkített) gráfja.
pub(crate) struct FlowFile<'a> {
    pub rel: &'a str,
    pub role: Option<Role>,
    pub graph: &'a FileGraph,
}

/// A rétegek közti hívások összesítése.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FlowStats {
    /// A hívásokban részt vevő fő rétegek, a várt irány sorrendjében.
    pub chain: Vec<Role>,
    /// A várt irányba menő (rétegek közti) hívások száma.
    pub expected: usize,
    /// Az összes rétegek közti hívás.
    pub total: usize,
    /// (hívó réteg, hívott réteg) → darab, csökkenő sorrendben.
    pub pairs: Vec<((Role, Role), usize)>,
}

impl FlowStats {
    /// A várt irányba menő hívások aránya százalékban.
    pub(crate) fn percent(&self) -> usize {
        (self.expected * 100).checked_div(self.total).unwrap_or(0)
    }
}

/// A rétegek közti hívások iránya; `None`, ha egyetlen ilyen hívás sincs feloldva.
pub(crate) fn compute(files: &[FlowFile<'_>]) -> Option<FlowStats> {
    let mut types: HashMap<String, usize> = HashMap::new();
    let mut modules: HashMap<&str, usize> = HashMap::new();
    for (at, file) in files.iter().enumerate() {
        modules.entry(file.rel).or_insert(at);
        if let Some(module) = file.graph.module.as_deref() {
            modules.entry(module).or_insert(at);
        }
        let namespace = file.graph.namespace.as_deref().unwrap_or("");
        for container in &file.graph.containers {
            let full = if namespace.is_empty() {
                container.name.clone()
            } else {
                format!("{namespace}\\{}", container.name)
            };
            types.entry(normalize_type(&full)).or_insert(at);
        }
    }
    let mut counts: HashMap<(Role, Role), usize> = HashMap::new();
    for (at, file) in files.iter().enumerate() {
        let Some(from) = file.role.filter(|r| r.in_flow()) else {
            continue;
        };
        for call in &file.graph.calls {
            let Some(target) = call.resolved.as_deref() else {
                continue;
            };
            let found = match target.rsplit_once("::") {
                Some((ty, _)) if ty.contains('/') => modules.get(ty).copied(),
                Some((ty, _)) => types.get(&normalize_type(ty)).copied(),
                None => modules.get(target).copied(),
            };
            let Some(to) = found.filter(|to| *to != at) else {
                continue;
            };
            let Some(into) = files[to].role.filter(|r| r.in_flow() && *r != from) else {
                continue;
            };
            *counts.entry((from, into)).or_default() += 1;
        }
    }
    let total: usize = counts.values().sum();
    if total == 0 {
        return None;
    }
    let expected = counts
        .iter()
        .filter(|((from, into), _)| from.rank() <= into.rank())
        .map(|(_, n)| n)
        .sum();
    let mut involvement: HashMap<Role, usize> = HashMap::new();
    for ((from, into), n) in &counts {
        *involvement.entry(*from).or_default() += n;
        *involvement.entry(*into).or_default() += n;
    }
    // a lánc csak a fő rétegeket mutatja: legalább a hívások 5%-ában részt vesz
    let floor = (total / 20).max(1);
    let mut chain: Vec<Role> = involvement
        .into_iter()
        .filter(|(_, n)| *n >= floor)
        .map(|(role, _)| role)
        .collect();
    chain.sort_by_key(|role| (role.rank(), *role));
    let mut pairs: Vec<((Role, Role), usize)> = counts.into_iter().collect();
    pairs.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    Some(FlowStats {
        chain,
        expected,
        total,
        pairs,
    })
}
