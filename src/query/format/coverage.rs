//! `coverage` formázó: a publikus metódusok docblock-lefedettsége, és a docblock nélküliek listája.

use std::fmt::Write as _;

use super::Formatter;
use super::selection::selected;
use crate::i18n::Lang;
use crate::query::QueryResult;

pub struct Coverage;

impl Formatter for Coverage {
    fn format(&self, rows: &[QueryResult], lang: Lang) -> String {
        let mut public = Vec::new();
        for row in rows {
            let graph = selected(row);
            for m in graph
                .methods
                .iter()
                .filter(|m| m.visibility.as_deref() == Some("public"))
            {
                public.push((graph.file.clone(), m.name.clone(), m.doc.is_some()));
            }
        }
        let total = public.len();
        let documented = public.iter().filter(|(_, _, doc)| *doc).count();
        let mut out = ltf!(
            lang,
            "coverage: {documented}/{total}",
            "lefedettség: {documented}/{total}"
        );
        if let Some(percent) = (documented * 100).checked_div(total) {
            write!(out, " ({percent}%)").unwrap();
        }
        out.push_str("\n\n");
        if total > documented {
            out.push_str(lang.pick(
                "public methods WITHOUT a docblock:\n",
                "docblock NÉLKÜLI publikus metódusok:\n",
            ));
            for (file, name, _) in public.iter().filter(|(_, _, doc)| !doc) {
                writeln!(out, "  {file}::{name}()").unwrap();
            }
        } else {
            out.push_str(lang.pick(
                "(all public methods documented)\n",
                "(minden publikus metódus dokumentálva)\n",
            ));
        }
        out
    }
}
