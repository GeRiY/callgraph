//! Framework-kötések a lekérdezésekhez (Strategy): fájlokon átívelő framework-tudás, amelyet a kimenetek egy-egy
//! linkként mutatnak (nem új szekcióként). A stratégiát a `Packages::frameworks()` választja ki (a `FRAMEWORKS`
//! regiszter neve szerint); egyszerre több is lehet aktív (Laravel + Inertia). Ha a projekt nem használja, a kimenet
//! bájtra a régi.
//!
//! - `inertia/`: a PHP `Inertia::render('X')` ↔ a page-komponens (`resources/js/Pages/X.vue`);
//! - `files.rs`: DI a fájlrendszerhez (page-gyökér, page-lista, a render-hívások forrása).
//!
//! Minden hibatűrő: ami nem olvasható / nem ismerhető fel, az kimarad, soha nem hiba.

mod files;
mod inertia;

use std::path::Path;
use std::sync::Arc;

use crate::analysis::Analyzer;
use crate::locate::ProjectSource;
use crate::query::QueryResult;

pub(crate) use files::DiskProjectFiles;
pub(crate) use inertia::{Inertia, PageLink};

/// A sorok a framework-kötésekkel (`format`: a kért nézet). Inertia-projektben a sorok megkapják a kötéseket (az
/// outline / json render-hívásának page-fájlja); a projekt render-helyei (egy menet a projekt-gráf szűkített
/// tényein) csak akkor töltődnek, ha kellenek: az `inertia` nézetnél, és az outline-nál, ha egy sor oldal-fájl
/// (`rendered by:`). Inertia nélkül a sorok érintetlenek.
pub(crate) fn annotate(rows: &mut [QueryResult], root: &Path, format: &str) {
    let Some(first) = rows.first() else {
        return;
    };
    let packages = match &first.packages {
        Some(packages) => Arc::clone(packages),
        None => Arc::new(super::open(root)),
    };
    let Some(mut inertia) = Inertia::detect(root, &packages, &DiskProjectFiles) else {
        return;
    };
    let scan = format == "inertia"
        || (format == "outline" && rows.iter().any(|row| inertia.is_page(&row.graph.file)));
    if scan {
        let analyzer = Analyzer::with_defaults();
        // a projekt-gráf hibája csak a render-helyeket hagyja üresen
        let _ = ProjectSource::new(root, &analyzer).with_files(|files| {
            let code = files
                .iter()
                .filter(|file| file.code)
                .map(|file| (file.path, file.graph));
            inertia.scan(code, &DiskProjectFiles);
        });
    }
    let shared = Arc::new(inertia);
    for row in rows {
        row.inertia = Some(Arc::clone(&shared));
    }
}
