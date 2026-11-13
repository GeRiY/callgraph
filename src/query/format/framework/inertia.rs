//! Az `inertia` nézet: oldalanként egy sor (`Pages/Board.vue  ← BoardController::show L23 · props: title, board`,
//! a hibák `!` jellel ugyanazon a soron), a végén a soha nem renderelt oldalak. A sorok a projekt-szintű kötésekből
//! (`crate::packages::Inertia`) jönnek — a lekérdezett fájlok csak a hatókört adják; kötések nélkül (nem
//! Inertia-projekt) az üres szöveg.

use crate::i18n::Lang;
use crate::query::QueryResult;
use crate::query::format::Formatter;

pub(super) struct InertiaView;

impl Formatter for InertiaView {
    fn format(&self, rows: &[QueryResult], lang: Lang) -> String {
        let lines = rows
            .iter()
            .find_map(|row| row.inertia.as_ref())
            .map(|inertia| {
                let scope: Vec<String> = rows
                    .iter()
                    .filter_map(|row| inertia.rel(&row.graph.file))
                    .collect();
                inertia.view_lines(&scope, lang)
            })
            .unwrap_or_default();
        if lines.is_empty() {
            format!(
                "{}\n",
                lang.pick("(no Inertia page matches)", "(nincs Inertia-oldal találat)")
            )
        } else {
            format!("{}\n", lines.join("\n"))
        }
    }
}
