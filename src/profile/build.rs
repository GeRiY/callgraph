//! A profil előállítása egy gyökérre: a bejáró fájllistája (a `.callgraphignore`-t is tisztelve) → név-alapú
//! besorolás; a teljes változatban a projekt-gráfból (`crate::graph`, a többi lekérdezéssel közös `.callgraph/`)
//! a megerősítés (öröklés, árulkodó hívások) és a rétegek közti hívásirány is. Ha a gráf nem érhető el (írásvédett
//! projekt, hiba), a név-alapú profil marad — a profil soha nem bukik el emiatt.

use std::path::{Path, PathBuf};

use crate::analysis::Analyzer;
use crate::index::TypeHierarchy;
use crate::io::FileWalker;
use crate::model::FileGraph;

use super::classify::Classifier;
use super::confirm::confirm;
use super::ecosystem::manifest_frameworks;
use super::flow::{self, FlowFile};
use super::layers_file::slashed;
use super::project::{FileEntry, ProjectProfile};

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
    names_only(root, &files)
}

/// A teljes profil: a nevek és a projekt-gráf megerősítése, hívásiránya.
pub(crate) fn with_graph(root: &Path) -> ProjectProfile {
    let analyzer = Analyzer::with_defaults();
    let files = walk(root, &analyzer);
    let Some(shared) = crate::graph::shared(root) else {
        return names_only(root, &files);
    };
    let mut guard = crate::graph::lock(&shared);
    let ready = guard
        .refresh(&files, true, &analyzer)
        .and_then(|()| guard.ensure_callers(&analyzer));
    if ready.is_err() {
        return names_only(root, &files);
    }
    let _ = guard.save();
    let graph = &*guard;
    let classifier = Classifier::for_root(root);
    let mut entries: Vec<FileEntry> = Vec::new();
    let mut graphs: Vec<Option<&FileGraph>> = Vec::new();
    for file in &files {
        if let Some(entry) = entry(root, file, &classifier) {
            graphs.push(graph.slim_graph(&file.to_string_lossy()));
            entries.push(entry);
        }
    }
    let present: Vec<&FileGraph> = graphs.iter().flatten().copied().collect();
    let hierarchy = TypeHierarchy::from_graphs(present.iter().copied());
    for (entry, file_graph) in entries.iter_mut().zip(&graphs) {
        if let (Some(role), Some(file_graph)) = (entry.class.role, file_graph) {
            entry.confirmed = confirm(role, file_graph, &hierarchy);
        }
    }
    let mut frameworks = manifest_frameworks(root);
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
                role: entry.class.role,
                graph: (*file_graph)?,
            })
        })
        .collect();
    let flow = flow::compute(&flow_files);
    let profile = ProjectProfile::aggregate(&entries, frameworks, flow, true);
    with_overrides(profile, &classifier)
}

/// A név-alapú profil a már bejárt fájllistából.
fn names_only(root: &Path, files: &[PathBuf]) -> ProjectProfile {
    let classifier = Classifier::for_root(root);
    let entries: Vec<FileEntry> = files
        .iter()
        .filter_map(|file| entry(root, file, &classifier))
        .collect();
    let profile = ProjectProfile::aggregate(&entries, manifest_frameworks(root), None, false);
    with_overrides(profile, &classifier)
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

/// A `.callgraph-layers` helye és figyelmeztetései a profilba.
fn with_overrides(mut profile: ProjectProfile, classifier: &Classifier) -> ProjectProfile {
    let overrides = classifier.overrides();
    profile.overrides = overrides.source.clone();
    profile.warnings = overrides.warnings.clone();
    profile
}
