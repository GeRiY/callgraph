//! Framework-kötések a lekérdezésekhez (Strategy): fájlokon átívelő framework-tudás, amelyet a kimenetek egy-egy
//! linkként mutatnak (nem új szekcióként). A stratégiát a `Packages::frameworks()` választja ki (a `FRAMEWORKS`
//! regiszter neve szerint); egyszerre több is lehet aktív (Laravel + Inertia). Ha a projekt nem használja, a kimenet
//! bájtra a régi.
//!
//! - `inertia/`: a PHP `Inertia::render('X')` ↔ a page-komponens (`resources/js/Pages/X.vue`);
//! - `laravel/`: a route-tábla (`routes/*.php`) ↔ a JS `route('név')` hívás és a controller-metódus;
//! - `files.rs`: DI a fájlrendszerhez (page-gyökér, page-lista, a render-hívások forrása).
//!
//! Minden hibatűrő: ami nem olvasható / nem ismerhető fel, az kimarad, soha nem hiba.

mod files;
mod inertia;
pub(crate) mod laravel;
mod targeted;

use std::path::Path;
use std::sync::Arc;

use crate::analysis::Analyzer;
use crate::locate::ProjectSource;
use crate::query::QueryResult;

pub(crate) use files::DiskProjectFiles;
pub(crate) use inertia::{Inertia, PageLink};
pub(crate) use laravel::Laravel;

/// A sorok a framework-kötésekkel (`format`: a kért nézet). Inertia-projektben a sorok megkapják a kötéseket (az
/// outline / json render-hívásának page-fájlja); a projekt render-helyei (egy menet a projekt-gráf szűkített
/// tényein) csak akkor töltődnek, ha kellenek: az `inertia` nézetnél, és az outline-nál, ha egy sor oldal-fájl
/// (`rendered by:`). Laravel-projektben a route-tábla (a `routes/*.php` forrásából) csak akkor, ha kell: a `ziggy`
/// nézetnél, és az outline / json-nál, ha egy sor Ziggy `route(..)` hívást tartalmaz vagy controller-fájl. Framework
/// nélkül a sorok érintetlenek. A projekt-szintű nézetek (`ziggy`, `forms`, `models`, `inertia`) a teljes
/// projekt-gráfot töltik; az egy-fájlos outline / json csak a szükséges fájlokat (`targeted.rs`).
pub(crate) fn annotate(rows: &mut [QueryResult], root: &Path, format: &str) {
    let Some(first) = rows.first() else {
        return;
    };
    let packages = match &first.packages {
        Some(packages) => Arc::clone(packages),
        None => Arc::new(super::open(root)),
    };
    let mut inertia = Inertia::detect(root, &packages, &DiskProjectFiles);
    let mut laravel = Laravel::detect(root, &packages);
    if inertia.is_none() && laravel.is_none() {
        return;
    }
    let scan_inertia = inertia.as_ref().is_some_and(|inertia| {
        format == "inertia"
            || (format == "outline" && rows.iter().any(|row| inertia.is_page(&row.graph.file)))
    });
    // az űrlap-szerződés (`useForm` ↔ route ↔ validáció) csak Laravel + Inertia projektben
    let with_forms = laravel.is_some() && inertia.is_some();
    let scan_forms = with_forms
        && (format == "forms"
            || (matches!(format, "outline" | "json" | "jsonl") && rows.iter().any(needs_forms)));
    let scan_laravel = laravel.is_some()
        && (format == "ziggy"
            || scan_forms
            || (matches!(format, "outline" | "json" | "jsonl") && rows.iter().any(needs_routes)));
    // az Eloquent-modell összefoglaló (modellek + migrációk) csak modell-fájlnál és a `models` nézetnél
    let scan_models = laravel.is_some()
        && (format == "models"
            || (matches!(format, "outline" | "json" | "jsonl") && rows.iter().any(needs_models)));
    if scan_inertia || scan_laravel || scan_models {
        let analyzer = Analyzer::with_defaults();
        // a projekt-szintű nézetek (és a közvetett modell-öröklés) a teljes projekt-gráfot kérik; az egy-fájlos
        // outline / json csak a szükséges fájlokat tölti (`targeted`)
        let project_wide = matches!(format, "ziggy" | "forms" | "models" | "inertia")
            || (scan_models && rows.iter().any(targeted::maybe_indirect_model));
        if project_wide {
            // a projekt-gráf hibája csak a render-helyeket / route-táblát hagyja üresen
            let _ = ProjectSource::new(root, &analyzer).with_files(|files| {
                let code = || {
                    files
                        .iter()
                        .filter(|file| file.code)
                        .map(|file| (file.path, file.graph))
                };
                if let Some(inertia) = inertia.as_mut().filter(|_| scan_inertia) {
                    inertia.scan(code(), &DiskProjectFiles);
                }
                if let Some(laravel) = laravel.as_mut() {
                    if scan_laravel {
                        laravel.scan(code(), &DiskProjectFiles, format == "ziggy");
                        if scan_forms {
                            laravel.scan_forms(code(), &DiskProjectFiles);
                        }
                    }
                    if scan_models {
                        laravel.scan_models(code(), &DiskProjectFiles);
                    }
                }
            });
        } else {
            let need = targeted::Need {
                inertia: scan_inertia,
                routes: scan_laravel,
                forms: scan_forms,
                models: scan_models,
            };
            let analyze = |path: &Path| {
                analyzer
                    .analyze_path(path)
                    .ok()
                    .and_then(|analysis| targeted::slim(analysis.graph))
            };
            targeted::scan(
                rows,
                inertia.as_mut(),
                laravel.as_mut(),
                need,
                root,
                &DiskProjectFiles,
                &analyze,
            );
        }
    }
    let shared_inertia = inertia.map(Arc::new);
    let shared_laravel = laravel
        .filter(|_| scan_laravel || scan_models)
        .map(Arc::new);
    for row in rows {
        row.inertia.clone_from(&shared_inertia);
        row.laravel.clone_from(&shared_laravel);
    }
}

/// Kell-e a route-tábla a sorhoz: Ziggy `route('név')` hívás van benne (JS / Vue), vagy controller-fájl (a
/// metódusok `route:` sora).
fn needs_routes(row: &QueryResult) -> bool {
    row.graph
        .calls
        .iter()
        .any(|call| laravel::route_name(call).is_some())
        || is_controller(row)
}

/// Controller-fájl-e a sor (a metódusok `route:` / `form:` sora).
fn is_controller(row: &QueryResult) -> bool {
    let graph = &row.graph;
    graph.file.ends_with(".php")
        && (graph.file.contains("/Controllers/")
            || graph
                .containers
                .iter()
                .any(|c| c.name.ends_with("Controller")))
}

/// Kell-e az űrlap-ellenőrzés a sorhoz: `useForm` küldés van benne (`form.post(..)` …), vagy controller-fájl (a
/// metódusok `form:` sora).
fn needs_forms(row: &QueryResult) -> bool {
    is_controller(row)
        || row.graph.calls.iter().any(|call| {
            call.resolved
                .as_deref()
                .is_some_and(|r| r.starts_with("@inertiajs/vue3::useForm"))
        })
}

/// Kell-e a modell-összefoglaló a sorhoz: PHP-fájl, amelyben Eloquent-alaposztályból öröklő osztály van (vagy a
/// `Models/` mappában áll — a közvetett örökléshez).
fn needs_models(row: &QueryResult) -> bool {
    let graph = &row.graph;
    graph.file.ends_with(".php")
        && (graph.file.contains("Models/")
            || graph.containers.iter().any(|c| {
                c.extends
                    .iter()
                    .flatten()
                    .any(|name| laravel::extends_model_base(name))
            }))
}
