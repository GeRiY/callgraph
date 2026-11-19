//! The `livewire` view: one block per component (class, template, uses, props, actions, events, validation rules),
//! the findings of the template <-> class contract and the footer of the skipped items. The lines come from the
//! project-level scan (`crate::packages::Livewire`); the queried files only give the scope.

use crate::i18n::Lang;
use crate::query::QueryResult;
use crate::query::format::Formatter;

pub(super) struct LivewireView;

impl Formatter for LivewireView {
    fn format(&self, rows: &[QueryResult], lang: Lang) -> String {
        let lines = rows
            .iter()
            .find_map(|row| row.livewire.as_ref())
            .map(|livewire| livewire.lines(lang))
            .unwrap_or_default();
        if lines.is_empty() {
            format!(
                "{}\n",
                lang.pick(
                    "(no Livewire components found)",
                    "(nincs Livewire-komponens találat)"
                )
            )
        } else {
            format!("{}\n", lines.join("\n"))
        }
    }
}
