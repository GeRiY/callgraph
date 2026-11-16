//! Az `inertia` nézet sorai: oldalanként egy sor — `Pages/Board.vue  ← BoardController::show L23 · props: title,
//! board`, a hibák ugyanazon a soron `!` jellel (a page-fájl hiányzik; a render-hely olyan props-ot ad át, amelyet
//! az oldal `defineProps`-a nem deklarál — csak ha mindkét oldal statikusan ismert) —, a végén egy sor a PHP-ból
//! soha nem renderelt oldalakkal (`unrendered: …`, legfeljebb 10).

use std::collections::{BTreeMap, BTreeSet};

use super::{Inertia, RenderSite};
use crate::i18n::Lang;

/// A nem renderelt oldalak sorában legfeljebb ennyi név.
const MAX_UNRENDERED: usize = 10;
/// Egy sorban legfeljebb ennyi render-hely (a többi `+N`).
const MAX_SITES: usize = 2;

impl Inertia {
    /// A nézet sorai a hatókörre (`scope`: a lekérdezett fájlok a gyökérhez relatívan): az oldal, ha a render-helye
    /// vagy maga a page-fájl a hatókörben van. A `unrendered` sor a projekt egészére szól. Üres: nincs mit mutatni.
    pub(crate) fn view_lines(&self, scope: &[String], lang: Lang) -> Vec<String> {
        let in_scope = |file: &str| scope.iter().any(|have| have == file);
        let mut pages: BTreeMap<&str, Vec<&RenderSite>> = BTreeMap::new();
        for site in &self.sites {
            let page_in_scope = self
                .resolve(&site.page)
                .is_some_and(|link| in_scope(&link.rel));
            if in_scope(&site.file) || page_in_scope {
                pages.entry(site.page.as_str()).or_default().push(site);
            }
        }
        let mut lines: Vec<String> = pages
            .into_iter()
            .filter_map(|(page, sites)| self.page_line(page, &sites, lang))
            .collect();
        if let Some(line) = self.unrendered_line(lang) {
            lines.push(line);
        }
        lines
    }

    fn page_line(&self, page: &str, sites: &[&RenderSite], lang: Lang) -> Option<String> {
        let link = self.resolve(page)?;
        let shown: Vec<String> = sites.iter().take(MAX_SITES).map(|s| s.label()).collect();
        let mut line = format!("{}  ← {}", link.short, shown.join(", "));
        if sites.len() > MAX_SITES {
            line.push_str(&format!(" +{}", sites.len() - MAX_SITES));
        }
        // az átadott props-ok a statikus render-helyekről, az első előfordulás sorrendjében
        let mut passed: Vec<&str> = Vec::new();
        for props in sites.iter().filter_map(|site| site.props.as_ref()) {
            for prop in props {
                if !passed.contains(&prop.as_str()) {
                    passed.push(prop);
                }
            }
        }
        if !passed.is_empty() {
            line.push_str(&format!(" · props: {}", passed.join(", ")));
        }
        if !link.exists {
            line.push_str(lang.pick("  ! page file missing", "  ! a page-fájl hiányzik"));
        } else if let Some(declared) = self.declared.get(&link.rel) {
            let undeclared: Vec<&str> = passed
                .iter()
                .copied()
                .filter(|prop| !declared.iter().any(|d| d.as_str() == *prop))
                .collect();
            if !undeclared.is_empty() {
                let list = undeclared.join(", ");
                line.push_str(&match lang {
                    Lang::En => format!("  ! props not declared: {list}"),
                    Lang::Hu => format!("  ! nem deklarált props: {list}"),
                });
            }
        }
        Some(line)
    }

    /// A PHP-ból (render-hívással vagy `Route::inertia`-val) soha nem renderelt oldalak; mind renderelt: `None`.
    fn unrendered_line(&self, lang: Lang) -> Option<String> {
        let rendered: BTreeSet<&str> = self.sites.iter().map(|site| site.page.as_str()).collect();
        let unrendered: Vec<String> = self
            .files
            .iter()
            .filter(|rel| {
                self.pages
                    .page_name(rel)
                    .is_some_and(|name| !rendered.contains(name.as_str()))
            })
            .map(|rel| self.pages.short(rel))
            .collect();
        if unrendered.is_empty() {
            return None;
        }
        let mut list = unrendered
            .iter()
            .take(MAX_UNRENDERED)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        if unrendered.len() > MAX_UNRENDERED {
            list.push_str(&format!(", +{}", unrendered.len() - MAX_UNRENDERED));
        }
        // dinamikus render (változó page-név) mellett csak annyi biztos, hogy statikusan nem renderelik
        let Some((file, line)) = self.dynamic.first() else {
            return Some(format!(
                "{}{list}",
                lang.pick("unrendered: ", "nem renderelt: ")
            ));
        };
        let more = match self.dynamic.len() {
            1 => String::new(),
            n => format!(" (+{})", n - 1),
        };
        Some(format!(
            "{}{list} · {}{file} L{line}{more}",
            lang.pick("not rendered statically: ", "statikusan nem renderelt: "),
            lang.pick("dynamic render: ", "dinamikus render: ")
        ))
    }
}
