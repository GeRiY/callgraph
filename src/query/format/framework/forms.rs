//! A `forms` nézet: az Inertia `useForm` űrlapok szerződés-eltérései (`⚠ resources/js/Pages/X.vue L12 → POST /x ·
//! FooController@store · unvalidated: a · required not sent: b`) és egy összegző sor (`forms: 5 checked, 1 with
//! issues, 0 without validation`). A sorok a projekt-szintű kötésekből (`crate::packages::Laravel`) jönnek — a
//! lekérdezett fájlok csak a hatókört adják; kötések nélkül (nem Laravel + Inertia projekt) az üres szöveg.

use crate::i18n::Lang;
use crate::query::QueryResult;
use crate::query::format::Formatter;

pub(super) struct FormsView;

impl Formatter for FormsView {
    fn format(&self, rows: &[QueryResult], lang: Lang) -> String {
        let lines = rows
            .iter()
            .find_map(|row| row.laravel.as_ref())
            .map(|laravel| {
                let scope: Vec<String> = rows
                    .iter()
                    .filter_map(|row| laravel.rel(&row.graph.file))
                    .collect();
                laravel.form_view_lines(&scope, lang)
            })
            .unwrap_or_default();
        if lines.is_empty() {
            format!(
                "{}\n",
                lang.pick("(no Inertia forms found)", "(nincs Inertia-űrlap találat)")
            )
        } else {
            format!("{}\n", lines.join("\n"))
        }
    }
}
