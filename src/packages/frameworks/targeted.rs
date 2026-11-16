//! Célzott betöltés az egy-fájlos outline-hoz / json-hoz: a framework-kötések adatai (route-tábla, űrlapok,
//! modell-összefoglaló, render-helyek) a teljes projekt-gráf nélkül, csak a ténylegesen szükséges fájlokból.
//! Ugyanazokat a `scan` / `scan_forms` / `scan_models` / `Inertia::scan` függvényeket hívja, csak szűkebb bemenettel,
//! így a kimenet bájtra azonos a teljes keresésével. A projekt-szintű nézetek (`ziggy`, `forms`, `models`,
//! `inertia`) továbbra is a teljes projekt-gráfot használják.

use std::path::{Path, PathBuf};

use super::files::ProjectFiles;
use super::{Inertia, Laravel, is_controller};
use crate::model::FileGraph;
use crate::query::QueryResult;

/// Mely adatok kellenek (az `annotate` kapui szerint).
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Need {
    pub inertia: bool,
    pub routes: bool,
    pub forms: bool,
    pub models: bool,
}

/// A JS / Vue fájlok kiterjesztései, amelyekben `useForm` küldés lehet.
const JS_EXTENSIONS: [&str; 7] = ["vue", "js", "jsx", "ts", "tsx", "mjs", "svelte"];
/// A PHP-források gyökerei, ahol render-hívás lehet.
const PHP_DIRS: [&str; 4] = ["app", "routes", "src", "Modules"];

/// Egy route-fájl gráfja csak a `use` importokkal (`use App\X;`, `use App\X as Y;`, `use App\{A, B as C};`): a
/// route-tábla a forrásból épül, a teljes elemzés itt fölösleges költség.
pub(super) fn imports_only(path: &Path, text: &str) -> FileGraph {
    static USE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"(?m)^\s*use\s+([\w\\]+?)(\\\{([^}]*)\})?(?:\s+as\s+(\w+))?\s*;")
            .expect("valid regex")
    });
    let mut graph = crate::model::FileGraphBuilder::new(path.to_string_lossy()).build();
    let mut add = |full: &str, alias: Option<&str>| {
        let full = full.trim().trim_start_matches('\\');
        let short = full.rsplit('\\').next().unwrap_or(full);
        graph
            .imports
            .insert(alias.unwrap_or(short).to_owned(), full.to_owned());
    };
    for caps in USE.captures_iter(text) {
        let base = &caps[1];
        match caps.get(3) {
            // csoportos import: minden elem a közös előtaggal
            Some(group) => {
                for item in group.as_str().split(',').filter(|i| !i.trim().is_empty()) {
                    let (name, alias) = match item.split_once(" as ") {
                        Some((name, alias)) => (name.trim(), Some(alias.trim())),
                        None => (item.trim(), None),
                    };
                    add(&format!("{base}\\{name}"), alias);
                }
            }
            None => add(base, caps.get(4).map(|m| m.as_str())),
        }
    }
    graph
}

/// A sor fájljának útja: abszolút marad, a relatív a gyökérhez képest értendő (mint a `relative_path`-ban).
pub(super) fn row_path(root: &Path, file: &str) -> PathBuf {
    let path = Path::new(file);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

/// A teljes keresés szűkített gráfjához hasonlóan: a hívások tárolt argumentuma nélkül (a render-helyek
/// argumentumai a forrásból jönnek); bináris fájl kimarad.
pub(super) fn slim(mut graph: FileGraph) -> Option<FileGraph> {
    if graph.binary {
        return None;
    }
    for call in &mut graph.calls {
        call.args = None;
    }
    Some(graph)
}

/// Közvetett modell lehet-e a sor osztálya: az Eloquent-alaposztálytól eltérő őst nevez meg (pl. `BaseModel`) —
/// ehhez az örökléslánc a teljes projekt-gráfot kéri.
pub(super) fn maybe_indirect_model(row: &QueryResult) -> bool {
    super::needs_models(row)
        && row.graph.containers.iter().any(|container| {
            container.kind == "class"
                && container
                    .extends
                    .as_ref()
                    .and_then(|extends| extends.first())
                    .is_some_and(|parent| !super::laravel::extends_model_base(parent))
        })
}

/// A célzott keresés. Az `analyze` a fájl szűkített gráfját adja (hibánál / binárisnál `None`: a fájl kimarad).
pub(super) fn scan(
    rows: &[QueryResult],
    inertia: Option<&mut Inertia>,
    laravel: Option<&mut Laravel>,
    need: Need,
    root: &Path,
    files: &dyn ProjectFiles,
    analyze: &dyn Fn(&Path) -> Option<FileGraph>,
) {
    if let Some(inertia) = inertia.filter(|_| need.inertia) {
        let graphs = render_graphs(root, files, analyze);
        inertia.scan(
            graphs.iter().map(|(path, graph)| (path.as_path(), graph)),
            files,
        );
    }
    let Some(laravel) = laravel else {
        return;
    };
    if need.routes {
        // a route-tábla a forrásból épül, a gráfból csak a `use` importok kellenek: teljes elemzés helyett ezek; a
        // lemezes cache-ből (ha van), amíg a route-fájlok / belépési pontok bélyege nem változik
        let inputs = laravel.route_fingerprint(files);
        if !laravel.apply_cached_routes(inputs.as_ref()) {
            let graphs: Vec<(PathBuf, FileGraph)> = files
                .files_under(&root.join("routes"))
                .into_iter()
                .filter(|path| path.extension().is_some_and(|ext| ext == "php"))
                .filter_map(|path| {
                    let graph = imports_only(&path, &files.read(&path)?);
                    Some((path, graph))
                })
                .collect();
            laravel.scan(
                graphs.iter().map(|(path, graph)| (path.as_path(), graph)),
                files,
                false,
            );
            laravel.store_routes(inputs);
        }
        if need.forms {
            scan_forms(rows, laravel, root, files, analyze);
        }
    }
    if need.models {
        // a lekérdezett osztályok közvetlen alaposztálya elég (a közvetett örökléshez a hívó a teljes keresést kéri);
        // a relációk / fillable / cast-ok a modell saját szövegéből, a séma a migrációk szövegéből jön
        let own = row_graphs(rows, root, |_| true);
        laravel.scan_models(
            own.iter().map(|(path, graph)| (path.as_path(), *graph)),
            files,
        );
    }
}

/// A lekérdezett sorok (út, gráf) párjai a `keep` szűrővel.
fn row_graphs<'r>(
    rows: &'r [QueryResult],
    root: &Path,
    keep: impl Fn(&FileGraph) -> bool,
) -> Vec<(PathBuf, &'r FileGraph)> {
    rows.iter()
        .filter(|row| keep(&row.graph))
        .map(|row| (row_path(root, &row.graph.file), &row.graph))
        .collect()
}

/// A render-hívást tartalmazó PHP-fájlok gráfjai: csak amelyek szövegében szerepel az `inertia`.
fn render_graphs(
    root: &Path,
    files: &dyn ProjectFiles,
    analyze: &dyn Fn(&Path) -> Option<FileGraph>,
) -> Vec<(PathBuf, FileGraph)> {
    PHP_DIRS
        .iter()
        .flat_map(|dir| files.files_under(&root.join(dir)))
        .filter(|path| path.extension().is_some_and(|ext| ext == "php"))
        .filter(|path| {
            files
                .read(path)
                .is_some_and(|text| text.to_ascii_lowercase().contains("inertia"))
        })
        .filter_map(|path| {
            let graph = analyze(&path)?;
            Some((path, graph))
        })
        .collect()
}

/// A `resources/` alatti JS / Vue fájlok, amelyekben `useForm` küldés lehet (rendezve).
fn js_paths(root: &Path, files: &dyn ProjectFiles) -> Vec<PathBuf> {
    files
        .files_under(&root.join("resources"))
        .into_iter()
        .filter(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| JS_EXTENSIONS.contains(&ext))
        })
        .collect()
}

/// Az űrlapok: a küldő (JS / Vue) fájlok és a lekérdezett PHP-fájlok gráfjából. Controller-sornál (a `form:` sor
/// az őt küldő oldalakat sorolja) a `resources/` összes `useForm`-ot tartalmazó fájljának küldései; különben csak a lekérdezett
/// JS / Vue fájlok. A controller és a FormRequest forrását a `scan_forms_for` a PSR-4-ből olvassa.
fn scan_forms(
    rows: &[QueryResult],
    laravel: &mut Laravel,
    root: &Path,
    files: &dyn ProjectFiles,
    analyze: &dyn Fn(&Path) -> Option<FileGraph>,
) {
    let is_php = |graph: &FileGraph| graph.file.ends_with(".php");
    let mut graphs = row_graphs(rows, root, is_php);
    if rows.iter().any(is_controller) {
        // a `useForm` küldések fájlonként (a lemezes cache-ből, ha van): se olvasás, se elemzés a változatlan fájlokra
        let sites = laravel.js_sites(&js_paths(root, files), files, analyze);
        laravel.scan_forms_with_sites(
            graphs.iter().map(|(path, graph)| (path.as_path(), *graph)),
            sites,
            files,
        );
        return;
    }
    graphs.extend(row_graphs(rows, root, |graph| !is_php(graph)));
    laravel.scan_forms_for(
        graphs.iter().map(|(path, graph)| (path.as_path(), *graph)),
        files,
    );
}
