//! The `props` view: what a parent passes to a child component against what the child declares (missing required
//! props, unknown props, unknown slots). The lines come from the project-level scan
//! (`crate::packages::VueContract`); the queried files only give the scope.

use crate::i18n::Lang;
use crate::query::QueryResult;
use crate::query::format::Formatter;

pub(super) struct PropsView;

impl Formatter for PropsView {
    fn format(&self, rows: &[QueryResult], lang: Lang) -> String {
        let lines = rows
            .iter()
            .find_map(|row| row.vue_contract.as_ref())
            .map(|contract| contract.lines(lang))
            .unwrap_or_default();
        if lines.is_empty() {
            format!(
                "{}\n",
                lang.pick(
                    "(no component usages found)",
                    "(nincs komponens-használat találat)"
                )
            )
        } else {
            format!("{}\n", lines.join("\n"))
        }
    }
}
