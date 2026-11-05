//! PHP-framework nézetek (`events`, `routes`, `middleware`, `validation`, `jobs`, `providers`, `magic`, `di`, `views`):
//! fájlonként szekciók, mindegyik egy forrást (hívás / string / metódus) regexszel szűr és fix sorsablont tölt ki.
//! Egy nézet = egy gyűjtőfüggvény a `rules.rs`-ben + egy sor a `register`-ben.

mod routes;
mod rules;

use super::selection::selected;
use super::{Formatter, FormatterRegistry};
use crate::model::{Call, FileGraph, StringLit};
use crate::query::QueryResult;

/// Egyszer fordított regex (a gyűjtők fájlonként futnak).
macro_rules! re {
    ($pattern:literal) => {{
        static RE: std::sync::LazyLock<regex::Regex> =
            std::sync::LazyLock::new(|| regex::Regex::new($pattern).expect("érvényes regex"));
        &*RE
    }};
}
pub(crate) use re;

type Collect = fn(&FileGraph, &str, &mut Vec<String>);

pub(super) struct RuleView {
    empty: &'static str,
    collect: Collect,
}

impl Formatter for RuleView {
    fn format(&self, rows: &[QueryResult]) -> String {
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
        // A callgraph a jq-kimenetet $(…)-ba veszi: a záró sortörések levágódnak, az echo egyet tesz vissza.
        let text = lines.join("\n");
        let text = text.trim_end_matches('\n');
        if text.is_empty() {
            format!("{}\n", self.empty)
        } else {
            format!("{text}\n")
        }
    }
}

pub(super) fn register(registry: &mut FormatterRegistry) {
    let views: [(&'static str, &'static str, Collect); 9] = [
        ("events", "(nincs event/listener találat)", rules::events),
        ("routes", "(nincs route találat)", rules::routes),
        (
            "middleware",
            "(nincs middleware találat)",
            rules::middleware,
        ),
        (
            "validation",
            "(nincs validation találat)",
            rules::validation,
        ),
        ("jobs", "(nincs job/queue találat)", rules::jobs),
        (
            "providers",
            "(nincs service provider találat)",
            rules::providers,
        ),
        ("magic", "(nincs magic method találat)", rules::magic),
        ("di", "(nincs DI container találat)", rules::di),
        ("views", "(nincs view/template találat)", rules::views),
    ];
    for (name, empty, collect) in views {
        registry.register(name, Box::new(RuleView { empty, collect }));
    }
}

/// `<ns><container|__top__>::<caller>` — a hívás helye.
fn call_site(ns: &str, call: &Call) -> String {
    site(ns, call.container.as_deref(), call.caller.as_deref())
}

fn string_site(ns: &str, s: &StringLit) -> String {
    site(ns, s.container.as_deref(), s.caller.as_deref())
}

fn site(ns: &str, container: Option<&str>, caller: Option<&str>) -> String {
    format!(
        "{ns}{}::{}",
        container.unwrap_or("__top__"),
        caller.unwrap_or("")
    )
}

/// A `resolved // target` (a feloldott cél, ha van).
fn resolved_or_target(call: &Call) -> &str {
    call.resolved.as_deref().unwrap_or(&call.target)
}

/// `<címke><szöveg>  @  <hely>  L<sor>` — a legtöbb hívás-szekció sora.
fn at_line(label: &str, text: &str, ns: &str, call: &Call) -> String {
    format!("{label}{text}  @  {}  L{}", call_site(ns, call), call.line)
}

/// `<címke>"<érték>"  <hely>  L<sor>` — a string-szekciók sora.
fn quoted_line(label: &str, ns: &str, s: &StringLit) -> String {
    format!(
        "{label}\"{}\"  {}  L{}",
        s.value,
        string_site(ns, s),
        s.line
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::FileGraphBuilder;

    #[test]
    fn every_view_compiles_its_patterns_and_reports_empty() {
        let row = QueryResult {
            graph: FileGraphBuilder::new("app/views/x.php").build(),
            methods: None,
        };
        let mut registry = FormatterRegistry::default();
        register(&mut registry);
        for name in [
            "events",
            "routes",
            "middleware",
            "validation",
            "jobs",
            "providers",
            "magic",
            "di",
            "views",
        ] {
            let text = registry.render(name, std::slice::from_ref(&row)).unwrap();
            assert!(
                text.starts_with("(nincs ") && text.ends_with("találat)\n"),
                "{name}: {text}"
            );
        }
    }
}
