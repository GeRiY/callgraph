//! A profil előállítása egy gyökérre: a bejáró fájllistája (a `.callgraphignore`-t is tisztelve) → név-alapú
//! besorolás; a teljes változatban a projekt-gráfból (`crate::graph`, a többi lekérdezéssel közös `.callgraph/`)
//! a megerősítés (öröklés, árulkodó hívások) és a rétegek közti hívásirány is. Ha a gráf nem érhető el (írásvédett
//! projekt, hiba), a név-alapú profil marad — a profil soha nem bukik el emiatt. A vendor- és generált kód
//! (`vendor.rs`) kimarad a rétegekből, a mintákból és a hívásirányból; a kód-jelet kívánó szerepek (`event`,
//! `listener`, `job`) megerősítés nélkül a tartalék besorolást kapják.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::analysis::Analyzer;
use crate::index::TypeHierarchy;
use crate::io::FileWalker;
use crate::model::FileGraph;

use super::classify::{Classifier, Reason};
use super::confirm::{confirm, enum_only};
use super::ecosystem::manifest_frameworks;
use super::flow::{self, FlowFile};
use super::layer::Layer;
use super::layers_file::slashed;
use super::project::{FileEntry, ProjectProfile};
use super::vendor::{Skip, Skipper};

/// A fájlok `framework` mezőjéből ezek számítanak a profilban (a többi — `js`, `swift`, `serde`… — nem mond
/// architektúrát).
const GRAPH_FRAMEWORKS: &[&str] = &[
    "laravel",
    "yii",
    "vue",
    "swiftui",
    "axum",
    "actix-web",
    "rocket",
    "warp",
    "poem",
    "leptos",
    "dioxus",
    "yew",
    "tauri",
    "bevy",
];

/// Csak a nevekből (mappák, fájlnevek, manifestek): gyors, elemzés nélkül — az MCP-kapcsolódáskor ez fut.
pub(crate) fn quick(root: &Path) -> ProjectProfile {
    let analyzer = Analyzer::with_defaults();
    let files = walk(root, &analyzer);
    names_only(root, &files, false)
}

/// A teljes profil: a nevek és a projekt-gráf megerősítése, hívásiránya. `with_vendor`: a vendor-kód is számít
/// (`profile --with-vendor`).
pub(crate) fn with_graph(root: &Path, with_vendor: bool) -> ProjectProfile {
    let analyzer = Analyzer::with_defaults();
    let files = walk(root, &analyzer);
    let Some(shared) = crate::graph::shared(root) else {
        return names_only(root, &files, with_vendor);
    };
    let mut guard = crate::graph::lock(&shared);
    let ready = guard
        .refresh(&files, true, &analyzer)
        .and_then(|()| guard.ensure_callers(&analyzer));
    if ready.is_err() {
        return names_only(root, &files, with_vendor);
    }
    let _ = guard.save();
    let graph = &*guard;
    let classifier = Classifier::for_root(root);
    let mut skipper = Skipper::new(root, !with_vendor);
    let mut skipped = Skipped::default();
    let mut entries: Vec<FileEntry> = Vec::new();
    let mut graphs: Vec<Option<&FileGraph>> = Vec::new();
    // a típus-hierarchia a kihagyott (vendor) fájlokból is: egy saját model őse lehet egy bemásolt alaposztály
    let mut every: Vec<&FileGraph> = Vec::new();
    for file in &files {
        let Some(entry) = entry(root, file, &classifier) else {
            continue;
        };
        let file_graph = graph.slim_graph(&file.to_string_lossy());
        every.extend(file_graph);
        if let Some(skip) = skipper.skip(&entry.rel) {
            skipped.add(skip);
            continue;
        }
        if file_graph.is_some_and(|g| g.binary) {
            skipped.add(Skip::Generated(folder_of(&entry.rel)));
            continue;
        }
        graphs.push(file_graph);
        entries.push(entry);
    }
    let hierarchy = TypeHierarchy::from_graphs(every.iter().copied());
    for (entry, file_graph) in entries.iter_mut().zip(&graphs) {
        settle(entry, *file_graph, &hierarchy, &classifier);
    }
    let mut frameworks = manifest_frameworks(root);
    let present: Vec<&FileGraph> = graphs.iter().flatten().copied().collect();
    for name in present.iter().filter_map(|g| g.framework.as_deref()) {
        if GRAPH_FRAMEWORKS.contains(&name) && !frameworks.iter().any(|have| have == name) {
            frameworks.push(name.to_owned());
        }
    }
    let flow_files: Vec<FlowFile<'_>> = entries
        .iter()
        .zip(&graphs)
        .filter_map(|(entry, file_graph)| {
            Some(FlowFile {
                rel: &entry.rel,
                layer: entry.class.layer(),
                graph: (*file_graph)?,
            })
        })
        .collect();
    let flow = flow::compute(&flow_files);
    let profile = ProjectProfile::aggregate(&entries, frameworks, flow, true);
    finish(profile, &classifier, skipped)
}

/// A megerősítés (C jel); kód-jelet kívánó szerepnél megerősítés nélkül a tartalék besorolás (és annak megerősítése).
/// Csak enumot tartalmazó fájlnál (`confirm::enum_only`) a fájlnév nem ad szerepet: a besorolás a név nélküli.
pub(super) fn settle(
    entry: &mut FileEntry,
    graph: Option<&FileGraph>,
    hierarchy: &TypeHierarchy,
    classifier: &Classifier,
) {
    let named = matches!(entry.class.reason, Reason::Name(_) | Reason::Conflict(..))
        && !entry.class.role.is_some_and(|role| role.name() == "enum");
    if named
        && graph.is_some_and(enum_only)
        && let Some(class) = classifier.classify_without_name(&entry.rel)
    {
        entry.class = class;
    }
    let check = |role: Option<super::role::Role>| match (role, graph) {
        (Some(role), Some(graph)) => confirm(role, graph, hierarchy),
        _ => None,
    };
    entry.confirmed = check(entry.class.role);
    if entry.confirmed.is_none()
        && let Some(fallback) = entry.class.fallback.take()
    {
        entry.class = *fallback;
        entry.confirmed = check(entry.class.role);
    }
}

/// A réteg-kapcsolók (`--layer`, `--layers`, `--by-layer`, `callers --by-layer`) rétegei a profillal azonos
/// besorolással: a projekt-gráf (már frissítve) szűkített tényeiből megerősítve (`settle`), a vendor- és generált
/// kód réteg nélkül. Kulcs: a gyökérhez relatív, `/`-es út.
pub(crate) fn settled_layers(
    root: &Path,
    files: &[PathBuf],
    graph: &crate::graph::ProjectGraph,
) -> HashMap<String, Option<Layer>> {
    let classifier = Classifier::for_root(root);
    let mut skipper = Skipper::new(root, true);
    let mut found: Vec<(FileEntry, Option<&FileGraph>)> = Vec::new();
    for file in files {
        let Some(entry) = entry(root, file, &classifier) else {
            continue;
        };
        found.push((entry, graph.slim_graph(&file.to_string_lossy())));
    }
    let hierarchy = TypeHierarchy::from_graphs(found.iter().filter_map(|(_, g)| *g));
    found
        .into_iter()
        .map(|(mut entry, file_graph)| {
            let skipped = skipper.skip(&entry.rel).is_some() || file_graph.is_some_and(|g| g.binary);
            let layer = if skipped {
                None
            } else {
                settle(&mut entry, file_graph, &hierarchy, &classifier);
                entry.class.layer()
            };
            (entry.rel, layer)
        })
        .collect()
}

/// A név-alapú profil a már bejárt fájllistából (kód-jel nincs: a kód-jelet kívánó szerepek a tartalékot kapják).
fn names_only(root: &Path, files: &[PathBuf], with_vendor: bool) -> ProjectProfile {
    let classifier = Classifier::for_root(root);
    let mut skipper = Skipper::new(root, !with_vendor);
    let mut skipped = Skipped::default();
    let mut entries: Vec<FileEntry> = Vec::new();
    for file in files {
        let Some(mut entry) = entry(root, file, &classifier) else {
            continue;
        };
        if let Some(skip) = skipper.skip(&entry.rel) {
            skipped.add(skip);
            continue;
        }
        if let Some(fallback) = entry.class.fallback.take() {
            entry.class = *fallback;
        }
        entries.push(entry);
    }
    let profile = ProjectProfile::aggregate(&entries, manifest_frameworks(root), None, false);
    finish(profile, &classifier, skipped)
}

/// A gyökér kódfájljai a bejárás (és a hívó-index) sorrendjében.
fn walk(root: &Path, analyzer: &Analyzer) -> Vec<PathBuf> {
    let files =
        FileWalker::for_registry(analyzer.registry()).collect(&[root.to_path_buf()], &mut |_| {});
    crate::graph::legacy_order(analyzer.registry(), files)
}

/// Egy fájl a profilhoz (`None`, ha nem kódfájl).
fn entry(root: &Path, file: &Path, classifier: &Classifier) -> Option<FileEntry> {
    let rel = slashed(file.strip_prefix(root).unwrap_or(file));
    let class = classifier.classify(&rel)?;
    Some(FileEntry {
        rel,
        class,
        confirmed: None,
    })
}

/// A fájl mappája (a gyökérnél `.`).
fn folder_of(rel: &str) -> String {
    rel.rsplit_once('/')
        .map_or_else(|| ".".to_owned(), |(dir, _)| dir.to_owned())
}

/// A kihagyott fájlok száma egység-mappánként.
#[derive(Default)]
struct Skipped {
    vendor: HashMap<String, usize>,
    generated: HashMap<String, usize>,
}

impl Skipped {
    fn add(&mut self, skip: Skip) {
        let (map, unit) = match skip {
            Skip::Vendor(unit) => (&mut self.vendor, unit),
            Skip::Generated(unit) => (&mut self.generated, unit),
        };
        *map.entry(unit).or_default() += 1;
    }
}

/// Darabszám szerint csökkenő, azonos darabnál név szerinti sorrend.
fn by_count(counts: HashMap<String, usize>) -> Vec<(String, usize)> {
    let mut out: Vec<(String, usize)> = counts.into_iter().collect();
    out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    out
}

/// A `.callgraph-layers` helye és figyelmeztetései, és a kihagyott kód a profilba.
fn finish(
    mut profile: ProjectProfile,
    classifier: &Classifier,
    skipped: Skipped,
) -> ProjectProfile {
    let overrides = classifier.overrides();
    profile.overrides = overrides.source.clone();
    profile.warnings = overrides.warnings.clone();
    profile.vendor = by_count(skipped.vendor);
    profile.generated = by_count(skipped.generated);
    profile
}
