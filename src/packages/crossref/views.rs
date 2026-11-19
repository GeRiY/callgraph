//! Blade-nézet ↔ használat: ki rendereli a nézetet (`view('admin.users.index')`, `View::make`, `Route::view`,
//! Mailable `->view(..)` a PHP-ban) és ki hivatkozik rá a többi sablonból (`@extends`, `@include`, `@each`,
//! `@component`, `<x-…>`). A PHP-oldal regexes olvasás (a hívó osztály és metódus a hívás előtti `class` / `function`
//! kulcsszóból): nem teljes elemzés, a dinamikus nevű render kimarad. Hibatűrő: ami nem olvasható, kimarad.

use std::collections::{BTreeMap, BTreeSet};
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

/// A reference a template makes to another view (kept to check that the target exists).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Reference {
    view: String,
    line: u32,
    how: &'static str,
}

/// A custom Blade directive: where a provider registers it and where templates use it.
#[derive(Debug, Default)]
struct Directive {
    registered: Option<(String, u32)>,
    uses: Vec<(String, u32)>,
}

/// A projekt nézet-használatai nézet-név szerint.
#[derive(Debug, Default)]
pub(crate) struct Views {
    pub(crate) uses: BTreeMap<String, Vec<Use>>,
    /// Custom directives by name (`Blade::directive('datetime', ..)` / `Blade::if('admin', ..)`).
    directives: BTreeMap<String, Directive>,
    /// The view names of every Blade file, and the PHP files (class components live in `app/View/Components`).
    known_views: BTreeSet<String>,
    php_files: BTreeSet<String>,
    /// The static references of each template, by its view name.
    references: BTreeMap<String, Vec<Reference>>,
    /// The lines of `<x-dynamic-component>` tags, by view name.
    dynamic: BTreeMap<String, Vec<u32>>,
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

fn directive_re() -> &'static Regex {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"Blade::(?:directive|if)\(\s*['"]([A-Za-z_][A-Za-z0-9_]*)['"]"#)
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
        self.php_files.insert(rel.to_owned());
        if text.contains("Blade::") {
            let lines = LineIndex::new(text);
            for caps in directive_re().captures_iter(text) {
                let (Some(whole), Some(name)) = (caps.get(0), caps.get(1)) else {
                    continue;
                };
                self.directives
                    .entry(name.as_str().to_owned())
                    .or_default()
                    .registered = Some((rel.to_owned(), lines.line_of(whole.start())));
            }
        }
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
        self.known_views.insert(who.clone());
        for found in crate::lang::blade::view_refs(text) {
            if found.view.is_empty() {
                self.dynamic
                    .entry(who.clone())
                    .or_default()
                    .push(found.line);
                continue;
            }
            self.references
                .entry(who.clone())
                .or_default()
                .push(Reference {
                    view: found.view.clone(),
                    line: found.line,
                    how: found.how,
                });
            self.uses.entry(found.view).or_default().push(Use {
                who: who.clone(),
                line: found.line,
                how: found.how,
            });
        }
        for (name, line) in crate::lang::blade::directive_uses(text) {
            self.directives
                .entry(name)
                .or_default()
                .uses
                .push((who.clone(), line));
        }
    }

    /// Does the static reference have a target? A package-namespaced view (`mail::message`) is never missing; a
    /// component is also an `index` template or a class, and only counts as missing when the project has components
    /// of its own at all (a package may register the rest).
    fn target_exists(&self, view: &str) -> bool {
        if view.contains("::") || self.known_views.contains(view) {
            return true;
        }
        if view.starts_with("components.") {
            let has_components = self
                .known_views
                .iter()
                .any(|v| v.starts_with("components."))
                || self
                    .php_files
                    .iter()
                    .any(|f| f.starts_with("app/View/Components/"));
            return !has_components
                || self.known_views.contains(&format!("{view}.index"))
                || crate::lang::blade::component_class(view)
                    .is_some_and(|class| self.php_files.contains(&class));
        }
        false
    }

    /// The extra header lines of a Blade file (missing targets, dynamic components, custom directives used) and of a
    /// PHP file (the directives it registers).
    fn extra_lines(&self, rel: &str, lang: Lang) -> Vec<String> {
        let mut out = Vec::new();
        let capped = |items: Vec<String>| {
            let more = items.len().saturating_sub(SHOWN);
            let mut text = items.into_iter().take(SHOWN).collect::<Vec<_>>().join(", ");
            if more > 0 {
                text.push_str(&format!(", +{more}"));
            }
            text
        };
        if rel.ends_with(".blade.php") {
            let who = crate::lang::blade::view_name(rel);
            let missing: Vec<String> = self
                .references
                .get(&who)
                .into_iter()
                .flatten()
                .filter(|r| !self.target_exists(&r.view))
                .map(|r| format!("{} ({} L{})", r.view, r.how, r.line))
                .collect();
            if !missing.is_empty() {
                out.push(format!(
                    "{}{}",
                    lang.pick("⚠ missing view: ", "⚠ hiányzó nézet: "),
                    capped(missing)
                ));
            }
            if let Some(lines) = self.dynamic.get(&who) {
                let at: Vec<String> = lines.iter().map(|line| format!("L{line}")).collect();
                out.push(format!(
                    "{}{}",
                    lang.pick(
                        "? <x-dynamic-component>, target is a run-time value: ",
                        "? <x-dynamic-component>, a cél futásidejű érték: "
                    ),
                    capped(at)
                ));
            }
            let mut used: Vec<String> = Vec::new();
            for (name, directive) in &self.directives {
                let Some((file, line)) = &directive.registered else {
                    continue;
                };
                if directive.uses.iter().any(|(view, _)| *view == who) {
                    used.push(format!("@{name} → {file} L{line}"));
                }
            }
            if !used.is_empty() {
                out.push(format!(
                    "{}{}",
                    lang.pick("directives: ", "direktívák: "),
                    capped(used)
                ));
            }
        } else if rel.ends_with(".php") {
            let mut own: Vec<String> = Vec::new();
            for (name, directive) in &self.directives {
                let Some((file, line)) = &directive.registered else {
                    continue;
                };
                if file != rel {
                    continue;
                }
                let views: BTreeSet<&str> = directive
                    .uses
                    .iter()
                    .map(|(view, _)| view.as_str())
                    .collect();
                let tail = if views.is_empty() {
                    lang.pick("unused", "nincs használat").to_owned()
                } else {
                    let shown: Vec<&str> = views.iter().copied().take(SHOWN).collect();
                    let more = views.len().saturating_sub(SHOWN);
                    if more > 0 {
                        format!("{}, +{more}", shown.join(", "))
                    } else {
                        shown.join(", ")
                    }
                };
                own.push(format!("@{name} L{line} → {tail}"));
            }
            if !own.is_empty() {
                out.push(format!(
                    "{}{}",
                    lang.pick("blade directives: ", "blade direktívák: "),
                    capped(own)
                ));
            }
        }
        out
    }

    /// A nézet fejléc-sorai az outline-ban: `rendered by: UserController::index L23 (+1)` és `used by:
    /// admin.users.index L1 @extends, … (+N)`; használat nélkül nincs sor.
    pub(crate) fn lines(&self, rel: &str, lang: Lang) -> Vec<String> {
        let name = crate::lang::blade::view_name(rel);
        let mut out = Vec::new();
        let empty = Vec::new();
        let uses = self.uses.get(&name).unwrap_or(&empty);
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
        out.extend(self.extra_lines(rel, lang));
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
        // the targets exist (empty templates), so nothing is flagged missing
        for target in [
            "resources/views/layouts/app.blade.php",
            "resources/views/partials/row.blade.php",
            "resources/views/components/alert.blade.php",
        ] {
            views.add_blade(target, "");
        }
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

    #[test]
    fn missing_targets_dynamic_components_and_directives_are_flagged_or_linked() {
        let mut views = Views::default();
        views.add_blade(
            "resources/views/page.blade.php",
            "@include('partials.gone')\n@include('mail::message')\n@include('partials.here')\n<x-ghost />\n<x-dynamic-component :component=\"$c\" />\n@datetime($at)\n@click\n",
        );
        views.add_blade("resources/views/partials/here.blade.php", "");
        views.add_blade("resources/views/components/real.blade.php", "");
        views.add_php(
            "app/Providers/AppServiceProvider.php",
            "<?php\nclass AppServiceProvider {\n  public function boot() {\n    Blade::directive('datetime', fn ($e) => $e);\n  }\n}\n",
        );
        let page = views.lines("resources/views/page.blade.php", Lang::En);
        assert_eq!(
            page,
            [
                "⚠ missing view: partials.gone (@include L1), components.ghost (<x-…> L4)",
                "? <x-dynamic-component>, target is a run-time value: L5",
                "directives: @datetime → app/Providers/AppServiceProvider.php L4",
            ]
        );
        let provider = views.lines("app/Providers/AppServiceProvider.php", Lang::En);
        assert_eq!(provider, ["blade directives: @datetime L4 → page"]);
    }

    #[test]
    fn component_classes_and_index_templates_are_not_missing() {
        let mut views = Views::default();
        views.add_blade(
            "resources/views/page.blade.php",
            "<x-card />\n<x-forms.input />\n<x-real />\n",
        );
        views.add_blade("resources/views/components/card/index.blade.php", "");
        views.add_blade("resources/views/components/real.blade.php", "");
        views.add_php("app/View/Components/Forms/Input.php", "<?php\n");
        assert!(
            views
                .lines("resources/views/page.blade.php", Lang::En)
                .is_empty()
        );
        // no components of its own at all: a package may register them, nothing is flagged
        let mut bare = Views::default();
        bare.add_blade("resources/views/page.blade.php", "<x-ghost />\n");
        assert!(
            bare.lines("resources/views/page.blade.php", Lang::En)
                .is_empty()
        );
    }
}
