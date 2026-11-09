//! Fájl → szerep (réteg), bizonyosság és indok, csak az útjából: a `.callgraph-layers` saját hozzárendelései, a
//! mappanevek (A jel) és a fájlnév-végződés (B jel) a `rules.rs` táblái szerint. Fájlrendszert nem olvas (a
//! `.callgraph-layers` a `Classifier` létrehozásakor töltődik), így a lekérdezéskor is olcsó.
//!
//! Döntés: a teszt-mappa bárhol az útban → `test`; különben a legmélyebb illeszkedő mappa (a DDD-kontextus —
//! `Domain`, `Application`… — csak ha más mappa nem mond semmit) és a végződés. Ha a kettő egyezik: magas
//! bizonyosság; ha a végződés egy minta (`*Strategy`) egy réteg-mappában, a réteg marad és minta-címkét kap; más
//! ellentmondásnál a végződés nyer, eggyel kisebb bizonyossággal. Ha semmi sem illeszkedik: ismeretlen (nem találgat).

use std::path::Path;

use super::layer::Layer;
use super::layers_file::Overrides;
use super::names::{NameHit, name_hit};
use super::role::{Confidence, Kind, Role};
use super::rules::{Eco, FOLDER_RULES, FolderRule, VUE_FALLBACK};

/// Miből jött a döntés (a `profile --full` indoklásához).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Reason {
    /// Mappanév (az eredeti alakjában).
    Folder(String),
    /// Fájlnév-végződés / -alak (`*Service`, `*.store`, `use*`, Rust-modulnév).
    Name(String),
    /// A mappa és a név ugyanazt mondja.
    Both(String, String),
    /// A mappa mást mond, a név nyert.
    Conflict(String, String),
    /// `.vue` fájl (single-file component).
    Vue,
    /// A `.callgraph-layers` hozzárendelése.
    Override,
    /// A `.callgraph-layers` `ignore-role` sora.
    Ignored,
    /// Semmi sem illeszkedett.
    Unknown,
}

/// Egy fájl besorolása.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Classification {
    pub eco: Eco,
    pub role: Option<Role>,
    pub confidence: Confidence,
    pub reason: Reason,
    /// A tervezési minta (Repository, Factory, Strategy…), ha a szerepből vagy a végződésből következik.
    pub pattern: Option<&'static str>,
    /// A mappa, amelyhez a döntés kötődik (a profil réteg-soraihoz): az illeszkedő mappa útja, különben a fájl mappája.
    pub anchor: String,
    /// A minta példája a profilban: a mappa, vagy `mappa/*Végződés`.
    pub pattern_example: Option<String>,
    /// Kód-jelet kívánó szerepnél (`event`, `listener`, `job`; lásd `role::CODE_SIGNAL_ROLES`) a besorolás e
    /// szerepek nélkül: a profil ezt használja, ha a gráf nem erősíti meg a szerepet (vagy nincs gráf).
    pub fallback: Option<Box<Classification>>,
}

impl Classification {
    /// A réteg (szerep + ökoszisztéma), ha van szerep.
    pub(crate) fn layer(&self) -> Option<Layer> {
        self.role.map(|role| Layer::new(role, self.eco))
    }
}

/// A név-alapú osztályozó; a projekt saját hozzárendeléseivel (`.callgraph-layers`).
#[derive(Debug, Clone, Default)]
pub(crate) struct Classifier {
    overrides: Overrides,
}

/// Egy illeszkedő mappa: szerep, bizonyosság, a mappa útja és a neve.
struct FolderHit {
    role: Role,
    confidence: Confidence,
    anchor: String,
    name: String,
}

impl Classifier {
    pub(crate) fn new(overrides: Overrides) -> Self {
        Self { overrides }
    }

    /// A gyökér `.callgraph-layers` fájljával (ha van; a gyökértől felfelé a legközelebbi).
    pub(crate) fn for_root(root: &Path) -> Self {
        Self::new(Overrides::for_root(root))
    }

    pub(crate) fn overrides(&self) -> &Overrides {
        &self.overrides
    }

    /// Csak a szerep (tesztekhez; a lekérdezések a `LayerScope`-on át, a tartalék-szabállyal).
    #[cfg(test)]
    pub(crate) fn role_of(&self, rel: &str) -> Option<Role> {
        self.classify(rel).and_then(|c| c.role)
    }

    /// Csak a réteg (szerep + ökoszisztéma; tesztekhez).
    #[cfg(test)]
    pub(crate) fn layer_of(&self, rel: &str) -> Option<Layer> {
        self.classify(rel).and_then(|c| c.layer())
    }

    /// A projekt-gyökérhez relatív, `/`-rel elválasztott út besorolása; `None`, ha nem kódfájl (CSS, JSON, MD…).
    /// Kód-jelet kívánó szerepnél a `fallback` is kitöltve (a `.callgraph-layers` hozzárendelésénél nem).
    pub(crate) fn classify(&self, rel: &str) -> Option<Classification> {
        self.classify_with(rel, true)
    }

    /// Mint a `classify`, de a fájlnév (végződés, előtag, Rust-modulnév) nem dönt: egy csak enumot tartalmazó fájl
    /// (`PaymentProvider` enum) neve nem szerep (a profil a gráf alapján hívja).
    pub(crate) fn classify_without_name(&self, rel: &str) -> Option<Classification> {
        self.classify_with(rel, false)
    }

    /// A tartalék-besorolás kitöltése kód-jelet kívánó szerepnél; `names`: számít-e a fájlnév.
    fn classify_with(&self, rel: &str, names: bool) -> Option<Classification> {
        let mut class = self.classify_by(rel, false, names)?;
        if class.reason != Reason::Override && class.role.is_some_and(Role::needs_code_signal) {
            class.fallback = self.classify_by(rel, true, names).map(Box::new);
        }
        Some(class)
    }

    /// A besorolás; `strict`: a kód-jelet kívánó szerepek mappa- és névszabályai nélkül; `names`: a fájlnév-szabályok
    /// is számítanak.
    fn classify_by(&self, rel: &str, strict: bool, names: bool) -> Option<Classification> {
        let (dir, file) = rel.rsplit_once('/').unwrap_or(("", rel));
        let (stem, ext) = file.rsplit_once('.')?;
        let eco = Eco::of_extension(ext)?;
        if let Some(mapping) = self.overrides.lookup(rel) {
            let (confidence, reason) = match mapping.role {
                Some(_) => (Confidence::High, Reason::Override),
                None => (Confidence::High, Reason::Ignored),
            };
            return Some(Classification {
                eco,
                role: mapping.role,
                confidence,
                reason,
                pattern: mapping.role.and_then(|r| r.info().pattern),
                anchor: mapping.prefix.clone(),
                pattern_example: None,
                fallback: None,
            });
        }
        let name = name_hit(stem, eco)
            .filter(|hit| names && !(strict && hit.role.needs_code_signal()));
        // a DDD-kontextus mappa (`Domain/…`) csak akkor dönt, ha a fájlnév semmit sem mond
        let folder = folder_hit(dir, eco, strict)
            .filter(|folder| name.is_none() || folder.role.kind() != Kind::Context);
        let base = Classification {
            eco,
            role: None,
            confidence: Confidence::Low,
            reason: Reason::Unknown,
            pattern: None,
            anchor: dir.to_owned(),
            pattern_example: None,
            fallback: None,
        };
        let example = |hit: &NameHit| hit.suffix.map(|s| suffix_example(dir, s));
        Some(match (folder, name) {
            (None, None) if eco == Eco::Js && ext.eq_ignore_ascii_case("vue") => {
                let (role, confidence) = VUE_FALLBACK;
                Classification {
                    role: Role::named(role),
                    confidence,
                    reason: Reason::Vue,
                    ..base
                }
            }
            (None, None) => base,
            (Some(folder), None) => Classification {
                role: Some(folder.role),
                confidence: folder.confidence,
                reason: Reason::Folder(folder.name),
                pattern: folder.role.info().pattern,
                pattern_example: folder.role.info().pattern.map(|_| folder.anchor.clone()),
                anchor: folder.anchor,
                ..base
            },
            (None, Some(name)) => Classification {
                role: Some(name.role),
                confidence: name.confidence,
                pattern: name.role.info().pattern,
                pattern_example: name.role.info().pattern.and_then(|_| example(&name)),
                reason: Reason::Name(name.shown),
                ..base
            },
            (Some(folder), Some(name)) if folder.role == name.role => Classification {
                role: Some(folder.role),
                confidence: Confidence::High,
                reason: Reason::Both(folder.name, name.shown),
                pattern: folder.role.info().pattern,
                pattern_example: folder.role.info().pattern.map(|_| folder.anchor.clone()),
                anchor: folder.anchor,
                ..base
            },
            // minta-végződés egy réteg-mappában (`Services/StripeStrategy`), vagy teszt- / enum-mappa
            // (`Enums/PaymentProvider`): a mappa dönt
            (Some(folder), Some(name))
                if (name.role.kind() == Kind::Pattern && folder.role.kind() != Kind::Pattern)
                    || matches!(folder.role.name(), "test" | "enum") =>
            {
                let pattern = name.role.info().pattern.or(folder.role.info().pattern);
                Classification {
                    role: Some(folder.role),
                    confidence: folder.confidence,
                    reason: Reason::Folder(folder.name),
                    pattern,
                    pattern_example: pattern.and_then(|_| example(&name)),
                    anchor: folder.anchor,
                    ..base
                }
            }
            (Some(folder), Some(name)) => Classification {
                role: Some(name.role),
                confidence: lower(name.confidence),
                pattern: name.role.info().pattern,
                pattern_example: name.role.info().pattern.and_then(|_| example(&name)),
                reason: Reason::Conflict(folder.name, name.shown),
                ..base
            },
        })
    }
}

/// Eggyel kisebb bizonyosság (ellentmondó jeleknél).
fn lower(confidence: Confidence) -> Confidence {
    match confidence {
        Confidence::High => Confidence::Medium,
        _ => Confidence::Low,
    }
}

fn suffix_example(dir: &str, suffix: &str) -> String {
    if dir.is_empty() {
        format!("*{suffix}")
    } else {
        format!("{dir}/*{suffix}")
    }
}

/// Mappanév összevetéshez: kisbetű, `-`, `_` és szóköz nélkül (`View-Models` → `viewmodels`).
pub(crate) fn normalize(segment: &str) -> String {
    segment
        .chars()
        .filter(|c| !matches!(c, '-' | '_' | ' '))
        .flat_map(char::to_lowercase)
        .collect()
}

fn rule_matches(rule: &FolderRule, name: &str, parent: &str, eco: Eco) -> bool {
    rule.ecos.contains(eco) && rule.names.contains(&name) && rule.parent.is_none_or(|p| p == parent)
}

/// A mappák alapján: teszt-mappa bárhol → `test`; különben a legmélyebb nem-kontextus mappa, végül a legmélyebb
/// kontextus-mappa. `strict`: a kód-jelet kívánó szerepek mappái nem számítanak.
fn folder_hit(dir: &str, eco: Eco, strict: bool) -> Option<FolderHit> {
    if dir.is_empty() {
        return None;
    }
    let segments: Vec<&str> = dir.split('/').collect();
    let normalized: Vec<String> = segments.iter().map(|s| normalize(s)).collect();
    let mut hits: Vec<(usize, Role, Confidence)> = Vec::new();
    for at in (0..segments.len()).rev() {
        let parent = if at > 0 {
            normalized[at - 1].as_str()
        } else {
            ""
        };
        let found = FOLDER_RULES
            .iter()
            .filter(|rule| rule_matches(rule, &normalized[at], parent, eco))
            .find_map(|rule| Role::named(rule.role).map(|role| (role, rule.confidence)))
            .filter(|(role, _)| !(strict && role.needs_code_signal()));
        if let Some((role, confidence)) = found {
            hits.push((at, role, confidence));
        }
    }
    let chosen = hits
        .iter()
        .find(|(_, role, _)| role.name() == "test")
        .or_else(|| {
            hits.iter()
                .find(|(_, role, _)| role.kind() != Kind::Context)
        })
        .or_else(|| hits.first())?;
    let (at, role, confidence) = *chosen;
    Some(FolderHit {
        role,
        confidence,
        anchor: segments[..=at].join("/"),
        name: segments[at].to_owned(),
    })
}
