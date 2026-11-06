//! C jel: megerősítés a már kész gráfból — az öröklés / interfész / trait (közvetlen és a hatókör
//! típus-hierarchiáján át tranzitív ősök), és néhány árulkodó hívás (Vue / Pinia `defineStore`). Csak megerősít
//! (a bizonyosság magasra nő); szerepet nem ad és nem vesz el, így a `--layer` szűrő (ami csak az utat nézi) és a
//! profil ugyanazt a besorolást látja.

use std::collections::HashSet;

use crate::index::TypeHierarchy;
use crate::model::FileGraph;

use super::role::Role;

/// Egy megerősítő szabály: a szerep, az ősök rövid nevei (kisbetűvel; `*x` = `x`-re végződő), az árulkodó hívások
/// rövid nevei, és a profilban kiírt címke.
pub(crate) struct ConfirmRule {
    pub role: &'static str,
    pub ancestors: &'static [&'static str],
    pub calls: &'static [&'static str],
    pub label: &'static str,
}

const fn rule(
    role: &'static str,
    ancestors: &'static [&'static str],
    calls: &'static [&'static str],
    label: &'static str,
) -> ConfirmRule {
    ConfirmRule {
        role,
        ancestors,
        calls,
        label,
    }
}

/// A megerősítő szabályok (adat).
pub(crate) static CONFIRM_RULES: &[ConfirmRule] = &[
    rule("controller", &["*controller"], &[], "extends *Controller"),
    rule(
        "repository",
        &["*repositoryinterface", "*repositorycontract", "*repository"],
        &[],
        "implements *RepositoryInterface",
    ),
    rule(
        "model",
        &[
            "model",
            "authenticatable",
            "pivot",
            "activerecord",
            "baseactiverecord",
        ],
        &[],
        "extends Model / ActiveRecord",
    ),
    rule("request", &["formrequest"], &[], "extends FormRequest"),
    rule("job", &["shouldqueue"], &[], "implements ShouldQueue"),
    rule("command", &["command"], &[], "extends Command"),
    rule(
        "provider",
        &["serviceprovider"],
        &[],
        "extends ServiceProvider",
    ),
    rule(
        "policy",
        &["handlesauthorization"],
        &[],
        "uses HandlesAuthorization",
    ),
    rule(
        "dto",
        &["jsonresource", "resourcecollection"],
        &[],
        "extends JsonResource",
    ),
    rule("factory", &["factory"], &[], "extends Factory"),
    rule("migration", &["migration"], &[], "extends Migration"),
    rule("test", &["*testcase"], &[], "extends TestCase"),
    rule("viewmodel", &["observableobject"], &[], "ObservableObject"),
    rule("view", &["view"], &[], "conforms to View"),
    rule(
        "store",
        &[],
        &["definestore", "createslice", "createstore"],
        "defineStore",
    ),
];

/// A fájl szerepét megerősíti-e a gráfja: a megerősítő szabály címkéje, ha igen.
pub(crate) fn confirm(
    role: Role,
    graph: &FileGraph,
    hierarchy: &TypeHierarchy,
) -> Option<&'static str> {
    let rules: Vec<&ConfirmRule> = CONFIRM_RULES
        .iter()
        .filter(|rule| rule.role == role.name())
        .collect();
    if rules.is_empty() {
        return None;
    }
    let ancestors = ancestor_names(graph, hierarchy);
    let calls: HashSet<String> = graph
        .calls
        .iter()
        .flat_map(|call| [Some(&call.target), call.resolved.as_ref()])
        .flatten()
        .map(|name| short(name))
        .collect();
    rules
        .into_iter()
        .find(|rule| {
            rule.ancestors
                .iter()
                .any(|wanted| ancestors.iter().any(|name| matches(wanted, name)))
                || rule.calls.iter().any(|wanted| calls.contains(*wanted))
        })
        .map(|rule| rule.label)
}

/// `*x`: `x`-re végződik; különben pontos egyezés (kisbetűvel).
fn matches(wanted: &str, name: &str) -> bool {
    match wanted.strip_prefix('*') {
        Some(tail) => name.ends_with(tail),
        None => name == wanted,
    }
}

/// A fájl típusainak ősei rövid, kisbetűs névvel: a közvetlen `extends` / `implements` / trait-use, és a hatókör
/// hierarchiáján át a tranzitívak.
fn ancestor_names(graph: &FileGraph, hierarchy: &TypeHierarchy) -> HashSet<String> {
    let namespace = graph.namespace.as_deref().unwrap_or("");
    let mut out = HashSet::new();
    for container in &graph.containers {
        let direct = [&container.extends, &container.implements, &container.traits];
        for parent in direct.into_iter().flatten().flatten() {
            out.insert(short(parent));
        }
        let full = if namespace.is_empty() {
            container.name.clone()
        } else {
            format!("{namespace}\\{}", container.name)
        };
        let key = crate::index::normalize_type(&full);
        for (_, label) in hierarchy.ancestors(&key) {
            out.insert(label.to_ascii_lowercase());
        }
    }
    out
}

/// Egy típus- vagy hívás-név utolsó szegmense, kisbetűvel, a generikus paraméterek és a `()` nélkül.
pub(crate) fn short(name: &str) -> String {
    let name = name.split('<').next().unwrap_or(name).trim();
    let name = name.trim_end_matches("()");
    name.rsplit(['\\', '.', ':', '/'])
        .next()
        .unwrap_or(name)
        .to_ascii_lowercase()
}
