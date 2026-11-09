//! A rétegek közti hívások iránya (C jel): a fájlok feloldott hívásaiból (`resolved`) melyik réteg melyiket hívja.
//! A cél fájlja: PHP / Rust / Swift-nél a típus teljes neve (névtér + konténer), JS / TS / Vue-nál a modul útja
//! (`src/stores/user.ts::useUserStore`). A várt irány a szerepek rangja szerint lefelé (vagy azonos rangon) halad.
//! A réteg szerep + ökoszisztéma: a PHP `service` és a JS `services` mappa két külön csomópont, így a backend és a
//! frontend lánca nem fut össze egy közös szerepnév miatt.

use std::cmp::Reverse;
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
    /// A kiírt láncok: a fő rétegek a várt irány sorrendjében, minden szomszédos pár egy megfigyelt hívás-pár
    /// (fő út, utána a mellékágak; lásd `chains`).
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
            let Some(into) = files[to].layer.filter(|l| l.role.in_flow() && *l != from) else {
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
    let mut main: Vec<Layer> = involvement
        .into_iter()
        .filter(|(_, n)| *n >= floor)
        .map(|(layer, _)| layer)
        .collect();
    main.sort_by_key(|layer| (layer.role.rank(), *layer));
    // egy nyíl csak megfigyelt párra: legalább a hívások 2%-a (min. 1) megy a két réteg között ebben az irányban
    let chain = chains(&main, &counts, (total / 50).max(1));
    let mut pairs: Vec<((Layer, Layer), usize)> = counts.into_iter().collect();
    pairs.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    Some(FlowStats {
        chain,
        expected,
        total,
        pairs,
    })
}

/// Egy út pontszáma: (még nem szereplő rétegek, hossz, hívások száma).
type Score = (usize, usize, usize);

/// A kiírt láncok: minden `A → B` nyíl egy megfigyelt pár (`counts` ≥ `edge_floor`, a rétegek sorrendjében előre).
/// Először a leghosszabb út (a legtöbb, még nem szereplő réteggel; egyenlőségnél a több hívással), utána a még
/// hiányzó rétegek mellékágai (pl. `command → service`) a még nem használt párokból, amíg van új réteg; pár nélküli
/// réteg nem kerül a láncba. Sorrend: összefüggő csoportonként (a csoport első rétegének helye szerint), azon belül a
/// fő út, utána a mellékágak — így a backend és a frontend lánca külön marad.
fn chains(
    main: &[Layer],
    counts: &HashMap<(Layer, Layer), usize>,
    edge_floor: usize,
) -> Vec<Vec<Layer>> {
    let weight = |i: usize, j: usize| {
        counts
            .get(&(main[i], main[j]))
            .copied()
            .filter(|n| *n >= edge_floor)
    };
    let mut used: Vec<(usize, usize)> = Vec::new();
    let mut covered = vec![false; main.len()];
    let mut paths: Vec<Vec<usize>> = Vec::new();
    loop {
        // best[j]: a j-ben végződő legjobb út pontszáma (új rétegek, hossz, hívások) és az előző réteg
        let mut best: Vec<(Score, Option<usize>)> = Vec::with_capacity(main.len());
        for (j, new) in covered.iter().map(|c| usize::from(!c)).enumerate() {
            let mut top = ((new, 1, 0), None);
            for (i, (score, _)) in best.iter().enumerate() {
                let Some(n) = weight(i, j).filter(|_| !used.contains(&(i, j))) else {
                    continue;
                };
                let candidate = (score.0 + new, score.1 + 1, score.2 + n);
                if candidate > top.0 {
                    top = (candidate, Some(i));
                }
            }
            best.push(top);
        }
        let Some(end) = best
            .iter()
            .enumerate()
            .filter(|(_, (score, previous))| score.0 > 0 && previous.is_some())
            .max_by_key(|(j, (score, _))| (*score, Reverse(*j)))
            .map(|(j, _)| j)
        else {
            break;
        };
        let mut path = vec![end];
        while let Some(previous) = best[*path.last().unwrap_or(&end)].1 {
            path.push(previous);
        }
        path.reverse();
        for pair in path.windows(2) {
            used.push((pair[0], pair[1]));
        }
        for at in &path {
            covered[*at] = true;
        }
        paths.push(path);
    }
    // összefüggő csoportok a használt párokból: csoportonként, a csoport első rétegének helye szerint
    let group = |path: &[usize]| {
        let mut members: Vec<usize> = path.to_vec();
        let mut grew = true;
        while grew {
            grew = false;
            for (a, b) in &used {
                let touches = members.contains(a) || members.contains(b);
                for x in [a, b] {
                    if touches && !members.contains(x) {
                        members.push(*x);
                        grew = true;
                    }
                }
            }
        }
        members.into_iter().min().unwrap_or(0)
    };
    let mut keyed: Vec<(usize, Vec<usize>)> = paths.into_iter().map(|p| (group(&p), p)).collect();
    keyed.sort_by_key(|(group, _)| *group);
    keyed
        .into_iter()
        .map(|(_, path)| path.into_iter().map(|at| main[at]).collect())
        .collect()
}
