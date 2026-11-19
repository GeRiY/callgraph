//! A `ziggy` nézet: az ismeretlen route-nevek (`⚠ unknown route name 'x'  resources/js/Pages/Foo.vue L12`) és egy
//! összegző sor (`routes: 42 named, 17 referenced from JS`). A sorok a projekt-szintű route-táblából
//! (`crate::packages::Laravel`) jönnek — a lekérdezett fájlok csak a hatókört adják; kötések nélkül (nem
//! Laravel-projekt, vagy nincs route-fájl) az üres szöveg.

use crate::i18n::Lang;
use crate::query::QueryResult;
use crate::query::format::Formatter;

pub(super) struct ZiggyView;

impl Formatter for ZiggyView {
    fn format(&self, rows: &[QueryResult], lang: Lang) -> String {
        let lines = rows
            .iter()
            .find_map(|row| row.laravel.as_ref())
            .map(|laravel| {
                let scope: Vec<String> = rows
                    .iter()
                    .filter_map(|row| laravel.rel(&row.graph.file))
                    .collect();
                laravel.view_lines(&scope, lang)
            })
            .unwrap_or_default();
        // without Laravel: the route names the client uses against the vue-router tables
        let lines = if lines.is_empty() {
            rows.iter()
                .find_map(|row| row.web_routes.as_ref())
                .map(|web| web.unknown_names(lang, true))
                .unwrap_or_default()
        } else {
            lines
        };
        if lines.is_empty() {
            format!(
                "{}\n",
                lang.pick("(no Laravel routes found)", "(nincs Laravel route találat)")
            )
        } else {
            format!("{}\n", lines.join("\n"))
        }
    }
}
