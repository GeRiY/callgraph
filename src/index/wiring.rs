//! Laravel „drótozás” (F3): a keretrendszer köti össze, amit a kód nem hív közvetlenül — az event kiváltója és a
//! figyelői (`handle(Event $e)` a `Listeners` névtérben, `Event::listen(E::class, L::class)`, az `$listen` tömb),
//! az observer a modelljével (`Model::observe(O::class)`, `#[ObservedBy]`), a job / notification kiváltója és a
//! `handle` / `toMail` metódusa, és az Eloquent local scope (`Shelf::query()->stocked()` → `Shelf::scopeStocked`).
//! A bemenet a szűkített gráf (hívás-argumentum nélkül): a `class_ref` hívások sorrendje és sora adja a párokat.

use std::collections::{BTreeSet, HashMap};

use crate::model::{Call, FileGraph};

/// A Laravel observer-metódusok (a modell eseményei).
const OBSERVER_METHODS: [&str; 12] = [
    "retrieved",
    "creating",
    "created",
    "updating",
    "updated",
    "saving",
    "saved",
    "deleting",
    "deleted",
    "restoring",
    "restored",
    "forceDeleted",
];

/// Mi köti a kiváltó helyet a céljához.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LinkKind {
    /// Event → figyelő (`handle` / `__invoke`).
    Event,
    /// A modell regisztrációja → az observer metódusai.
    Observer,
    /// `Job::dispatch` → `Job::handle`.
    Job,
    /// `->notify(new X)` → `X::toMail` / `via`.
    Notification,
}

/// Egy hívási hely és a keretrendszer által hívott cél.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Link {
    pub file: String,
    pub container: Option<String>,
    pub caller: String,
    pub line: u32,
    /// A cél: `Teljes\Osztály::metódus`.
    pub target: String,
    pub kind: LinkKind,
    /// Az event / job / notification / modell teljes neve (amelyik kiváltja / hordozza).
    pub subject: String,
}

/// Egy event figyelője.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Listener {
    pub event: String,
    pub class: String,
    pub method: String,
    /// Hogyan derül ki: `Event::listen L19` / `$listen L12` / `handle(BookLoaned)`.
    pub via: String,
}

/// Egy observer és a modellje.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Observer {
    pub model: String,
    pub class: String,
    pub via: String,
}

#[derive(Debug, Default)]
struct ClassInfo {
    methods: BTreeSet<String>,
    implements: Vec<String>,
    namespace: String,
}

/// Egy hívás-csoport helye: (fájl, konténer, hívó, sor).
type SiteKey = (String, Option<String>, Option<String>, u32);

/// A projekt PHP-drótozása.
#[derive(Debug, Default)]
pub(crate) struct PhpWiring {
    /// (osztály, scope-név kisbetűvel) → a scope-metódus tényleges neve (`scopeStocked`).
    scopes: HashMap<(String, String), String>,
    /// Egy hívás-csoport (fájl, konténer, hívó, sor) láncgyökér-osztályai, amelyeknek van scope-ja.
    roots: HashMap<SiteKey, Vec<String>>,
    listeners: Vec<Listener>,
    observers: Vec<Observer>,
    links: Vec<Link>,
}

impl PhpWiring {
    /// A fájlok (út, gráf) PHP-drótozása; nem PHP fájl kimarad.
    pub(crate) fn build(records: &[(&str, &FileGraph)]) -> Self {
        let php: Vec<(&str, &FileGraph)> = records
            .iter()
            .copied()
            .filter(|(path, _)| path.ends_with(".php"))
            .collect();
        let mut wiring = Self::default();
        if php.is_empty() {
            return wiring;
        }
        let classes = class_table(&php);
        wiring.collect_scopes(&php, &classes);
        wiring.collect_listeners(&php, &classes);
        wiring.collect_links(&php, &classes);
        wiring
    }

    /// Az Eloquent scope-hívás célja: a lánc (`Shelf::query()->stocked()`) vagy a statikus hívás (`Shelf::stocked()`)
    /// gyökérosztályának `scopeNév` metódusa; nem scope: `None`.
    pub(crate) fn scope_target(&self, path: &str, call: &Call) -> Option<String> {
        if self.scopes.is_empty() || (call.resolved.is_some() && call.kind != "static_call") {
            return None;
        }
        match call.kind.as_str() {
            "method_call" if call.on.as_deref() == Some("<expr>") => {
                let key = (
                    path.to_owned(),
                    call.container.clone(),
                    call.caller.clone(),
                    call.line,
                );
                let name = call.target.to_ascii_lowercase();
                self.roots.get(&key)?.iter().find_map(|class| {
                    let real = self.scopes.get(&(class.clone(), name.clone()))?;
                    Some(format!("{class}::{real}"))
                })
            }
            "static_call" => {
                let (class, method) = static_parts(call)?;
                let real = self
                    .scopes
                    .get(&(class.clone(), method.to_ascii_lowercase()))?;
                (real.as_str() != method.as_str()).then(|| format!("{class}::{real}"))
            }
            _ => None,
        }
    }

    /// Az event-figyelők.
    pub(crate) fn listeners(&self) -> &[Listener] {
        &self.listeners
    }

    /// Az observerek.
    pub(crate) fn observers(&self) -> &[Observer] {
        &self.observers
    }

    /// A hívási helyek és a keretrendszer által hívott céljaik.
    pub(crate) fn links(&self) -> &[Link] {
        &self.links
    }

    fn collect_scopes(&mut self, php: &[(&str, &FileGraph)], classes: &HashMap<String, ClassInfo>) {
        for (class, info) in classes {
            for method in &info.methods {
                let Some(rest) = method.strip_prefix("scope") else {
                    continue;
                };
                if rest.chars().next().is_some_and(char::is_uppercase) {
                    self.scopes
                        .insert((class.clone(), rest.to_ascii_lowercase()), method.clone());
                }
            }
        }
        if self.scopes.is_empty() {
            return;
        }
        let with_scopes: BTreeSet<&str> = self
            .scopes
            .keys()
            .map(|(class, _)| class.as_str())
            .collect();
        let mut roots: HashMap<SiteKey, Vec<String>> = HashMap::new();
        for (path, graph) in php {
            for call in graph.calls.iter().filter(|call| call.kind == "static_call") {
                let Some((class, _)) = static_parts_in(graph, call) else {
                    continue;
                };
                if !with_scopes.contains(class.as_str()) {
                    continue;
                }
                let key = (
                    (*path).to_owned(),
                    call.container.clone(),
                    call.caller.clone(),
                    call.line,
                );
                let list = roots.entry(key).or_default();
                if !list.contains(&class) {
                    list.push(class);
                }
            }
        }
        self.roots = roots;
    }

    fn collect_listeners(
        &mut self,
        php: &[(&str, &FileGraph)],
        classes: &HashMap<String, ClassInfo>,
    ) {
        let handler = |class: &str| -> String {
            let info = classes.get(class);
            ["handle", "__invoke"]
                .into_iter()
                .find(|name| info.is_some_and(|info| info.methods.contains(*name)))
                .unwrap_or("handle")
                .to_owned()
        };
        let mut found: Vec<Listener> = Vec::new();
        for (_, graph) in php {
            let namespace = graph.namespace.as_deref().unwrap_or("");
            // 1. a `Listeners` névtér `handle(Event $e)` / `__invoke(Event $e)` metódusa (auto-discovery)
            if namespace.split('\\').any(|part| part == "Listeners") {
                for method in graph
                    .methods
                    .iter()
                    .filter(|m| m.name == "handle" || m.name == "__invoke")
                {
                    let Some(container) = method.container.as_deref() else {
                        continue;
                    };
                    let Some(event) = method.params.first().and_then(|p| param_type(graph, p))
                    else {
                        continue;
                    };
                    found.push(Listener {
                        event,
                        class: qualify(namespace, container),
                        method: method.name.clone(),
                        via: format!("{}({})", method.name, first_param_short(method)),
                    });
                }
            }
            for call in &graph.calls {
                // 2. `Event::listen(E::class, L::class)`: a két `X::class` hivatkozás ugyanazon a soron
                if call.kind == "static_call" && call.target.ends_with("Event::listen") {
                    let refs = class_refs(graph, call);
                    if let [event, listener, ..] = refs.as_slice() {
                        found.push(Listener {
                            event: event.clone(),
                            class: listener.clone(),
                            method: handler(listener),
                            via: format!("Event::listen L{}", call.line),
                        });
                    }
                }
            }
            // 3. az `$listen` tömb (EventServiceProvider): a kulcs (event) után a figyelői, sorrendben
            if graph.properties.iter().any(|p| p.name == "listen") {
                let mut refs: Vec<&Call> = graph
                    .calls
                    .iter()
                    .filter(|c| c.kind == "class_ref" && c.caller.as_deref() == Some("__top__"))
                    .collect();
                refs.sort_by_key(|c| c.line);
                let mut event: Option<String> = None;
                for reference in refs {
                    let class = reference
                        .resolved
                        .clone()
                        .unwrap_or_else(|| reference.target.clone());
                    if class.split('\\').any(|part| part == "Events") {
                        event = Some(class);
                    } else if let Some(event) = &event {
                        found.push(Listener {
                            event: event.clone(),
                            class: class.clone(),
                            method: handler(&class),
                            via: format!("$listen L{}", reference.line),
                        });
                    }
                }
            }
        }
        // egy (event, figyelő) pár egyszer: a kifejezett regisztráció előbbre való az auto-felismerésnél
        found.sort_by_key(|l| l.via.ends_with(')'));
        let mut seen: BTreeSet<(String, String)> = BTreeSet::new();
        found.retain(|l| seen.insert((l.event.clone(), l.class.clone())));
        self.listeners = found;
    }

    fn collect_links(&mut self, php: &[(&str, &FileGraph)], classes: &HashMap<String, ClassInfo>) {
        for (path, graph) in php {
            // `#[ObservedBy([O::class])]`: a modell osztály-sorában álló hivatkozás
            for container in &graph.containers {
                for call in graph.calls.iter().filter(|c| {
                    c.kind == "class_ref"
                        && c.caller.as_deref() == Some("__top__")
                        && c.container.as_deref() == Some(container.name.as_str())
                        && c.line == container.line
                }) {
                    let observer = call.resolved.clone().unwrap_or_else(|| call.target.clone());
                    let model = qualify(graph.namespace.as_deref().unwrap_or(""), &container.name);
                    self.observe(path, call, &model, &observer, "ObservedBy", classes);
                }
            }
            for call in &graph.calls {
                self.dispatch_links(path, graph, call, classes);
                // `Model::observe(O::class)`
                if call.kind == "static_call" && call.target.ends_with("::observe") {
                    let (Some((model, _)), Some(observer)) = (
                        static_parts_in(graph, call),
                        class_refs(graph, call).first().cloned(),
                    ) else {
                        continue;
                    };
                    self.observe(path, call, &model, &observer, "observe", classes);
                }
            }
        }
    }

    fn observe(
        &mut self,
        path: &str,
        site: &Call,
        model: &str,
        observer: &str,
        how: &str,
        classes: &HashMap<String, ClassInfo>,
    ) {
        let Some(info) = classes.get(observer) else {
            return;
        };
        let mut any = false;
        for method in OBSERVER_METHODS
            .iter()
            .filter(|m| info.methods.contains(**m))
        {
            any = true;
            self.links.push(Link {
                file: path.to_owned(),
                container: site.container.clone(),
                caller: site.caller.clone().unwrap_or_else(|| "__top__".to_owned()),
                line: site.line,
                target: format!("{observer}::{method}"),
                kind: LinkKind::Observer,
                subject: model.to_owned(),
            });
        }
        if any {
            self.observers.push(Observer {
                model: model.to_owned(),
                class: observer.to_owned(),
                via: format!("{how} L{}", site.line),
            });
        }
    }

    /// A kiváltó hívás (`E::dispatch`, `event(new E)`, `Job::dispatch`, `->notify(new N)`) → a keretrendszer által
    /// hívott metódus(ok).
    fn dispatch_links(
        &mut self,
        path: &str,
        graph: &FileGraph,
        call: &Call,
        classes: &HashMap<String, ClassInfo>,
    ) {
        let name = call.target.rsplit("::").next().unwrap_or(&call.target);
        let lower = name.to_ascii_lowercase();
        let subjects: Vec<String> = match call.kind.as_str() {
            "static_call" if is_dispatch(&lower) => match static_parts_in(graph, call) {
                // `Event::dispatch(new E)` / `Bus::dispatch(new J)`: az átadott objektum az alany
                Some((class, _)) if class.ends_with("\\Event") || class.ends_with("\\Bus") => {
                    created(graph, call)
                }
                Some((class, _)) => vec![class],
                None => Vec::new(),
            },
            "func_call" if lower == "event" || lower == "dispatch" || lower == "dispatch_sync" => {
                created(graph, call)
            }
            "method_call" if lower == "notify" || lower == "notifynow" => created(graph, call),
            "static_call" if lower == "send" && call.target.starts_with("Notification::") => {
                created(graph, call)
            }
            _ => Vec::new(),
        };
        let notification = matches!(lower.as_str(), "notify" | "notifynow" | "send");
        for subject in subjects {
            let caller = call.caller.clone().unwrap_or_else(|| "__top__".to_owned());
            let mut push = |target: String, kind: LinkKind| {
                self.links.push(Link {
                    file: path.to_owned(),
                    container: call.container.clone(),
                    caller: caller.clone(),
                    line: call.line,
                    target,
                    kind,
                    subject: subject.clone(),
                });
            };
            if notification {
                if let Some(info) = classes.get(&subject)
                    && let Some(method) = ["toMail", "toArray", "via"]
                        .into_iter()
                        .find(|name| info.methods.contains(*name))
                {
                    push(format!("{subject}::{method}"), LinkKind::Notification);
                }
                continue;
            }
            let events: Vec<&Listener> = self
                .listeners
                .iter()
                .filter(|listener| listener.event == subject)
                .collect();
            if !events.is_empty() {
                let targets: Vec<String> = events
                    .iter()
                    .map(|l| format!("{}::{}", l.class, l.method))
                    .collect();
                for target in targets {
                    push(target, LinkKind::Event);
                }
            } else if let Some(info) = classes.get(&subject)
                && info.methods.contains("handle")
                && (info.namespace.split('\\').any(|part| part == "Jobs")
                    || info
                        .implements
                        .iter()
                        .any(|name| name.contains("ShouldQueue")))
            {
                push(format!("{subject}::handle"), LinkKind::Job);
            }
        }
    }
}

fn is_dispatch(lower: &str) -> bool {
    matches!(
        lower,
        "dispatch" | "dispatchif" | "dispatchunless" | "dispatchsync" | "dispatchafterresponse"
    )
}

/// Az osztály teljes neve (`Ns\Osztály`).
fn qualify(namespace: &str, name: &str) -> String {
    if namespace.is_empty() {
        name.to_owned()
    } else {
        format!("{namespace}\\{name}")
    }
}

/// A fájl osztályai: teljes név → metódusok, `implements`, névtér.
fn class_table(php: &[(&str, &FileGraph)]) -> HashMap<String, ClassInfo> {
    let mut out: HashMap<String, ClassInfo> = HashMap::new();
    for (_, graph) in php {
        let namespace = graph.namespace.as_deref().unwrap_or("");
        for container in &graph.containers {
            let info = out.entry(qualify(namespace, &container.name)).or_default();
            namespace.clone_into(&mut info.namespace);
            info.implements.extend(
                container
                    .implements
                    .iter()
                    .flatten()
                    .chain(container.extends.iter().flatten())
                    .cloned(),
            );
            info.methods.extend(
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

/// A statikus hívás osztálya (teljes név) és metódusa, a fájl `use`-aival.
fn static_parts_in(graph: &FileGraph, call: &Call) -> Option<(String, String)> {
    if let Some(resolved) = call.resolved.as_deref() {
        let (class, method) = resolved.rsplit_once("::")?;
        return Some((class.trim_start_matches('\\').to_owned(), method.to_owned()));
    }
    let (class, method) = call.target.rsplit_once("::")?;
    Some((resolve(graph, class), method.to_owned()))
}

/// Mint [`static_parts_in`], import-térkép nélkül (a hívás feloldott célja kell).
fn static_parts(call: &Call) -> Option<(String, String)> {
    let (class, method) = call.resolved.as_deref()?.rsplit_once("::")?;
    Some((class.trim_start_matches('\\').to_owned(), method.to_owned()))
}

/// A név teljes alakja a fájl névterében és `use`-aival.
fn resolve(graph: &FileGraph, raw: &str) -> String {
    let raw = raw.trim().trim_start_matches('?');
    if let Some(full) = raw.strip_prefix('\\') {
        return full.to_owned();
    }
    let (head, rest) = match raw.split_once('\\') {
        Some((head, rest)) => (head, Some(rest)),
        None => (raw, None),
    };
    match (graph.imports.get(head), rest) {
        (Some(full), Some(rest)) => format!("{full}\\{rest}"),
        (Some(full), None) => full.to_owned(),
        (None, _) => qualify(graph.namespace.as_deref().unwrap_or(""), raw),
    }
}

/// A metódus-paraméter osztály-típusa (teljes név); skalár / unió / típus nélküli: `None`.
fn param_type(graph: &FileGraph, param: &str) -> Option<String> {
    let head = param.split_whitespace().next()?;
    if head.starts_with('$') || head.contains(['|', '&']) {
        return None;
    }
    let name = head.trim_start_matches('?');
    name.chars()
        .next()
        .is_some_and(|c| c.is_uppercase() || c == '\\')
        .then(|| resolve(graph, name))
}

/// A metódus első paraméterének rövid típusa (a `via` szövegéhez).
fn first_param_short(method: &crate::model::Method) -> String {
    method
        .params
        .first()
        .and_then(|p| p.split_whitespace().next())
        .map(|ty| ty.rsplit('\\').next().unwrap_or(ty).to_owned())
        .unwrap_or_default()
}

/// A hívással azonos sorú (azonos konténer + hívó) `X::class` hivatkozások teljes nevei, sorrendben.
fn class_refs(graph: &FileGraph, call: &Call) -> Vec<String> {
    graph
        .calls
        .iter()
        .filter(|c| {
            c.kind == "class_ref"
                && c.line == call.line
                && c.container == call.container
                && c.caller == call.caller
        })
        .map(|c| c.resolved.clone().unwrap_or_else(|| c.target.clone()))
        .collect()
}

/// A hívással azonos sorú (azonos konténer + hívó) `new X(..)` objektumok teljes osztálynevei.
fn created(graph: &FileGraph, call: &Call) -> Vec<String> {
    graph
        .calls
        .iter()
        .filter(|c| {
            c.kind == "new"
                && c.line == call.line
                && c.container == call.container
                && c.caller == call.caller
        })
        .map(|c| {
            c.resolved
                .clone()
                .unwrap_or_else(|| resolve(graph, &c.target))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Call, FileGraphBuilder};

    fn graph(file: &str, namespace: &str) -> FileGraph {
        let mut graph = FileGraphBuilder::new(file).build();
        graph.namespace = Some(namespace.to_owned());
        graph
    }

    fn method(container: &str, name: &str, params: &[&str]) -> crate::model::Method {
        crate::model::Method {
            container: Some(container.to_owned()),
            name: name.to_owned(),
            line: 1,
            end_line: None,
            visibility: None,
            is_static: false,
            is_abstract: false,
            kind: None,
            params: params.iter().map(|p| (*p).to_owned()).collect(),
            return_type: None,
            doc_types: None,
            doc: None,
            body: None,
        }
    }

    fn container(name: &str, line: u32) -> crate::model::Container {
        crate::model::Container {
            kind: "class".to_owned(),
            name: name.to_owned(),
            line,
            end_line: line + 5,
            is_abstract: None,
            is_final: None,
            extends: None,
            implements: None,
            traits: None,
            doc: None,
        }
    }

    #[test]
    fn scope_chain_resolves_to_the_scope_method() {
        let mut model = graph("app/Models/Shelf.php", "App\\Models");
        model.containers.push(container("Shelf", 3));
        model.methods.push(method("Shelf", "scopeStocked", &[]));
        let mut ctl = graph("app/Http/ShelfController.php", "App\\Http");
        let at = |call: Call| call.at(15).within(Some("C".into()), Some("index".into()));
        ctl.calls
            .push(at(Call::new("static_call", "Shelf::query", "model_use")
                .resolved(Some("App\\Models\\Shelf::query".to_owned()))));
        let mut chained = Call::new("method_call", "stocked", "external_call");
        chained.on = Some("<expr>".to_owned());
        ctl.calls.push(at(chained.clone()));
        let wiring = PhpWiring::build(&[
            ("app/Models/Shelf.php", &model),
            ("app/Http/ShelfController.php", &ctl),
        ]);
        assert_eq!(
            wiring.scope_target("app/Http/ShelfController.php", &at(chained)),
            Some("App\\Models\\Shelf::scopeStocked".to_owned())
        );
    }

    #[test]
    fn listener_is_paired_by_the_handle_type_hint() {
        let mut listener = graph("app/Listeners/Send.php", "App\\Listeners");
        listener.containers.push(container("Send", 3));
        listener
            .methods
            .push(method("Send", "handle", &["BookLoaned $event"]));
        listener.imports.insert(
            "BookLoaned".to_owned(),
            "App\\Events\\BookLoaned".to_owned(),
        );
        let mut desk = graph("app/Services/Desk.php", "App\\Services");
        desk.calls.push(
            Call::new("static_call", "BookLoaned::dispatch", "static_call")
                .resolved(Some("App\\Events\\BookLoaned::dispatch".to_owned()))
                .at(15)
                .within(Some("Desk".into()), Some("checkout".into())),
        );
        let wiring = PhpWiring::build(&[
            ("app/Listeners/Send.php", &listener),
            ("app/Services/Desk.php", &desk),
        ]);
        assert_eq!(wiring.listeners().len(), 1);
        let link = &wiring.links()[0];
        assert_eq!(link.target, "App\\Listeners\\Send::handle");
        assert_eq!((link.kind, link.line), (LinkKind::Event, 15));
    }
}
