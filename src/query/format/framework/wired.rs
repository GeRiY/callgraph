//! A Laravel-nézetek, amelyek a legacy sorok mellé a gráf tényeiből épített, fájlokon átívelő sorokat is adnak (R7 / F3):
//! `events` (kiváltó ↔ figyelő, observer ↔ modell), `jobs` (a sorba állítható osztályok és kiváltóik) és `middleware`
//! (a route-tábla middleware-oszlopa). A sorokat a `finish` függvény egészíti ki; a legacy gyűjtő sorai változatlanok.

use std::collections::BTreeSet;

use super::{Collect, selected};
use crate::index::{LinkKind, PhpWiring};
use crate::model::FileGraph;
use crate::query::QueryResult;
use crate::query::format::Formatter;

pub(super) type Finish = fn(&[QueryResult], &mut Vec<String>);

pub(super) struct WiredView {
    /// Az üres kimenet szövege: (angol, magyar).
    pub empty: (&'static str, &'static str),
    pub collect: Collect,
    pub finish: Finish,
}

impl Formatter for WiredView {
    fn format(&self, rows: &[QueryResult], lang: crate::i18n::Lang) -> String {
        let mut lines = Vec::new();
        for row in rows {
            let graph = selected(row);
            let ns = graph
                .namespace
                .as_ref()
                .map(|n| format!("{n}\\"))
                .unwrap_or_default();
            (self.collect)(&graph, &ns, &mut lines);
        }
        (self.finish)(rows, &mut lines);
        let text = lines.join("\n");
        let text = text.trim_end_matches('\n');
        if text.is_empty() {
            format!("{}\n", lang.pick(self.empty.0, self.empty.1))
        } else {
            format!("{text}\n")
        }
    }
}

/// A sorok (fájl, gráf) párjai a drótozás építéséhez.
fn records(rows: &[QueryResult]) -> Vec<(&str, &FileGraph)> {
    rows.iter()
        .map(|row| (row.graph.file.as_str(), &row.graph))
        .collect()
}

/// A név utolsó `\`-szegmense.
fn short(name: &str) -> &str {
    name.rsplit('\\').next().unwrap_or(name)
}

/// `Teljes\Osztály::metódus` → `Osztály::metódus`.
fn short_target(target: &str) -> String {
    match target.rsplit_once("::") {
        Some((class, method)) => format!("{}::{method}", short(class)),
        None => short(target).to_owned(),
    }
}

/// A hívási hely: `Konténer::hívó` (konténer nélkül a hívó).
fn site(container: Option<&str>, caller: &str) -> String {
    match container {
        Some(container) => format!("{container}::{caller}"),
        None => caller.to_owned(),
    }
}

/// `events`: az event kiváltója ↔ figyelője, a figyelő kiváltó nélkül, és az observer ↔ modell.
pub(super) fn events_finish(rows: &[QueryResult], lines: &mut Vec<String>) {
    let wiring = PhpWiring::build(&records(rows));
    for listener in wiring.listeners() {
        let target = format!("{}::{}", listener.class, listener.method);
        let event = short(&listener.event);
        let sites: Vec<_> = wiring
            .links()
            .iter()
            .filter(|link| {
                link.kind == LinkKind::Event
                    && link.subject == listener.event
                    && link.target == target
            })
            .collect();
        if sites.is_empty() {
            lines.push(format!(
                "listener  {event}  →  {}  ({})",
                short_target(&target),
                listener.via
            ));
        }
        for link in sites {
            lines.push(format!(
                "wired     {event}  dispatch: {} L{}  →  listener: {}  ({})",
                site(link.container.as_deref(), &link.caller),
                link.line,
                short_target(&target),
                listener.via
            ));
        }
    }
    for observer in wiring.observers() {
        lines.push(format!(
            "observer  {}  →  {}  ({})",
            short(&observer.class),
            short(&observer.model),
            observer.via
        ));
    }
}

/// `jobs`: az event-kiváltás (amelynek van figyelője) nem job; a sorba állítható osztályok, a job / notification
/// kiváltói és a hívott metódusuk.
pub(super) fn jobs_finish(rows: &[QueryResult], lines: &mut Vec<String>) {
    let wiring = PhpWiring::build(&records(rows));
    let events: BTreeSet<&str> = wiring
        .listeners()
        .iter()
        .map(|listener| short(&listener.event))
        .collect();
    lines.retain(|line| {
        let Some(rest) = line.strip_prefix("dispatch  ") else {
            return true;
        };
        let target = rest.split("  @  ").next().unwrap_or(rest);
        target
            .split_once("::")
            .is_none_or(|(class, _)| !events.contains(short(class)))
    });
    for row in rows {
        let graph = &row.graph;
        let ns = graph
            .namespace
            .as_deref()
            .filter(|ns| !ns.is_empty())
            .map(|ns| format!("{ns}\\"))
            .unwrap_or_default();
        for container in &graph.containers {
            let queued = container
                .implements
                .iter()
                .flatten()
                .any(|name| short(name) == "ShouldQueue");
            if queued {
                lines.push(format!(
                    "queued    {ns}{}  implements ShouldQueue  L{}",
                    container.name, container.line
                ));
            }
        }
    }
    for link in wiring.links() {
        if matches!(link.kind, LinkKind::Job | LinkKind::Notification) {
            lines.push(format!(
                "wired     {}  from: {} L{}  →  {}",
                short(&link.subject),
                site(link.container.as_deref(), &link.caller),
                link.line,
                short_target(&link.target)
            ));
        }
    }
}

/// `middleware`: a route-tábla middleware-oszlopa (middleware-enként a route-ok száma és az első néhány); route-tábla
/// nélkül (nem Laravel) nincs további sor.
pub(super) fn middleware_finish(rows: &[QueryResult], lines: &mut Vec<String>) {
    let Some(laravel) = rows
        .iter()
        .find_map(|row| row.laravel.as_ref())
        .filter(|laravel| laravel.has_routes())
    else {
        return;
    };
    let (files, classes) = scope(rows, laravel);
    lines.extend(laravel.middleware_lines(&files, &classes));
}

/// A lekérdezett sorok hatóköre a route-táblához: a fájlok (gyökérhez relatív utak) és az osztályok (teljes nevek).
pub(super) fn scope(
    rows: &[QueryResult],
    laravel: &crate::packages::Laravel,
) -> (Vec<String>, Vec<String>) {
    let files = rows
        .iter()
        .filter_map(|row| laravel.rel(&row.graph.file))
        .collect();
    let classes = rows
        .iter()
        .flat_map(|row| {
            let namespace = row.graph.namespace.as_deref().unwrap_or("");
            row.graph.containers.iter().map(move |container| {
                if namespace.is_empty() {
                    container.name.clone()
                } else {
                    format!("{namespace}\\{}", container.name)
                }
            })
        })
        .collect();
    (files, classes)
}
