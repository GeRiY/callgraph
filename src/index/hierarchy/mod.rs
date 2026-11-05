//! Típus-hierarchia (extends / implements / trait-use) a hatókör gráfjaiból, és az erre épülő
//! „interfészen át” illesztés: egy `Típus::metódus` keresés a rokon típusokon át hívott metódust is megtalálja.
//!
//! A nevek összevetése nyelvfüggetlen: a `\` és a `.` elválasztó `::`-ra normalizálva, kisbetűvel, a generikus
//! paraméterek (`From<String>`) nélkül; két név azonos, ha az egyik a másik szegmens-határra eső végződése
//! (`std::fmt::Display` ~ `fmt::Display`, `App\Contracts\Resolver` ~ `Resolver`).

use std::collections::{HashMap, HashSet};

use regex::Regex;

use crate::model::FileGraph;

/// Egy típus (osztály, interfész, trait, struct…) a hatókörben; az azonos nevűek (több `impl`-blokk) összevonva.
#[derive(Debug, Default)]
struct TypeNode {
    /// Normalizált, kisbetűs teljes név (`app::services::playlistresolver`).
    key: String,
    /// A keresési minta próbájához: az eredeti alakok (`App\Services\PlaylistResolver`, `PlaylistResolver`).
    forms: Vec<String>,
    /// A szülők (extends + implements + traits) normalizált, kisbetűs neve.
    parents: Vec<String>,
    /// A szülők rövid, eredeti alakú neve (a `via` jelöléshez), a `parents` sorrendjében.
    parent_labels: Vec<String>,
    /// A típusban definiált metódusok, kisbetűvel.
    methods: HashSet<String>,
}

/// A hatókör típusai és a köztük lévő öröklési / megvalósítási kapcsolatok.
#[derive(Debug, Default)]
pub(crate) struct TypeHierarchy {
    nodes: Vec<TypeNode>,
    /// Rövid (utolsó szegmens), kisbetűs név → a csomópontok indexei.
    by_short: HashMap<String, Vec<usize>>,
    /// Csomópont → a közvetlen leszármazottai (akiknek valamelyik szülője ez a típus).
    children: Vec<Vec<usize>>,
}

/// Normalizált, kisbetűs típusnév: `\App\X` / `a.b.X` / `Vec<T>` → `app::x` / `a::b::x` / `vec`.
pub(crate) fn normalize(name: &str) -> String {
    let name = name.split('<').next().unwrap_or(name).trim();
    name.trim_start_matches('\\')
        .replace(['\\', '.'], "::")
        .trim_start_matches("::")
        .to_ascii_lowercase()
}

/// Az utolsó szegmens (`app::x` → `x`).
fn short(normalized: &str) -> &str {
    normalized.rsplit("::").next().unwrap_or(normalized)
}

/// Az eredeti alak rövid neve (`App\Contracts\Resolver` → `Resolver`).
fn short_label(name: &str) -> String {
    let name = name.split('<').next().unwrap_or(name).trim();
    name.rsplit(['\\', '.', ':'])
        .next()
        .unwrap_or(name)
        .to_owned()
}

/// Két normalizált név ugyanazt a típust jelölheti-e (egyenlő, vagy az egyik a másik szegmens-végződése).
pub(crate) fn same_type(a: &str, b: &str) -> bool {
    if a.is_empty() || b.is_empty() {
        return false;
    }
    let ends = |long: &str, tail: &str| {
        long.len() > tail.len() + 2
            && long.ends_with(tail)
            && long[..long.len() - tail.len()].ends_with("::")
    };
    a == b || ends(a, b) || ends(b, a)
}

/// Egy hívás-cél szétbontása típusra és metódusra az utolsó `::` mentén (`App\X::run` → (`app::x`, `run`)).
pub(crate) fn split_target(target: &str) -> Option<(String, String)> {
    let (ty, method) = target.rsplit_once("::")?;
    let method = method.trim_end_matches("()");
    if ty.is_empty() || method.is_empty() {
        return None;
    }
    Some((normalize(ty), method.to_ascii_lowercase()))
}

impl TypeHierarchy {
    pub(crate) fn from_graphs<'a>(graphs: impl IntoIterator<Item = &'a FileGraph>) -> Self {
        let mut hierarchy = Self::default();
        let mut index: HashMap<String, usize> = HashMap::new();
        for graph in graphs {
            let namespace = graph.namespace.as_deref().unwrap_or("");
            for container in &graph.containers {
                let full = if namespace.is_empty() {
                    container.name.clone()
                } else {
                    format!("{namespace}\\{}", container.name)
                };
                let key = normalize(&full);
                if key.is_empty() {
                    continue;
                }
                let at = *index.entry(key.clone()).or_insert_with(|| {
                    hierarchy.nodes.push(TypeNode {
                        key: key.clone(),
                        ..TypeNode::default()
                    });
                    hierarchy.nodes.len() - 1
                });
                let node = &mut hierarchy.nodes[at];
                for form in [full, container.name.clone()] {
                    if !node.forms.contains(&form) {
                        node.forms.push(form);
                    }
                }
                let parents = [&container.extends, &container.implements, &container.traits];
                for parent in parents.into_iter().flatten().flatten() {
                    let normalized = normalize(parent);
                    if !normalized.is_empty()
                        && normalized != key
                        && !node.parents.contains(&normalized)
                    {
                        node.parents.push(normalized);
                        node.parent_labels.push(short_label(parent));
                    }
                }
                node.methods.extend(
                    graph
                        .methods
                        .iter()
                        .filter(|m| m.container.as_deref() == Some(container.name.as_str()))
                        .map(|m| m.name.to_ascii_lowercase()),
                );
            }
        }
        for (at, node) in hierarchy.nodes.iter().enumerate() {
            hierarchy
                .by_short
                .entry(short(&node.key).to_owned())
                .or_default()
                .push(at);
        }
        let mut children = vec![Vec::new(); hierarchy.nodes.len()];
        for (child, node) in hierarchy.nodes.iter().enumerate() {
            for parent in &node.parents {
                for at in hierarchy.find(parent) {
                    if at != child && !children[at].contains(&child) {
                        children[at].push(child);
                    }
                }
            }
        }
        hierarchy.children = children;
        hierarchy
    }

    /// A `name` nevű (normalizált) típus csomópontjai.
    fn find(&self, name: &str) -> Vec<usize> {
        self.by_short
            .get(short(name))
            .into_iter()
            .flatten()
            .copied()
            .filter(|&at| same_type(&self.nodes[at].key, name))
            .collect()
    }

    /// A típus összes (tranzitív) őse: (normalizált név, rövid címke); a hatókörön kívüli ős is (pl. vendor-interfész).
    pub(crate) fn ancestors(&self, name: &str) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = Vec::new();
        let mut seen: HashSet<String> = HashSet::from([name.to_owned()]);
        let mut stack = vec![name.to_owned()];
        while let Some(current) = stack.pop() {
            for at in self.find(&current) {
                let node = &self.nodes[at];
                for (parent, label) in node.parents.iter().zip(&node.parent_labels) {
                    if seen.insert(parent.clone()) {
                        out.push((parent.clone(), label.clone()));
                        stack.push(parent.clone());
                    }
                }
            }
        }
        out
    }

    /// A típus (tranzitív) leszármazottainak csomópontjai.
    fn descendants(&self, name: &str) -> Vec<usize> {
        let mut seen: HashSet<usize> = self.find(name).into_iter().collect();
        let mut stack: Vec<usize> = seen.iter().copied().collect();
        let mut out = Vec::new();
        while let Some(at) = stack.pop() {
            for &child in &self.children[at] {
                if seen.insert(child) {
                    out.push(child);
                    stack.push(child);
                }
            }
        }
        out
    }

    /// A `key` típus `method` metódusának rokon típusai (normalizált név, rövid címke): az ősök (interfész,
    /// absztrakt / szülő-osztály, trait — a hatókörön kívüliek is), és a leszármazottak, amelyek a metódust nem
    /// definiálják felül (rajtuk át hívva ugyanez a metódus fut). A testvér-implementációk nem rokonok.
    pub(crate) fn related(&self, key: &str, method: &str) -> Vec<(String, String)> {
        let method = method.to_ascii_lowercase();
        let mut out: Vec<(String, String)> = Vec::new();
        let mut push = |name: String, label: String| {
            if !same_type(&name, key) && !out.iter().any(|(k, _)| *k == name) {
                out.push((name, label));
            }
        };
        for (ancestor, label) in self.ancestors(key) {
            push(ancestor, label);
        }
        for child in self.descendants(key) {
            let child = &self.nodes[child];
            if !child.methods.contains(&method) {
                let label = child
                    .forms
                    .last()
                    .map(String::as_str)
                    .map(short_label)
                    .unwrap_or_default();
                push(child.key.clone(), label);
            }
        }
        out
    }

    /// A típus metódusai, a hatókörbeli ősökben definiáltakkal (örökölt metódusok) együtt.
    fn methods_with_inherited(&self, at: usize) -> HashSet<String> {
        let mut methods = self.nodes[at].methods.clone();
        for (ancestor, _) in self.ancestors(&self.nodes[at].key) {
            for found in self.find(&ancestor) {
                methods.extend(self.nodes[found].methods.iter().cloned());
            }
        }
        methods
    }
}

/// A „via” illesztés: metódusnév → a rokon típusok (normalizált név, rövid címke), amelyeken át hívva a keresett
/// metódus is célpont. Csak a mintára még nem illeszkedő hívásokra kérdezzük.
#[derive(Debug, Clone, Default)]
pub(crate) struct ViaMatcher {
    related: HashMap<String, Vec<(String, String)>>,
}

impl ViaMatcher {
    /// A minta által megnevezett `Típus::metódus` definíciókból (a minta a `Típus::metódus` / `Ns\Típus::metódus`
    /// alakra illeszkedik) a rokon típusok: az ősök (interfész, absztrakt szülő, trait) és azok a leszármazottak,
    /// amelyek a metódust nem definiálják felül. `None`, ha a minta nem típus-minősített (nincs benne `::`) —
    /// ilyenkor a puszta metódusnév úgyis minden fogadóra illeszkedik.
    pub(crate) fn new(hierarchy: &TypeHierarchy, pattern: &str, regex: &Regex) -> Option<Self> {
        if !pattern.contains("::") {
            return None;
        }
        let mut matcher = Self::default();
        for (at, node) in hierarchy.nodes.iter().enumerate() {
            for method in hierarchy.methods_with_inherited(at) {
                let named = node
                    .forms
                    .iter()
                    .any(|form| regex.is_match(&format!("{form}::{method}")));
                if !named {
                    continue;
                }
                let entry = matcher.related.entry(method.clone()).or_default();
                for (key, label) in hierarchy.related(&node.key, &method) {
                    if !entry.iter().any(|(k, _)| *k == key) {
                        entry.push((key, label));
                    }
                }
            }
        }
        matcher.related.retain(|_, types| !types.is_empty());
        (!matcher.related.is_empty()).then_some(matcher)
    }

    /// Ha a hívás-cél (`resolved` / `target`) egy rokon típuson át hívja a keresett metódust: a típus rövid neve.
    pub(crate) fn via(&self, target: &str) -> Option<&str> {
        let (ty, method) = split_target(target)?;
        self.related
            .get(&method)?
            .iter()
            .find(|(key, _)| same_type(&ty, key))
            .map(|(_, label)| label.as_str())
    }
}

#[cfg(test)]
mod tests;
