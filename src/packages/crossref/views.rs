//! Blade-nézet ↔ használat: ki rendereli a nézetet (`view('admin.users.index')`, `View::make`, `Route::view`,
//! Mailable `->view(..)` a PHP-ban) és ki hivatkozik rá a többi sablonból (`@extends`, `@include`, `@each`,
//! `@component`, `<x-…>`). A PHP-oldal regexes olvasás (a hívó osztály és metódus a hívás előtti `class` / `function`
//! kulcsszóból): nem teljes elemzés, a dinamikus nevű render kimarad. Hibatűrő: ami nem olvasható, kimarad.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use regex::Regex;

use crate::core::LineIndex;
use crate::i18n::Lang;

/// A nézet legfeljebb ennyi használatát írja ki egy sor.
const SHOWN: usize = 3;

/// Egy használat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Use {
    /// `UserController::index` (PHP), `routes/web.php` (felső szint), vagy a hivatkozó nézet neve.
    pub who: String,
    pub line: u32,
    /// Sablonból: `@extends`, `@include`, …; PHP-ból üres.
    pub how: &'static str,
}

/// A projekt nézet-használatai nézet-név szerint.
#[derive(Debug, Default)]
pub(crate) struct Views {
    pub(crate) uses: BTreeMap<String, Vec<Use>>,
}

fn render_re() -> &'static Regex {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r#"(?:\bview\s*\(\s*|View::(?:make|first)\s*\(\s*|view\(\)\s*->make\s*\(\s*|->(?:view|markdown)\s*\(\s*|Route::view\s*\(\s*['"][^'"]*['"]\s*,\s*)['"]([A-Za-z0-9_.\-/:]+)['"]"#,
        )
        .expect("érvényes regex")
    });
    &RE
}

fn class_re() -> &'static Regex {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?m)^\s*(?:(?:final|abstract|readonly)\s+)*class\s+(\w+)")
            .expect("érvényes regex")
    });
    &RE
}

fn function_re() -> &'static Regex {
    static RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\bfunction\s+(\w+)\s*\(").expect("érvényes regex"));
    &RE
}

impl Views {
    /// Egy PHP-fájl render-helyei.
    pub(super) fn add_php(&mut self, rel: &str, text: &str) {
        if !text.contains("view") && !text.contains("View") && !text.contains("markdown") {
            return;
        }
        let lines = LineIndex::new(text);
        let classes: Vec<(usize, &str)> = class_re()
            .captures_iter(text)
            .filter_map(|c| Some((c.get(0)?.start(), c.get(1)?.as_str())))
            .collect();
        let functions: Vec<(usize, &str)> = function_re()
            .captures_iter(text)
            .filter_map(|c| Some((c.get(0)?.start(), c.get(1)?.as_str())))
            .collect();
        for caps in render_re().captures_iter(text) {
            let (Some(whole), Some(name)) = (caps.get(0), caps.get(1)) else {
                continue;
            };
            let at = whole.start();
            let class = classes.iter().rev().find(|(pos, _)| *pos < at);
            let function = functions.iter().rev().find(|(pos, _)| *pos < at);
            let who = match (class, function) {
                (Some((_, class)), Some((_, function))) => format!("{class}::{function}"),
                _ => rel.to_owned(),
            };
            self.uses
                .entry(crate::lang::blade::normalize(name.as_str()))
                .or_default()
                .push(Use {
                    who,
                    line: lines.line_of(at),
                    how: "",
                });
        }
    }

    /// Egy Blade-sablon hivatkozásai más nézetekre.
    pub(super) fn add_blade(&mut self, rel: &str, text: &str) {
        let who = crate::lang::blade::view_name(rel);
        for found in crate::lang::blade::view_refs(text) {
            self.uses.entry(found.view).or_default().push(Use {
                who: who.clone(),
                line: found.line,
                how: found.how,
            });
        }
    }

    /// A nézet fejléc-sorai az outline-ban: `rendered by: UserController::index L23 (+1)` és `used by:
    /// admin.users.index L1 @extends, … (+N)`; használat nélkül nincs sor.
    pub(crate) fn lines(&self, rel: &str, lang: Lang) -> Vec<String> {
        let name = crate::lang::blade::view_name(rel);
        let Some(uses) = self.uses.get(&name) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let (rendered, included): (Vec<&Use>, Vec<&Use>) =
            uses.iter().partition(|u| u.how.is_empty());
        let show = |u: &Use| {
            if u.how.is_empty() {
                format!("{} L{}", u.who, u.line)
            } else {
                format!("{} L{} {}", u.who, u.line, u.how)
            }
        };
        for (label, list) in [
            (lang.pick("rendered by: ", "renderelő: "), rendered),
            (lang.pick("used by: ", "használja: "), included),
        ] {
            if list.is_empty() {
                continue;
            }
            let shown: Vec<String> = list.iter().take(SHOWN).map(|u| show(u)).collect();
            let more = list.len().saturating_sub(SHOWN);
            let tail = if more > 0 {
                format!(" (+{more})")
            } else {
                String::new()
            };
            out.push(format!("{label}{}{tail}", shown.join(", ")));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controller_renders_and_blade_references_are_collected() {
        let mut views = Views::default();
        views.add_php(
            "app/Http/Controllers/UserController.php",
            "<?php\nclass UserController {\n  public function index() {\n    return view('admin.users.index', []);\n  }\n  public function show() { return View::make('admin.users.show'); }\n}\n",
        );
        views.add_php(
            "routes/web.php",
            "<?php\nRoute::view('/about', 'pages.about');\n",
        );
        views.add_blade(
            "resources/views/admin/users/index.blade.php",
            "@extends('layouts.app')\n@include('partials.row')\n<x-alert />\n",
        );
        let lines = views.lines("resources/views/admin/users/index.blade.php", Lang::En);
        assert_eq!(lines, ["rendered by: UserController::index L4"]);
        let layout = views.lines("resources/views/layouts/app.blade.php", Lang::En);
        assert_eq!(layout, ["used by: admin.users.index L1 @extends"]);
        let about = views.lines("resources/views/pages/about.blade.php", Lang::En);
        assert_eq!(about, ["rendered by: routes/web.php L2"]);
        let alert = views.lines("resources/views/components/alert.blade.php", Lang::En);
        assert_eq!(alert, ["used by: admin.users.index L3 <x-…>"]);
        assert!(
            views
                .lines("resources/views/nothing.blade.php", Lang::En)
                .is_empty()
        );
    }
}
