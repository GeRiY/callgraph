//! Projekt-szintű architektúra a rétegek darabszámából és a felismert frameworkökből (adat-tábla).
//! Új architektúra: egy sor az [`ARCHITECTURES`]-ban.

use std::collections::HashMap;

use super::role::{Confidence, Role};
use super::rules::Eco;

/// Egy felismerhető architektúra.
pub(crate) struct ArchRule {
    pub id: &'static str,
    /// A kiírt név (szakkifejezés, mindkét nyelven így).
    pub label: &'static str,
    /// Ezek valamelyike a frameworkök között (üres = framework nélkül is).
    pub frameworks: &'static [&'static str],
    /// Csak ennek az ökoszisztémának a fájljai számítanak (`None` = mind).
    pub eco: Option<Eco>,
    /// Mind kell (legalább egy fájl).
    pub all: &'static [&'static str],
    /// Ezekből legalább `min_any` kell.
    pub any: &'static [&'static str],
    pub min_any: usize,
    /// Ha ezek közül valamelyik felismerődött, ez (az általánosabb) nem íródik ki.
    pub subsumed_by: &'static [&'static str],
}

const LARAVEL_LAYERS: &[&str] = &[
    "model",
    "service",
    "repository",
    "request",
    "job",
    "middleware",
    "policy",
    "provider",
];
const FRONTEND_LAYERS: &[&str] = &["page", "composable", "store", "api"];
const RUST_WEB_LAYERS: &[&str] = &["handler", "route", "service", "repository", "model"];

/// Az architektúrák, a kiírás sorrendjében (a specifikusak elöl).
pub(crate) static ARCHITECTURES: &[ArchRule] = &[
    ArchRule {
        id: "laravel",
        label: "laravel (layered MVC)",
        frameworks: &["laravel"],
        eco: Some(Eco::Php),
        all: &["controller"],
        any: LARAVEL_LAYERS,
        min_any: 1,
        subsumed_by: &[],
    },
    ArchRule {
        id: "yii",
        label: "yii (MVC)",
        frameworks: &["yii"],
        eco: Some(Eco::Php),
        all: &["controller"],
        any: &["model", "view", "service"],
        min_any: 1,
        subsumed_by: &[],
    },
    ArchRule {
        id: "ddd",
        label: "DDD / Clean / Hexagonal",
        frameworks: &[],
        eco: None,
        all: &["domain"],
        any: &["application", "infrastructure", "presentation", "port", "adapter"],
        min_any: 1,
        subsumed_by: &[],
    },
    ArchRule {
        id: "hexagonal",
        label: "Hexagonal (ports / adapters)",
        frameworks: &[],
        eco: None,
        all: &["port", "adapter"],
        any: &[],
        min_any: 0,
        subsumed_by: &["ddd"],
    },
    ArchRule {
        id: "vue",
        label: "Vue frontend (pages / components / composables / stores)",
        frameworks: &["vue", "nuxt"],
        eco: Some(Eco::Js),
        all: &["component"],
        any: FRONTEND_LAYERS,
        min_any: 1,
        subsumed_by: &[],
    },
    ArchRule {
        id: "react",
        label: "React frontend (pages / components / hooks / stores)",
        frameworks: &["react", "next"],
        eco: Some(Eco::Js),
        all: &["component"],
        any: FRONTEND_LAYERS,
        min_any: 1,
        subsumed_by: &[],
    },
    ArchRule {
        id: "swiftui",
        label: "SwiftUI MVVM (Views → ViewModels)",
        frameworks: &[],
        eco: Some(Eco::Swift),
        all: &["view", "viewmodel"],
        any: &[],
        min_any: 0,
        subsumed_by: &[],
    },
    ArchRule {
        id: "rust-web",
        label: "Rust web (routes / handlers → service → repo)",
        frameworks: &["axum", "actix-web", "rocket", "warp", "poem"],
        eco: Some(Eco::Rust),
        all: &[],
        any: RUST_WEB_LAYERS,
        min_any: 2,
        subsumed_by: &[],
    },
    ArchRule {
        id: "layered",
        label: "layered (controller → service → repository)",
        frameworks: &[],
        eco: None,
        all: &["controller", "service", "repository"],
        any: &[],
        min_any: 0,
        subsumed_by: &["laravel", "yii", "rust-web"],
    },
    ArchRule {
        id: "mvc",
        label: "MVC",
        frameworks: &[],
        eco: None,
        all: &["controller", "model", "view"],
        any: &[],
        min_any: 0,
        subsumed_by: &["laravel", "yii", "layered"],
    },
    ArchRule {
        id: "frontend",
        label: "component-based frontend",
        frameworks: &[],
        eco: Some(Eco::Js),
        all: &["component"],
        any: FRONTEND_LAYERS,
        min_any: 1,
        subsumed_by: &["vue", "react"],
    },
];

/// Egy felismert architektúra a bizonyosságával.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Architecture {
    pub id: &'static str,
    pub label: &'static str,
    pub confidence: Confidence,
}

/// A szerepek fájlszáma ökoszisztémánként.
pub(crate) type RoleCounts = HashMap<(Eco, Role), usize>;

/// A felismert architektúrák (legfeljebb három), bizonyosság szerint csökkenő sorrendben.
pub(crate) fn detect(counts: &RoleCounts, frameworks: &[String]) -> Vec<Architecture> {
    let mut found: Vec<Architecture> = Vec::new();
    for rule in ARCHITECTURES {
        let framework = !rule.frameworks.is_empty()
            && rule
                .frameworks
                .iter()
                .any(|f| frameworks.iter().any(|have| have.as_str() == *f));
        if !rule.frameworks.is_empty() && !framework {
            continue;
        }
        let all = rule.all.iter().all(|name| role_count(counts, rule, name) > 0);
        let any = rule.any.iter().filter(|name| role_count(counts, rule, name) > 0).count();
        let shape = all && any >= rule.min_any;
        let partial = rule
            .all
            .iter()
            .chain(rule.any)
            .any(|name| role_count(counts, rule, name) > 0);
        let confidence = match (framework, shape) {
            (true, true) => Confidence::High,
            (true, false) if partial => Confidence::Medium,
            (true, false) => Confidence::Low,
            // framework nélkül: a mappák alakja; sok fájllal magas
            (false, true) => {
                let files: usize = rule
                    .all
                    .iter()
                    .chain(rule.any)
                    .map(|name| role_count(counts, rule, name))
                    .sum();
                if files >= 10 {
                    Confidence::High
                } else {
                    Confidence::Medium
                }
            }
            (false, false) => continue,
        };
        found.push(Architecture {
            id: rule.id,
            label: rule.label,
            confidence,
        });
    }
    let ids: Vec<&str> = found.iter().map(|a| a.id).collect();
    found.retain(|arch| {
        let rule = ARCHITECTURES.iter().find(|r| r.id == arch.id);
        !rule.is_some_and(|r| r.subsumed_by.iter().any(|by| ids.contains(by)))
    });
    found.sort_by(|a, b| b.confidence.cmp(&a.confidence));
    found.truncate(3);
    found
}

/// A szerep fájljainak száma a szabály ökoszisztémájában.
fn role_count(counts: &RoleCounts, rule: &ArchRule, name: &str) -> usize {
    let Some(role) = Role::named(name) else {
        return 0;
    };
    counts
        .iter()
        .filter(|((eco, r), _)| *r == role && rule.eco.is_none_or(|want| want == *eco))
        .map(|(_, n)| n)
        .sum()
}
