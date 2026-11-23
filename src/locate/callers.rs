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
    /// A hívó fájl nyelvcsaládja (`family`): csak azonos családú definíciót hívhat.
    family: String,
}

/// A fájl nyelvcsaládja a kiterjesztésből: a JS / TS / Vue / HTML együtt, a `kt` / `kts` együtt, a PHP külön, más a saját kiterjesztése.
fn family(file: &str) -> String {
    let ext = file
        .rsplit_once('.')
        .map_or("", |(_, ext)| ext)
        .to_lowercase();
    match ext.as_str() {
        "js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx" | "mts" | "cts" | "vue" | "html" | "htm" => {
            "js".to_owned()
        }
        "kt" | "kts" => "kt".to_owned(),
        _ => ext,
    }
}

/// A `defined_in` kulcsa: nyelvcsalád + név, hogy egy Vue-komponens azonos nevű metódusa ne tegye
/// találgatássá a PHP-hívást.
fn defined_key(family: &str, name: &str) -> String {
    format!("{family}\u{1}{name}")
}

/// A projekt hívásai a hívott név (kisbetűvel) szerint.
pub(crate) struct CallerCounts {
    by_name: HashMap<String, Vec<Site>>,
    /// A projektben definiált típusok rövid neve kisbetűvel: ilyen fogadójú hívás csak a saját típusához számít.
    types: HashSet<String>,
    /// Nyelvcsalád + metódus- / függvénynév (kisbetűvel, `defined_key`) → a definiáló típusok rövid neve: ha egy
    /// névnek több gazdája van, az
    /// ismeretlen fogadójú hívás csak találgatás.
    defined_in: HashMap<String, HashSet<String>>,
}

/// Egy definíció hívói: a biztos hívó metódusok száma és az első kettő neve, valamint a csak névegyezés alapján
/// sejtett hívók száma (`guessed`: ismeretlen fogadó, és más típus is definiálja ugyanezt a nevet).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Callers {
    pub count: usize,
    pub sample: Vec<String>,
    pub guessed: usize,
}

impl CallerCounts {
    /// A fájlok gráfjaiból (a bejárás sorrendjében — a minta-nevek is ebben a sorrendben jönnek).
    pub(crate) fn build<'a>(graphs: impl IntoIterator<Item = &'a FileGraph>) -> Self {
        let mut by_name: HashMap<String, Vec<Site>> = HashMap::new();
        let mut types = HashSet::new();
        let mut defined_in: HashMap<String, HashSet<String>> = HashMap::new();
        for graph in graphs {
            for method in &graph.methods {
                let Some(container) = &method.container else {
                    continue;
                };
                let name = method.name.to_lowercase();
                let (inner, bare) = split_member(&name);
                let owner = inner.map_or_else(
                    || short_name(&container.to_lowercase()).to_owned(),
                    |o| short_name(o).to_owned(),
                );
                defined_in
                    .entry(defined_key(&family(&graph.file), bare))
                    .or_default()
                    .insert(owner);
            }
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
                // `$this->x()` / `self::x()` / `static::x()`: a fogadó a hívás saját típusa (a `parent::` ismeretlen marad)
                let own_type = || {
                    let on = call.on.as_deref().map(str::to_lowercase);
                    let this = matches!(
                        on.as_deref().map(|o| o.trim_start_matches('$')),
                        Some("this" | "self" | "static")
                    );
                    call.container
                        .as_deref()
                        .filter(|_| this)
                        .map(|c| short_name(&c.to_lowercase()).to_owned())
                };
                let owner = match owner {
                    Some(o) if matches!(short_name(o), "this" | "self" | "static") => own_type(),
                    Some(o) => Some(short_name(o).to_owned()),
                    None => own_type(),
                };
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
                    owner,
                    caller,
                    family: family(&graph.file),
                });
            }
        }
        Self {
            by_name,
            types,
            defined_in,
        }
    }

    /// A `container` típus `name` metódusának (vagy a `name` függvénynek) hívói. A JS objektum-literál metódusa
    /// (`api.refund`) a pont előtti részt tekinti típusnak.
    /// Csak azonos nyelvcsaládú hívó számít: a Vue `setLocale()`-ját nem hívja a PHP `setLocale`-hívás.
    pub(crate) fn of(&self, container: Option<&str>, name: &str, file: &str) -> Callers {
        let own_family = family(file);
        let name = name.to_lowercase();
        let (inner_owner, bare) = split_member(&name);
        let owner = inner_owner
            .map(|o| short_name(o).to_owned())
            .or_else(|| container.map(|c| short_name(&c.to_lowercase()).to_owned()));
        let Some(sites) = self.by_name.get(bare) else {
            return Callers::default();
        };
        let others = self
            .defined_in
            .get(&defined_key(&own_family, bare))
            .is_some_and(|hosts| {
                hosts
                    .iter()
                    .any(|host| owner.as_ref().is_none_or(|own| host != own))
            });
        let mut sure: Vec<&str> = Vec::new();
        let mut guessed: Vec<&str> = Vec::new();
        for site in sites.iter().filter(|site| site.family == own_family) {
            let caller = site.caller.as_str();
            let is_sure = match (&site.owner, &owner) {
                (_, None) => true,
                (Some(called), Some(own)) if called == own => true,
                (Some(called), Some(_)) if self.types.contains(called) => continue,
                // ismeretlen fogadó: csak akkor biztos, ha más típus nem definiálja ugyanezt a nevet
                _ => !others,
            };
            let list = if is_sure { &mut sure } else { &mut guessed };
            if !list.contains(&caller) {
                list.push(caller);
            }
        }
        guessed.retain(|caller| !sure.contains(caller));
        Callers {
            count: sure.len(),
            sample: sure.iter().take(2).map(|s| (*s).to_owned()).collect(),
            guessed: guessed.len(),
        }
    }
}
