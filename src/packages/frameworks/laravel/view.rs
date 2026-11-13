//! A `ziggy` nézet sorai: minden ismeretlen route-névhez egy sor — `⚠ unknown route name 'x'  resources/js/Pages/
//! Foo.vue L12` (JS / Vue és PHP `route()` / `to_route()` / `redirect()->route()` hivatkozás) —, a végén egy
//! összegző sor: `routes: 42 named, 17 referenced from JS`. Más nincs (kevés token).

use std::collections::BTreeSet;

use super::Laravel;
use crate::i18n::Lang;

impl Laravel {
    /// A nézet sorai a hatókörre (`scope`: a lekérdezett fájlok a gyökérhez relatívan): az ismeretlen nevek csak a
    /// hatókör fájljaiból; az összegzés a projekt egészére szól. Üres: nincs route-tábla.
    pub(crate) fn view_lines(&self, scope: &[String], lang: Lang) -> Vec<String> {
        if self.routes.is_empty() {
            return Vec::new();
        }
        let in_scope = |file: &str| scope.iter().any(|have| have == file);
        let mut unknown: Vec<_> = self
            .refs
            .iter()
            .filter(|r| in_scope(&r.file) && !self.by_name.contains_key(&r.name))
            .collect();
        unknown.sort_by(|a, b| (&a.file, a.line, &a.name).cmp(&(&b.file, b.line, &b.name)));
        unknown.dedup_by(|a, b| (&a.file, a.line, &a.name) == (&b.file, b.line, &b.name));
        let mut lines: Vec<String> = unknown
            .iter()
            .map(|r| match lang {
                Lang::En => format!("⚠ unknown route name '{}'  {} L{}", r.name, r.file, r.line),
                Lang::Hu => format!(
                    "⚠ ismeretlen route-név '{}'  {} L{}",
                    r.name, r.file, r.line
                ),
            })
            .collect();
        let named = self.by_name.len();
        let from_js = self
            .refs
            .iter()
            .filter(|r| r.js && self.by_name.contains_key(&r.name))
            .map(|r| r.name.as_str())
            .collect::<BTreeSet<_>>()
            .len();
        lines.push(match lang {
            Lang::En => format!("routes: {named} named, {from_js} referenced from JS"),
            Lang::Hu => format!("route-ok: {named} elnevezett, {from_js} hivatkozva JS-ből"),
        });
        lines
    }
}
