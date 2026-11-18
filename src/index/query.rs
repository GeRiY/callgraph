//! A projekt-indexből készített fordított hívó-lekérdezés.

use std::collections::{HashMap, HashSet};

use regex::RegexBuilder;
use serde::Serialize;

use super::hierarchy::{TypeHierarchy, ViaMatcher, normalize, same_type, split_target};
use super::uncertain::{bare_name, is_uncertain, uncertain_match};
use super::wiring::PhpWiring;
use crate::error::Result;
use crate::model::FileGraph;

/// A `callers` JSONL-kimenetének egy sora.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CallerRow {
    pub file: String,
    pub container: Option<String>,
    pub caller: String,
    pub line: u32,
    pub target: String,
    pub level: usize,
    /// Interfészen / trait-en / szülő- vagy leszármazott-típuson át illesztett hívásnál a hívás típusának rövid neve.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub via: Option<String>,
    #[serde(skip)]
    pub(crate) guessed: bool,
    /// Hivatkozás, nem hívás (`Call::is_reference`): a szöveges kimenet `(ref)`-fel jelöli.
    #[serde(skip)]
    pub(crate) reference: bool,
}

#[derive(Clone)]
struct Edge {
    row: CallerRow,
    lookup: String,
    caller_key: String,
    /// A hívó metódus típusa normalizálva (`app::caller`) — a hierarchián át keresett szülő-hívásokhoz.
    caller_type: Option<String>,
    guessable: bool,
    /// Swift closure-bekötés (`socket.onFrame = { … }`): a bekötés sora és a closure utolsó sora.
    wire: Option<(u32, u32)>,
    /// Az index írta át a célt (Swift fogadó-lánc, Rust `T: A + B`, Kotlin útvonal): a hívás saját `target`-je más.
    rewritten: bool,
}

/// A teljes projektből épített, memóriabeli fordított hívó-index.
pub(crate) struct CallerIndex {
    edges: Vec<Edge>,
    reverse: HashMap<String, Vec<Edge>>,
    /// A `Típus::metódus` alakú (feloldott) hívások metódusnév (kisbetű) szerint: (a cél típusa, az él indexe).
    by_method: HashMap<String, Vec<(String, usize)>>,
    hierarchy: TypeHierarchy,
    /// A bekötött closure hívásai (`edge_key`) → a tárolt closure-t meghívó metódusok hívás-élei (`via <property>`):
    /// a closure hívottjának a hívója a bekötő metódus mellett a valódi kiváltó is.
    callbacks: HashMap<String, Vec<Edge>>,
}

impl CallerIndex {
    /// A hívó-index a fájlok gráfjaiból (út, gráf). Az élek a megadott fájl-sorrendben követik egymást — a kimenet
    /// holtversenyei (azonos fájl és sor) ezt a sorrendet követik, ezért a hívó a bejárás sorrendjét adja.
    pub(crate) fn build<'a>(graphs: impl IntoIterator<Item = (&'a str, &'a FileGraph)>) -> Self {
        let records: Vec<(&str, &FileGraph)> = graphs.into_iter().collect();
        let hierarchy = TypeHierarchy::from_graphs(records.iter().map(|(_, graph)| *graph));
        let properties = property_types(&records);
        let rust_traits = rust_trait_methods(&records);
        let kotlin_routes = kotlin_routes(&records);
        // Laravel-drótozás: Eloquent scope-hívás, event → figyelő, observer, job / notification (csak PHP)
        let wiring = PhpWiring::build(&records);
        let mut edges = Vec::new();
        for &(path, graph) in &records {
            let namespace = graph.namespace.as_deref().unwrap_or("");
            for call in &graph.calls {
                let caller = call.caller.clone().unwrap_or_else(|| "__top__".to_string());
                // Swift: a másik fájlban elakadt fogadó-lánc (`Store::session.user`) feloldása a property-típusokon át
                let chained = (call.resolved.is_none() && path.ends_with(".swift"))
                    .then(|| chain_target(&properties, call.on.as_deref()?, &call.target))
                    .flatten()
                    .or_else(|| {
                        // Rust: `T: A + B`, a trait-ek másik fájlban (az `on` = `?A+B`)
                        let uncertain = call.resolved.as_deref().is_some_and(is_uncertain);
                        (uncertain && path.ends_with(".rs"))
                            .then(|| multi_target(&rust_traits, call.on.as_deref()?, &call.target))
                            .flatten()
                    })
                    .or_else(|| {
                        // Kotlin: másik fájlban deklarált `composable(..)` útvonal (az `on` = `route:<arg>`)
                        (call.resolved.is_none() && path.ends_with(".kt"))
                            .then(|| route_target(&kotlin_routes, call.on.as_deref()?))
                            .flatten()
                    })
                    .or_else(|| {
                        // PHP: `Shelf::query()->stocked()` → `Shelf::scopeStocked`
                        path.ends_with(".php")
                            .then(|| wiring.scope_target(path, call))
                            .flatten()
                    });
                // Python: `self.m()` / `cls.m()` a saját osztály `m` metódusa (örökölt is lehet: a hierarchia köti)
                let own_python = (chained.is_none() && call.resolved.is_none())
                    .then(|| python_own_method(path, call))
                    .flatten();
                // Rust: a mezőláncon át hívott (`ig.0.matcher.matched()`) metódus fogadó-típusa ismeretlen; ha a
                // frontend a név egyediségéből a hívó saját típusára oldotta, az nem tény: név szerinti (`?::név`) hívás
                let demoted = chained
                    .is_none()
                    .then(|| rust_field_chain_guess(path, call))
                    .flatten();
                let target = chained
                    .clone()
                    .or_else(|| demoted.clone())
                    .or_else(|| call.resolved.clone())
                    .unwrap_or_else(|| call.target.clone());
                let lookup = if let Some(method) = own_python {
                    symbol_key(namespace, call.container.as_deref(), method)
                } else if chained.is_none()
                    && call.resolved.is_none()
                    && call.category == "internal_helper"
                {
                    symbol_key(namespace, call.container.as_deref(), &call.target)
                } else {
                    target.clone()
                };
                let caller_key = symbol_key(namespace, call.container.as_deref(), &caller);
                let caller_type = call
                    .container
                    .as_deref()
                    .map(|container| normalize(&symbol_key(namespace, None, container)));
                let uncertain = is_uncertain(&target);
                edges.push(Edge {
                    row: CallerRow {
                        file: path.to_owned(),
                        container: call.container.clone(),
                        caller,
                        line: call.line,
                        target,
                        level: 0,
                        via: None,
                        guessed: false,
                        reference: call.is_reference(),
                    },
                    lookup,
                    caller_key,
                    caller_type,
                    guessable: chained.is_none()
                        && ((call.resolved.is_none()
                            && call.category == "external_call"
                            && own_python.is_none())
                            || uncertain),
                    rewritten: chained.is_some(),
                    wire: (call.kind == "wire")
                        .then_some((call.line, call.end_line.unwrap_or(call.line))),
                });
            }
        }
        // a keretrendszer által hívott célok (event → figyelő …): a kiváltó hely hívja őket
        let namespaces: HashMap<&str, &str> = records
            .iter()
            .map(|&(path, graph)| (path, graph.namespace.as_deref().unwrap_or("")))
            .collect();
        for link in wiring.links() {
            let namespace = namespaces.get(link.file.as_str()).copied().unwrap_or("");
            edges.push(Edge {
                row: CallerRow {
                    file: link.file.clone(),
                    container: link.container.clone(),
                    caller: link.caller.clone(),
                    line: link.line,
                    target: link.target.clone(),
                    level: 0,
                    via: None,
                    guessed: false,
                    reference: false,
                },
                lookup: link.target.clone(),
                caller_key: symbol_key(namespace, link.container.as_deref(), &link.caller),
                caller_type: link
                    .container
                    .as_deref()
                    .map(|container| normalize(&symbol_key(namespace, None, container))),
                guessable: false,
                wire: None,
                rewritten: true,
            });
        }
        let mut reverse: HashMap<String, Vec<Edge>> = HashMap::new();
        let mut by_method: HashMap<String, Vec<(String, usize)>> = HashMap::new();
        for (at, edge) in edges.iter().enumerate() {
            if !edge.guessable || edge.row.target.contains('\\') || edge.row.target.contains("::") {
                reverse
                    .entry(edge.lookup.clone())
                    .or_default()
                    .push(edge.clone());
                if let Some((ty, method)) = split_target(&edge.lookup) {
                    by_method.entry(method).or_default().push((ty, at));
                }
            }
        }
        let callbacks = link_callbacks(&edges, &hierarchy);
        Self {
            edges,
            reverse,
            by_method,
            hierarchy,
            callbacks,
        }
    }

    /// Az él hívó metódusának hívói: a pontos kulcsra, és a hívó típusának rokon típusain (interfész, szülő,
    /// trait, felül nem definiáló leszármazott) át hívott azonos nevű metódusra (`via` jelöléssel).
    fn parent_edges(&self, edge: &Edge) -> Vec<Edge> {
        let mut out: Vec<Edge> = self
            .reverse
            .get(&edge.caller_key)
            .cloned()
            .unwrap_or_default();
        if let Some(invokers) = self.callbacks.get(&edge_key(edge)) {
            out.extend(invokers.iter().cloned());
        }
        // Vue-komponens: a következő szint a fájlt (a komponenst) a template-ben használók — a komponens-
        // használat éle a feloldott fájlútra kulcsol, a hívó kulcsa viszont `Komponens::template`;
        // az út minden utótagját megpróbáljuk (abszolút és projekt-relatív alak is illeszkedjen)
        if edge.row.file.ends_with(".vue") {
            // Windowson a fájlút `\`-es, a komponens-használat kulcsa `/`-es: egységesítjük
            let file = edge.row.file.replace('\\', "/");
            let mut rest = file.trim_start_matches("./").trim_start_matches('/');
            loop {
                if let Some(list) = self.reverse.get(rest) {
                    out.extend(list.iter().cloned());
                }
                match rest.split_once('/') {
                    Some((_, tail)) => rest = tail,
                    None => break,
                }
            }
        }
        let Some(caller_type) = edge.caller_type.as_deref() else {
            return out;
        };
        let method = edge.row.caller.to_ascii_lowercase();
        let Some(candidates) = self.by_method.get(&method) else {
            return out;
        };
        for (key, label) in self.hierarchy.related(caller_type, &method) {
            for (ty, at) in candidates {
                let parent = &self.edges[*at];
                if same_type(ty, &key) && parent.lookup != edge.caller_key {
                    let mut parent = parent.clone();
                    parent.row.via = Some(label.clone());
                    out.push(parent);
                }
            }
        }
        out
    }

    /// A megadott (1. szintű) hívó-sorok hívásai által hívott, tárolt closure-t meghívó metódusok: a bekötött
    /// closure (`socket.onFrame = { … }`) hívottjának valódi kiváltói, `via Típus::tulajdonság` jelöléssel. A sor
    /// célja a closure-ban hívott metódus. Bekötés (`wire` él) nélkül üres.
    pub(crate) fn wire_invokers(&self, front: &[CallerRow]) -> Vec<CallerRow> {
        if self.callbacks.is_empty() || front.is_empty() {
            return Vec::new();
        }
        let wanted: HashSet<(&str, Option<&str>, &str, u32)> = front
            .iter()
            .map(|row| {
                (
                    row.file.as_str(),
                    row.container.as_deref(),
                    row.caller.as_str(),
                    row.line,
                )
            })
            .collect();
        let mut out: Vec<CallerRow> = Vec::new();
        for edge in &self.edges {
            let key = (
                edge.row.file.as_str(),
                edge.row.container.as_deref(),
                edge.row.caller.as_str(),
                edge.row.line,
            );
            if !wanted.contains(&key) {
                continue;
            }
            for invoker in self.callbacks.get(&edge_key(edge)).into_iter().flatten() {
                let mut row = invoker.row.clone();
                row.level = 1;
                row.target = edge.row.target.clone();
                row.reference = false;
                if !out.iter().any(|seen| {
                    seen.file == row.file
                        && seen.container == row.container
                        && seen.caller == row.caller
                        && seen.line == row.line
                }) {
                    out.push(row);
                }
            }
        }
        out.sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));
        out
    }

    /// A mintára illeszkedő, az index által átírt célú hívások (Swift fogadó-lánc, Rust `T: A + B`, Kotlin útvonal):
    /// a fájl-szintű keresés ezeket nem látja, mert a hívás saját célja más. 1. szintű sorok; hibás minta: üres.
    pub(crate) fn rewritten(&self, pattern: &str) -> Vec<CallerRow> {
        let Ok(matcher) = RegexBuilder::new(&crate::query::filter::namespace_pattern(pattern))
            .case_insensitive(true)
            .build()
        else {
            return Vec::new();
        };
        self.edges
            .iter()
            .filter(|edge| edge.rewritten && matcher.is_match(&edge.row.target))
            .map(|edge| {
                let mut row = edge.row.clone();
                row.level = 1;
                row
            })
            .collect()
    }

    /// A `név` metódusra csak név szerint illeszkedő hívások: feloldatlan, fogadó-típus nélküli (`open`), nem
    /// `?::név` célúak (azok a mintára már név szerint illeszkednek). A saját nevű (rekurzív) hívó kimarad.
    pub(crate) fn name_only_callers(&self, name: &str) -> Vec<CallerRow> {
        self.edges
            .iter()
            .filter(|edge| {
                edge.guessable
                    && !is_uncertain(&edge.row.target)
                    && edge.row.target == edge.lookup
                    && edge.row.target.eq_ignore_ascii_case(name)
                    && !edge.row.caller.eq_ignore_ascii_case(name)
            })
            .map(|edge| edge.row.clone())
            .collect()
    }

    pub fn callers(&self, pattern: &str, depth: usize) -> Result<Vec<CallerRow>> {
        self.callers_with_guess(pattern, depth, false)
    }

    pub fn callers_with_guess(
        &self,
        pattern: &str,
        depth: usize,
        guess: bool,
    ) -> Result<Vec<CallerRow>> {
        let matcher = RegexBuilder::new(&crate::query::filter::namespace_pattern(pattern))
            .case_insensitive(true)
            .build()
            .map_err(|error| crate::query::filter::invalid_pattern(&error, pattern))?;
        let mut result = Vec::new();
        // a saját példányon át hívott (`$this->m()`) él célja a hívó típusán is: `Ns\Osztály::m` (a `lookup`),
        // így a `Típus::metódus` minta — és a leszármazottakon át a `via` — az örökölt hívást is megtalálja
        // a kicsomagolt használat (`usePageChrome().site.x`) nem a composable hívása: lásd `target_matches`
        let is_match = |target: &str| super::uncertain::target_matches(&matcher, target);
        let direct =
            |edge: &Edge| is_match(&edge.row.target) || own_target(edge).is_some_and(is_match);
        // a `?::név` (feloldatlan fogadójú) cél a `Típus::név` mintára név szerint, bizonytalanként illeszkedik
        let mut front: Vec<Edge> = self
            .edges
            .iter()
            .filter(|edge| direct(edge) || uncertain_match(&matcher, &edge.row.target))
            .cloned()
            .map(|mut edge| {
                edge.row.guessed |= is_uncertain(&edge.row.target);
                edge
            })
            .collect();
        if let Some(via) = ViaMatcher::new(&self.hierarchy, pattern, &matcher) {
            front.extend(self.edges.iter().filter_map(|edge| {
                if direct(edge) {
                    return None;
                }
                let label = via
                    .via(&edge.row.target)
                    .or_else(|| own_target(edge).and_then(|o| via.via(o)))?;
                let mut edge = edge.clone();
                edge.row.via = Some(label.to_owned());
                Some(edge)
            }));
        }
        if guess {
            let names: HashSet<_> = self
                .edges
                .iter()
                .map(|edge| edge.lookup.rsplit("::").next().unwrap_or(&edge.lookup))
                .collect();
            front.extend(
                self.edges
                    .iter()
                    .filter(|edge| {
                        edge.row
                            .target
                            .rsplit("::")
                            .next()
                            .is_some_and(|name| names.contains(name))
                    })
                    .cloned(),
            );
        }
        let mut seen = HashSet::new();
        for level in 1..=depth {
            if front.is_empty() {
                break;
            }
            let mut next = Vec::new();
            for edge in front {
                let key = format!("{}:{}:{}", edge.row.file, edge.caller_key, edge.row.line);
                if !seen.insert(key) {
                    continue;
                }
                let mut row = edge.row.clone();
                row.level = level;
                result.push(row);
                next.extend(self.parent_edges(&edge));
            }
            front = next;
        }
        result.sort_by(|a, b| {
            a.level
                .cmp(&b.level)
                .then_with(|| a.file.cmp(&b.file))
                .then_with(|| a.line.cmp(&b.line))
        });
        Ok(result)
    }

    pub fn parents_of_with_skips(
        &self,
        front_rows: &[CallerRow],
        depth: usize,
        guess: bool,
    ) -> (Vec<CallerRow>, Vec<usize>) {
        let mut front: Vec<Edge> = front_rows
            .iter()
            .flat_map(|row| {
                self.edges
                    .iter()
                    .filter(move |edge| {
                        edge.row.file == row.file
                            && edge.row.container == row.container
                            && edge.row.caller == row.caller
                            && edge.row.line == row.line
                    })
                    .cloned()
            })
            .collect();
        let mut result = Vec::new();
        let mut skipped = Vec::new();
        let mut seen = front_rows
            .iter()
            .map(|row| {
                format!(
                    "{}:{}:{}",
                    row.file,
                    row.container.as_deref().unwrap_or("__top__"),
                    row.caller
                )
            })
            .collect::<HashSet<_>>();
        let initial_seen = seen.clone();
        for level in 2..=depth {
            let mut next = Vec::new();
            let mut guessed = Vec::new();
            for edge in front {
                next.extend(self.parent_edges(&edge));
                let name = edge
                    .row
                    .caller
                    .rsplit("::")
                    .next()
                    .unwrap_or(&edge.row.caller);
                guessed.extend(
                    self.edges
                        .iter()
                        .filter(|parent| {
                            parent.guessable
                                && bare_name(&parent.row.target) == name
                                && parent.row.target == parent.lookup
                                && bare_name(&parent.row.target) != parent.row.caller
                        })
                        .cloned(),
                );
            }
            guessed.sort_by(|left, right| {
                left.row
                    .file
                    .cmp(&right.row.file)
                    .then_with(|| left.row.line.cmp(&right.row.line))
            });
            guessed.dedup_by(|left, right| {
                left.row.file == right.row.file
                    && left.row.container == right.row.container
                    && left.row.caller == right.row.caller
            });
            let guessed_keys = guessed
                .iter()
                .map(|edge| {
                    format!(
                        "{}:{}:{}",
                        edge.row.file,
                        edge.row.container.as_deref().unwrap_or("__top__"),
                        edge.row.caller
                    )
                })
                .collect::<HashSet<_>>();
            if guess {
                for edge in guessed {
                    let mut edge = edge;
                    edge.row.guessed = true;
                    next.push(edge);
                }
            } else {
                skipped.push(guessed_keys.len());
                next.retain(|edge| {
                    !edge.guessable
                        || edge.row.target.contains('\\')
                        || edge.row.target.contains("::")
                });
            }
            // Hívónként a legkisebb sorú hívás marad (a régi: fájlon belül hívás-sorrend).
            next.sort_by(|a, b| (&a.row.file, a.row.line).cmp(&(&b.row.file, b.row.line)));
            // A hívó egyszer lép tovább a következő szintre, de minden különböző célja sor marad (egy fájl
            // `__top__` hívójának több útvonal-hivatkozása — `Route::get(.., [A::class, 'x'])` — mind látszik).
            let mut extra: Vec<Edge> = Vec::new();
            let mut row_targets: HashSet<(String, String)> = HashSet::new();
            let mut level_keys: HashSet<String> = HashSet::new();
            front = Vec::new();
            for mut edge in next {
                let key = format!(
                    "{}:{}:{}",
                    edge.row.file,
                    edge.row.container.as_deref().unwrap_or("__top__"),
                    edge.row.caller
                );
                let first =
                    (edge.guessable && !initial_seen.contains(&key)) || seen.insert(key.clone());
                edge.row.guessed = edge.guessable;
                if first {
                    level_keys.insert(key.clone());
                    row_targets.insert((key, target_at(&edge)));
                    front.push(edge);
                } else if !edge.guessable
                    && level_keys.contains(&key)
                    && row_targets.insert((key, target_at(&edge)))
                {
                    extra.push(edge);
                }
            }
            for edge in front.iter().chain(&extra) {
                let mut row = edge.row.clone();
                row.level = level;
                result.push(row);
            }
        }
        (result, skipped)
    }
}

/// A sor azonosítója a hívón belül: a cél; a legfelső szintű (`__top__`) hívónál a hívás sora is (két útvonal, egy akció).
fn target_at(edge: &Edge) -> String {
    if edge.row.caller == "__top__" {
        format!("{}@{}", edge.row.target, edge.row.line)
    } else {
        edge.row.target.clone()
    }
}

/// Az él azonosítója a bekötés-táblához: fájl, hívó metódus, sor.
fn edge_key(edge: &Edge) -> String {
    format!("{}:{}:{}", edge.row.file, edge.caller_key, edge.row.line)
}

/// A closure-bekötések (`wire` élek) és a tárolt closure-t meghívó élek párosítása: ha a bekötött tulajdonság
/// (`RealtimeSocket::onFrame`) azonos vagy rokon (protokoll / implementáció) típuson, azonos nevű tulajdonságot
/// hív egy metódus (`onFrame?(frame)`), az a closure hívásainak (a bekötés sortartományában lévő éleknek) is
/// hívója — `via <tulajdonság>` jelöléssel. A `?::név` (ismeretlen fogadójú) bekötés bármely típusra illik.
fn link_callbacks(edges: &[Edge], hierarchy: &TypeHierarchy) -> HashMap<String, Vec<Edge>> {
    let mut out: HashMap<String, Vec<Edge>> = HashMap::new();
    for wire in edges.iter().filter(|edge| edge.wire.is_some()) {
        let (Some((start, end)), Some((wired, name))) = (wire.wire, split_target(&wire.lookup))
        else {
            continue;
        };
        let related: Vec<String> = hierarchy
            .related(&wired, &name)
            .into_iter()
            .map(|(key, _)| key)
            .collect();
        let property = wire
            .row
            .target
            .rsplit('.')
            .next()
            .unwrap_or(&wire.row.target)
            .to_owned();
        let invokers: Vec<&Edge> = edges
            .iter()
            .filter(|edge| {
                edge.wire.is_none()
                    && !(edge.row.file == wire.row.file
                        && edge.row.container == wire.row.container
                        && edge.row.caller == wire.row.caller)
            })
            .filter(|edge| {
                split_target(&edge.lookup).is_some_and(|(ty, method)| {
                    method == name
                        && (wired == "?"
                            || same_type(&ty, &wired)
                            || related.iter().any(|key| same_type(&ty, key)))
                })
            })
            .collect();
        if invokers.is_empty() {
            continue;
        }
        for callee in edges.iter().filter(|edge| {
            edge.wire.is_none()
                && edge.row.file == wire.row.file
                && edge.row.container == wire.row.container
                && edge.row.caller == wire.row.caller
                && (start..=end).contains(&edge.row.line)
        }) {
            let parents = out.entry(edge_key(callee)).or_default();
            for invoker in &invokers {
                let mut parent = (*invoker).clone();
                parent.row.via = Some(property.clone());
                parents.push(parent);
            }
        }
    }
    out
}

/// Python: a `self.m()` / `cls.m()` hívás `m` metódusa, ha a hívás osztályban van és a fogadó pontosan
/// `self` / `cls` (`self.repo.save()` nem az: a fogadó egy attribútum).
fn python_own_method<'a>(path: &str, call: &'a crate::model::Call) -> Option<&'a str> {
    if !path.ends_with(".py") || call.container.is_none() || call.kind != "method_call" {
        return None;
    }
    let method = call
        .target
        .strip_prefix("self.")
        .or_else(|| call.target.strip_prefix("cls."))?;
    (!method.is_empty() && method.chars().all(|c| c.is_alphanumeric() || c == '_'))
        .then_some(method)
}

/// Rust: a mezőláncú fogadójú (`on` = `ig.0.matcher`) hívás, amelyet a frontend a hívó saját típusára oldott
/// (`Ignore::matched`) — a fogadó valódi típusa ismeretlen, ezért `?::név`.
fn rust_field_chain_guess(path: &str, call: &crate::model::Call) -> Option<String> {
    if !path.ends_with(".rs") {
        return None;
    }
    let resolved = call.resolved.as_deref()?;
    let (owner, name) = resolved.rsplit_once("::")?;
    let container = call.container.as_deref()?;
    let on = call.on.as_deref()?;
    let chain = on.contains('.') && !on.starts_with('?') && !on.starts_with("Self");
    (chain && !is_uncertain(resolved) && owner.rsplit("::").next() == Some(container))
        .then(|| format!("{}{name}", super::uncertain::UNKNOWN_RECEIVER))
}

/// A saját példányon át hívott (`$this->m()`, feloldatlan belső) él célja a hívó típusán: a `lookup`
/// (`Ns\\Osztály::m`), ha eltér a nyers céltól.
fn own_target(edge: &Edge) -> Option<&str> {
    (edge.lookup != edge.row.target).then_some(edge.lookup.as_str())
}

fn symbol_key(namespace: &str, container: Option<&str>, caller: &str) -> String {
    let prefix = if namespace.is_empty() {
        String::new()
    } else {
        format!("{namespace}\\")
    };
    match container {
        Some(container) => format!("{prefix}{container}::{caller}"),
        None => format!("{prefix}{caller}"),
    }
}

/// A projekt property-típusai: (konténer, property) → a típus rövid neve. A típus a deklaráció utolsó
/// szóköz-tagja (`@Published User?` → `User`); tömb / szótár / generikus (`[User]`, `Foo<Bar>`) nem típus-lánc.
fn property_types(records: &[(&str, &FileGraph)]) -> HashMap<(String, String), String> {
    let mut out = HashMap::new();
    for (path, graph) in records {
        if !path.ends_with(".swift") {
            continue;
        }
        for property in &graph.properties {
            let (Some(container), Some(hint)) = (&property.container, &property.type_hint) else {
                continue;
            };
            if let Some(ty) = chain_type(hint) {
                out.entry((container.clone(), property.name.clone()))
                    .or_insert(ty);
            }
        }
    }
    out
}

/// A property deklarált típusa láncolható névként (`?` / `!` nélkül), vagy `None` (gyűjtemény, generikus, üres).
fn chain_type(hint: &str) -> Option<String> {
    let token = hint.split_whitespace().next_back()?;
    let token = token.trim_end_matches(['?', '!']);
    let valid = token
        .chars()
        .next()
        .is_some_and(|c| c.is_alphabetic() || c == '_')
        && token
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '.');
    valid.then(|| token.to_owned())
}

/// A projekt Kotlin navigációs útvonalai: (útvonal vagy típusnév, képernyő) a `composable(..)` nav-élekből.
fn kotlin_routes(records: &[(&str, &FileGraph)]) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for (path, graph) in records {
        if !path.ends_with(".kt") {
            continue;
        }
        for call in graph
            .calls
            .iter()
            .filter(|c| c.kind == "nav" && c.on.is_none())
        {
            if let Some(route) = call.args.as_ref().and_then(|a| a.first()) {
                let pair = (route.clone(), call.target.clone());
                if !out.contains(&pair) {
                    out.push(pair);
                }
            }
        }
    }
    out
}

/// Az útvonal literál-előtagja a `{…}` / `$…` / `?…` előtt, záró `/` nélkül.
fn route_prefix(route: &str) -> &str {
    let end = route.find(['{', '$', '?']).unwrap_or(route.len());
    route[..end].trim_end_matches('/')
}

/// A `route:<arg>` jelölésű `navigate(..)` hívás képernyője: az egyező literál-előtagú, vagy a paraméteres
/// útvonal (`detail/{id}`) alatti útvonal; több találatnál a leghosszabb előtag. Típusbiztos útvonalnál a típusnév.
fn route_target(routes: &[(String, String)], on: &str) -> Option<String> {
    let wanted = route_prefix(on.strip_prefix("route:")?);
    if wanted.is_empty() {
        return None;
    }
    routes
        .iter()
        .filter(|(route, _)| {
            let prefix = route_prefix(route);
            !prefix.is_empty()
                && (prefix == wanted
                    || (route.contains('{') && wanted.starts_with(&format!("{prefix}/"))))
        })
        .max_by_key(|(route, _)| route_prefix(route).len())
        .map(|(_, screen)| screen.clone())
}

/// A projekt Rust trait-jei: trait neve → a deklarált metódusok nevei.
fn rust_trait_methods(records: &[(&str, &FileGraph)]) -> HashMap<String, HashSet<String>> {
    let mut out: HashMap<String, HashSet<String>> = HashMap::new();
    for (path, graph) in records {
        if !path.ends_with(".rs") {
            continue;
        }
        for container in graph.containers.iter().filter(|c| c.kind == "trait") {
            let methods = out.entry(container.name.clone()).or_default();
            methods.extend(
                graph
                    .methods
                    .iter()
                    .filter(|m| m.container.as_deref() == Some(container.name.as_str()))
                    .map(|m| m.name.clone()),
            );
        }
    }
    out
}

/// `T: A + B` hívás (`on` = `?A+B`) tényleges célja: ha a jelöltek közül pontosan egy trait deklarálja a
/// metódust, `Trait::metódus`. Több (vagy nulla) találatnál `None`: a hívás marad bizonytalan (`?::név`) —
/// ez kevesebb zaj, mint minden trait-hez külön, találgatott él.
fn multi_target(
    traits: &HashMap<String, HashSet<String>>,
    on: &str,
    method: &str,
) -> Option<String> {
    let candidates = on.strip_prefix('?')?;
    let mut found = candidates
        .split('+')
        .filter_map(|c| c.rsplit("::").next())
        .filter(|name| traits.get(*name).is_some_and(|m| m.contains(method)));
    let first = found.next()?;
    found.next().is_none().then(|| format!("{first}::{method}"))
}

/// Az elakadt fogadó-lánc (`on` = `Típus::a.b`, `target` = `…a.b.metódus`) tényleges célja: a property-ösvény
/// végigjárása a típusokon (`Típus` → `a` típusa → `b` típusa = `U`), eredménye `U::metódus`. Ismeretlen
/// tagnál `None` (marad a mai viselkedés).
fn chain_target(
    properties: &HashMap<(String, String), String>,
    on: &str,
    target: &str,
) -> Option<String> {
    let (root, path) = on.split_once("::")?;
    let method = target.rsplit('.').next().filter(|name| !name.is_empty())?;
    let mut current = root.to_owned();
    for segment in path.split('.').filter(|segment| !segment.is_empty()) {
        current = properties
            .get(&(current.clone(), segment.to_owned()))?
            .clone();
    }
    Some(format!("{current}::{method}"))
}
