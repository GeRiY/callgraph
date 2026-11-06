//! A projekt-profil összesítése a fájlok besorolásából (fájlrendszer nélkül, tesztelhető): architektúra, rétegek
//! mappánként és darabszámmal, minták példákkal, ismeretlen mappák, és (ha van gráf) a hívásirány.

use std::collections::HashMap;
use std::path::PathBuf;

use super::architecture::{self, Architecture, RoleCounts};
use super::classify::Classification;
use super::flow::FlowStats;
use super::role::{Confidence, Role};

/// Egy fájl a profilhoz: a relatív útja, a besorolása, és a gráfból jött megerősítés címkéje (C jel).
#[derive(Debug, Clone)]
pub(crate) struct FileEntry {
    pub rel: String,
    pub class: Classification,
    pub confirmed: Option<&'static str>,
}

impl FileEntry {
    /// A végső bizonyosság: a gráf megerősítése magasra emeli.
    pub(crate) fn confidence(&self) -> Confidence {
        if self.confirmed.is_some() {
            Confidence::High
        } else {
            self.class.confidence
        }
    }
}

/// Egy réteg összesítése.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LayerSummary {
    pub role: Role,
    pub files: usize,
    /// (mappa, fájlszám), csökkenő sorrendben.
    pub folders: Vec<(String, usize)>,
    /// A gráfból megerősített fájlok száma és a leggyakoribb megerősítés címkéje.
    pub confirmed: usize,
    pub confirm_label: Option<&'static str>,
    /// Fájlszám bizonyosság szerint: magas, közepes, alacsony.
    pub by_confidence: [usize; 3],
}

/// Egy felismert minta: név, fájlszám, példák (mappa vagy `mappa/*Végződés`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PatternSummary {
    pub name: &'static str,
    pub files: usize,
    pub examples: Vec<String>,
}

/// A projekt-profil.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectProfile {
    pub architectures: Vec<Architecture>,
    pub frameworks: Vec<String>,
    pub layers: Vec<LayerSummary>,
    pub patterns: Vec<PatternSummary>,
    /// Az ismeretlen szerepű fájlok mappái (mappa, fájlszám), csökkenő sorrendben.
    pub unknown: Vec<(String, usize)>,
    /// Kódfájlok száma (PHP, JS / TS / Vue, Swift, Rust).
    pub files: usize,
    pub flow: Option<FlowStats>,
    /// A C jel (gráf) lefutott-e.
    pub graph_checked: bool,
    /// A `.callgraph-layers` helye és a fel nem ismert sorai.
    pub overrides: Option<PathBuf>,
    pub warnings: Vec<String>,
}

impl ProjectProfile {
    /// Az összesítés a fájlok besorolásából; `frameworks`: a manifestekből és a gráfból felismertek.
    pub(crate) fn aggregate(
        entries: &[FileEntry],
        frameworks: Vec<String>,
        flow: Option<FlowStats>,
        graph_checked: bool,
    ) -> Self {
        let mut counts: RoleCounts = HashMap::new();
        let mut layers: HashMap<Role, LayerSummary> = HashMap::new();
        let mut labels: HashMap<Role, HashMap<&'static str, usize>> = HashMap::new();
        let mut folders: HashMap<Role, HashMap<&str, usize>> = HashMap::new();
        let mut unknown: HashMap<&str, usize> = HashMap::new();
        let mut patterns: HashMap<&'static str, (usize, HashMap<&str, usize>)> = HashMap::new();
        for entry in entries {
            let class = &entry.class;
            let Some(role) = class.role else {
                *unknown.entry(class.anchor.as_str()).or_default() += 1;
                continue;
            };
            *counts.entry((class.eco, role)).or_default() += 1;
            let layer = layers.entry(role).or_insert_with(|| LayerSummary {
                role,
                files: 0,
                folders: Vec::new(),
                confirmed: 0,
                confirm_label: None,
                by_confidence: [0; 3],
            });
            layer.files += 1;
            let slot = match entry.confidence() {
                Confidence::High => 0,
                Confidence::Medium => 1,
                Confidence::Low => 2,
            };
            layer.by_confidence[slot] += 1;
            if let Some(label) = entry.confirmed {
                layer.confirmed += 1;
                *labels.entry(role).or_default().entry(label).or_default() += 1;
            }
            *folders
                .entry(role)
                .or_default()
                .entry(class.anchor.as_str())
                .or_default() += 1;
            if let Some(pattern) = class.pattern {
                let slot = patterns.entry(pattern).or_default();
                slot.0 += 1;
                if let Some(example) = class.pattern_example.as_deref() {
                    *slot.1.entry(example).or_default() += 1;
                }
            }
        }
        let mut layers: Vec<LayerSummary> = layers
            .into_values()
            .map(|mut layer| {
                layer.folders = ranked(folders.remove(&layer.role).unwrap_or_default())
                    .into_iter()
                    .map(|(folder, n)| (shown_folder(folder), n))
                    .collect();
                layer.confirm_label = labels
                    .remove(&layer.role)
                    .and_then(|l| ranked(l).into_iter().next())
                    .map(|(label, _)| label);
                layer
            })
            .collect();
        layers.sort_by_key(|layer| layer.role.display_order());
        let mut patterns: Vec<PatternSummary> = patterns
            .into_iter()
            .map(|(name, (files, examples))| PatternSummary {
                name,
                files,
                examples: ranked(examples)
                    .into_iter()
                    .take(3)
                    .map(|(e, _)| e.to_owned())
                    .collect(),
            })
            .collect();
        patterns.sort_by(|a, b| b.files.cmp(&a.files).then(a.name.cmp(b.name)));
        let unknown = ranked(unknown)
            .into_iter()
            .map(|(folder, n)| (shown_folder(folder), n))
            .collect();
        Self {
            architectures: architecture::detect(&counts, &frameworks),
            frameworks,
            layers,
            patterns,
            unknown,
            files: entries.len(),
            flow,
            graph_checked,
            overrides: None,
            warnings: Vec::new(),
        }
    }

    /// A profil bizonyossága: a legerősebb felismert architektúráé (ha nincs: alacsony).
    pub(crate) fn confidence(&self) -> Confidence {
        self.architectures
            .first()
            .map_or(Confidence::Low, |a| a.confidence)
    }
}

/// A gyökér mappája `.`-ként.
fn shown_folder(folder: &str) -> String {
    if folder.is_empty() {
        ".".into()
    } else {
        folder.to_owned()
    }
}

/// Darabszám szerint csökkenő, azonos darabnál név szerinti sorrend.
fn ranked<K: Ord + Copy>(counts: HashMap<K, usize>) -> Vec<(K, usize)> {
    let mut out: Vec<(K, usize)> = counts.into_iter().collect();
    out.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    out
}
