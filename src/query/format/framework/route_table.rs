//! A `routes` nézet: Laravel-projektben a projekt route-táblájából (`crate::packages::Laravel`: csoportok, `resource`,
//! betöltött fájlok, konfigból generált route-ok) ír — ugyanabból a forrásból, mint a `ziggy`, az `inertia` és a
//! `forms` nézet. Minden sor: ige, teljes URI, kezelő, név, middleware, `fájl:sor`. Ami nincs a táblában (más
//! framework, vagy a hatókör fájljai nem route-fájlok), a régi, gráf-alapú gyűjtő sorai.

use super::wired::scope;
use super::{RuleView, rules, selected};
use crate::i18n::Lang;
use crate::query::QueryResult;
use crate::query::format::Formatter;

pub(super) struct RoutesView;

impl Formatter for RoutesView {
    fn format(&self, rows: &[QueryResult], lang: Lang) -> String {
        let legacy = RuleView {
            empty: ("(no route matches)", "(nincs route találat)"),
            collect: rules::routes,
        };
        // TS / JS route tables (vue-router, Hono / Express, `route("GET", /re/, h)`) and the client calls to them
        let web = rows
            .iter()
            .find_map(|row| row.web_routes.as_ref())
            .map(|web| web.lines(lang))
            .unwrap_or_default();
        let laravel = rows
            .iter()
            .find_map(|row| row.laravel.as_ref())
            .filter(|laravel| laravel.has_routes());
        if !web.is_empty() {
            let mut lines = laravel.map_or_else(Vec::new, |laravel| {
                let (files, classes) = scope(rows, laravel);
                laravel.route_lines(&files, &classes)
            });
            lines.extend(web);
            lines.extend(laravel.map_or_else(Vec::new, |laravel| {
                laravel.route_footer(&scope(rows, laravel).0, lang)
            }));
            return format!("{}\n", lines.join("\n"));
        }
        let Some(laravel) = laravel else {
            return legacy.format(rows, lang);
        };
        let (files, classes) = scope(rows, laravel);
        let mut lines = laravel.route_lines(&files, &classes);
        if lines.is_empty() {
            return legacy.format(rows, lang);
        }
        // a nem route-fájlok route-szerű sztringjei (`path "/renewals"`) a régi gyűjtőből
        let table = laravel.route_files();
        for row in rows {
            let in_table = laravel
                .rel(&row.graph.file)
                .is_some_and(|rel| table.contains(rel.as_str()));
            if in_table {
                continue;
            }
            let graph = selected(row);
            let ns = graph
                .namespace
                .as_ref()
                .map(|n| format!("{n}\\"))
                .unwrap_or_default();
            rules::routes(&graph, &ns, &mut lines);
        }
        lines.extend(laravel.route_footer(&files, lang));
        format!("{}\n", lines.join("\n"))
    }
}
