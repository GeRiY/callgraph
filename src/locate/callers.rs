//! Hány helyről hívnak egy definíciót — a rangsor enyhe előnyéhez és a `callers: N` jelzéshez. A szűkített tények
//! hívásaiból, név szerint épül (gyors, a teljes hívó-index nélkül): a hívás neve egyezik, és a fogadó típusa vagy
//! ismeretlen (változó, `$this`, `?`), vagy éppen a definíció típusa. Közelítő szám, nem a `find_callers` helyett.

use std::collections::{HashMap, HashSet};

use super::terms::{short_name, split_member};
use crate::model::FileGraph;

/// Egy hívás helye: a fogadó rövid neve (kisbetűvel; `None` = sima függvényhívás) és a hívó (`Osztály::metódus`).
struct Site {
    owner: Option<String>,
    caller: String,
}

/// A projekt hívásai a hívott név (kisbetűvel) szerint.
pub(crate) struct CallerCounts {
    by_name: HashMap<String, Vec<Site>>,
    /// A projektben definiált típusok rövid neve kisbetűvel: ilyen fogadójú hívás csak a saját típusához számít.
    types: HashSet<String>,
}

/// Egy definíció hívói: a különböző hívó metódusok száma és az első kettő neve.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Callers {
    pub count: usize,
    pub sample: Vec<String>,
}

impl CallerCounts {
    /// A fájlok gráfjaiból (a bejárás sorrendjében — a minta-nevek is ebben a sorrendben jönnek).
    pub(crate) fn build<'a>(graphs: impl IntoIterator<Item = &'a FileGraph>) -> Self {
        let mut by_name: HashMap<String, Vec<Site>> = HashMap::new();
        let mut types = HashSet::new();
        for graph in graphs {
            types.extend(
                graph
                    .containers
                    .iter()
                    .map(|c| short_name(&c.name.to_lowercase()).to_owned()),
            );
            for call in graph.calls.iter().filter(|call| !call.is_dynamic()) {
                let target = call
                    .resolved
                    .as_deref()
                    .unwrap_or(&call.target)
                    .to_lowercase();
                let (owner, name) = split_member(&target);
                if name.is_empty() {
                    continue;
                }
                let caller = match (&call.container, &call.caller) {
                    (Some(container), Some(caller)) => format!("{container}::{caller}"),
                    (None, Some(caller)) => caller.clone(),
                    (Some(container), None) => container.clone(),
                    (None, None) => graph
                        .file
                        .rsplit(['/', '\\'])
                        .next()
                        .unwrap_or("")
                        .to_owned(),
                };
                by_name.entry(name.to_owned()).or_default().push(Site {
                    owner: owner.map(|o| short_name(o).to_owned()),
                    caller,
                });
            }
        }
        Self { by_name, types }
    }

    /// A `container` típus `name` metódusának (vagy a `name` függvénynek) hívói. A JS objektum-literál metódusa
    /// (`api.refund`) a pont előtti részt tekinti típusnak.
    pub(crate) fn of(&self, container: Option<&str>, name: &str) -> Callers {
        let name = name.to_lowercase();
        let (inner_owner, bare) = split_member(&name);
        let owner = inner_owner
            .map(|o| short_name(o).to_owned())
            .or_else(|| container.map(|c| short_name(&c.to_lowercase()).to_owned()));
        let Some(sites) = self.by_name.get(bare) else {
            return Callers::default();
        };
        let mut seen: Vec<&str> = Vec::new();
        for site in sites {
            let fits = match (&site.owner, &owner) {
                (None, _) | (_, None) => true,
                (Some(called), Some(own)) => called == own || !self.types.contains(called),
            };
            if fits && !seen.contains(&site.caller.as_str()) {
                seen.push(&site.caller);
            }
        }
        Callers {
            count: seen.len(),
            sample: seen.iter().take(2).map(|s| (*s).to_owned()).collect(),
        }
    }
}
