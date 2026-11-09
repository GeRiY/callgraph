//! A rétegek közti hívások iránya (C jel): a fájlok feloldott hívásaiból (`resolved`) melyik réteg melyiket hívja.
//! A cél fájlja: PHP / Rust / Swift-nél a típus teljes neve (névtér + konténer), JS / TS / Vue-nál a modul útja
//! (`src/stores/user.ts::useUserStore`). A várt irány a szerepek rangja szerint lefelé (vagy azonos rangon) halad.
//! A réteg szerep + ökoszisztéma: a PHP `service` és a JS `services` mappa két külön csomópont, így a backend és a
//! frontend lánca nem fut össze egy közös szerepnév miatt.

use std::collections::HashMap;

use crate::index::normalize_type;
use crate::model::FileGraph;

use super::layer::Layer;

/// Egy fájl a számításhoz: relatív út, réteg és a (szűkített) gráfja.
pub(crate) struct FlowFile<'a> {
    pub rel: &'a str,
    pub layer: Option<Layer>,
    pub graph: &'a FileGraph,
}

/// A rétegek közti hívások összesítése.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FlowStats {
    /// A hívásokban részt vevő fő rétegek, a várt irány sorrendjében.
    pub chain: Vec<Vec<Layer>>,
    /// A várt irányba menő (rétegek közti) hívások száma.
    pub expected: usize,
    /// Az összes rétegek közti hívás.
    pub total: usize,
    /// (hívó réteg, hívott réteg) → darab, csökkenő sorrendben.
    pub pairs: Vec<((Layer, Layer), usize)>,
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
    let mut counts: HashMap<(Layer, Layer), usize> = HashMap::new();
    for (at, file) in files.iter().enumerate() {
        let Some(from) = file.layer.filter(|l| l.role.in_flow()) else {
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
            let Some(into) = files[to]
                .layer
                .filter(|l| l.role.in_flow() && *l != from)
            else {
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
        .filter(|((from, into), _)| from.role.rank() <= into.role.rank())
        .map(|(_, n)| n)
        .sum();
    let mut involvement: HashMap<Layer, usize> = HashMap::new();
    for ((from, into), n) in &counts {
        *involvement.entry(*from).or_default() += n;
        *involvement.entry(*into).or_default() += n;
    }
    // a lánc csak a fő rétegeket mutatja: legalább a hívások 5%-ában részt vesz
    let floor = (total / 20).max(1);
    let mut chain: Vec<Layer> = involvement
        .into_iter()
        .filter(|(_, n)| *n >= floor)
        .map(|(layer, _)| layer)
        .collect();
    chain.sort_by_key(|layer| (layer.role.rank(), *layer));
    let chain = connected_groups(&chain, &counts);
    let mut pairs: Vec<((Layer, Layer), usize)> = counts.into_iter().collect();
    pairs.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    Some(FlowStats {
        chain,
        expected,
        total,
        pairs,
    })
}

/// A fő rétegek csoportjai: azok a rétegek egy csoportban, amelyek között ténylegesen van hívás (pl. a backend és a
/// frontend rétegei külön lánc, ha nem hívják egymást); csoporton belül és a csoportok között a rétegek sorrendjében.
fn connected_groups(chain: &[Layer], counts: &HashMap<(Layer, Layer), usize>) -> Vec<Vec<Layer>> {
    let mut group: Vec<usize> = (0..chain.len()).collect();
    fn root(group: &mut [usize], mut at: usize) -> usize {
        while group[at] != at {
            group[at] = group[group[at]];
            at = group[at];
        }
        at
    }
    for (from, into) in counts.keys() {
        let (Some(a), Some(b)) = (
            chain.iter().position(|r| r == from),
            chain.iter().position(|r| r == into),
        ) else {
            continue;
        };
        let (ra, rb) = (root(&mut group, a), root(&mut group, b));
        if ra != rb {
            group[ra.max(rb)] = ra.min(rb);
        }
    }
    let mut out: Vec<(usize, Vec<Layer>)> = Vec::new();
    for (at, layer) in chain.iter().enumerate() {
        let r = root(&mut group, at);
        match out.iter_mut().find(|(id, _)| *id == r) {
            Some((_, layers)) => layers.push(*layer),
            None => out.push((r, vec![*layer])),
        }
    }
    out.into_iter().map(|(_, layers)| layers).collect()
}
