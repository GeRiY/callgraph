//! A `models` nézet: az Eloquent-modellek egy-egy sorban (`User users(8) → roles:Role*, groups:UserGroup*`), a
//! problémák (`⚠ User: fillable not a column: nick`) és egy összegző sor. A sorok a projekt-szintű kötésekből
//! (`crate::packages::Laravel`) jönnek — a lekérdezett fájlok csak a hatókört adják; kötések nélkül (nem Laravel
//! projekt) az üres szöveg.

use crate::i18n::Lang;
use crate::query::QueryResult;
use crate::query::format::Formatter;

pub(super) struct ModelsView;

impl Formatter for ModelsView {
    fn format(&self, rows: &[QueryResult], lang: Lang) -> String {
        let lines = rows
            .iter()
            .find_map(|row| row.laravel.as_ref())
            .map(|laravel| {
                let scope: Vec<String> = rows
                    .iter()
                    .filter_map(|row| laravel.rel(&row.graph.file))
                    .collect();
                laravel.model_view_lines(&scope, lang)
            })
            .unwrap_or_default();
        if lines.is_empty() {
            format!(
                "{}\n",
                lang.pick(
                    "(no Eloquent models found)",
                    "(nincs Eloquent-modell találat)"
                )
            )
        } else {
            format!("{}\n", lines.join("\n"))
        }
    }
}
