//! The `filament` view: one block per resource (model, pages, form fields, table columns, filters, actions, relation
//! managers), the standalone pages, widgets and panels, the findings of the resource <-> model contract and the footer
//! of the skipped items. The lines come from the project-level scan (`crate::packages::Filament`); the model facts
//! from the Laravel models view.

use crate::i18n::Lang;
use crate::query::QueryResult;
use crate::query::format::Formatter;

pub(super) struct FilamentView;

impl Formatter for FilamentView {
    fn format(&self, rows: &[QueryResult], lang: Lang) -> String {
        let laravel = rows.iter().find_map(|row| row.laravel.as_ref());
        let models = |short: &str| laravel.and_then(|laravel| laravel.model_info(short));
        let lines = rows
            .iter()
            .find_map(|row| row.filament.as_ref())
            .map(|filament| filament.lines(lang, &models))
            .unwrap_or_default();
        if lines.is_empty() {
            format!(
                "{}\n",
                lang.pick(
                    "(no Filament resources found)",
                    "(nincs Filament-resource találat)"
                )
            )
        } else {
            format!("{}\n", lines.join("\n"))
        }
    }
}
