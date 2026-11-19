//! The text of the Livewire output: the `livewire` view (a block of at most four lines per component), the outline
//! header lines of a component class / its template, and the `trace` chain of a component.

use std::sync::LazyLock;

use regex::Regex;

use super::Livewire;
use super::scan::{Class, ViewRef, kebab};
use crate::core::diag::Diagnostics;
use crate::i18n::Lang;

/// Names in one list before `+N`.
const MAX_NAMES: usize = 6;
/// Validation fields in one list before `+N`.
const MAX_FIELDS: usize = 8;
/// Warnings printed per component before `+N`.
const MAX_WARNINGS: usize = 3;
/// Use sites in a header / trace line before `+N`.
const MAX_SITES: usize = 3;

/// A finding of the template <-> class contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Warn {
    pub(super) file: String,
    pub(super) line: u32,
    pub(super) text: String,
}

/// The methods Livewire calls itself (never an action of the template).
fn is_lifecycle(name: &str) -> bool {
    const EXACT: [&str; 14] = [
        "mount",
        "boot",
        "booted",
        "render",
        "rendering",
        "rendered",
        "placeholder",
        "rules",
        "messages",
        "validationAttributes",
        "getListeners",
        "paginationView",
        "paginationSimpleView",
        "exception",
    ];
    const PREFIX: [&str; 6] = [
        "updating",
        "updated",
        "hydrate",
        "dehydrate",
        "boot",
        "mount",
    ];
    name.starts_with("__")
        || EXACT.contains(&name)
        || PREFIX.iter().any(|prefix| {
            name.strip_prefix(prefix)
                .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_uppercase()))
        })
}

/// `a, b, +2`: at most `max` names, the rest counted.
fn capped(names: &[String], max: usize) -> String {
    let mut text = names
        .iter()
        .take(max)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    if names.len() > max {
        text.push_str(&format!(", +{}", names.len() - max));
    }
    text
}

/// The template path without the `resources/views/` root.
fn short_view(rel: &str) -> &str {
    rel.strip_prefix("resources/views/").unwrap_or(rel)
}

/// Unique names, in order.
fn unique(names: impl Iterator<Item = String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for name in names {
        if !out.contains(&name) {
            out.push(name);
        }
    }
    out
}

/// The method an action value calls (`save`, `remove($id)`, `remove({{ $id }})`); `None`: a magic action (`$refresh`,
/// `$set(..)`, `$parent.x()`, `$dispatch(..)`), a JS expression or a name built in Blade.
pub(super) fn action_name(value: &str) -> Option<&str> {
    static RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^([A-Za-z_]\w*)\s*(?:\(|$)").expect("valid regex"));
    RE.captures(value.trim())
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str())
}

/// The first and the second segment of a `wire:model` path (`form.title`, `items.0.name`, `rows[2]`); `None`: a
/// `$`-prefixed or Blade-built value.
pub(super) fn model_path(value: &str) -> Option<(&str, Option<&str>)> {
    static RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^([A-Za-z_]\w*)(?:\.([A-Za-z_]\w*))?").expect("valid regex"));
    let caps = RE.captures(value.trim())?;
    let first = caps.get(1)?.as_str();
    Some((first, caps.get(2).map(|m| m.as_str())))
}

impl Livewire {
    /// The components the queried files belong to: the class, its template or the route file that serves it.
    fn in_scope(&self) -> Vec<usize> {
        self.components()
            .filter(|&index| {
                let class = &self.classes[index];
                self.scope.contains(&class.rel)
                    || self
                        .view_rel(class)
                        .is_some_and(|view| self.scope.contains(&view))
                    || self
                        .routes_of(index)
                        .iter()
                        .any(|route| self.scope.contains(&route.file))
            })
            .collect()
    }

    /// The contract findings of the component's template.
    pub(super) fn warnings(&self, index: usize, lang: Lang) -> Vec<Warn> {
        let class = &self.classes[index];
        let mut out = Vec::new();
        let Some(view) = self.view_rel(class) else {
            return out;
        };
        let Some(scan) = self.views.get(&view) else {
            return out;
        };
        if !class.complete {
            return out;
        }
        let owner = class.name.as_str();
        for bind in &scan.actions {
            let Some(method) = action_name(&bind.value) else {
                continue;
            };
            let found = class
                .methods
                .iter()
                .any(|m| m.public && m.name.eq_ignore_ascii_case(method));
            if !found {
                let attr = &bind.attr;
                out.push(Warn {
                    file: view.clone(),
                    line: bind.line,
                    text: crate::tfl!(
                        lang,
                        "{attr}: no public method '{method}' in {owner}",
                        "{attr}: nincs publikus metódus '{method}' a(z) {owner} osztályban"
                    ),
                });
            }
        }
        for bind in &scan.models {
            let Some((first, second)) = model_path(&bind.value) else {
                continue;
            };
            let attr = &bind.attr;
            let Some(prop) = class.props.iter().find(|p| p.public && p.name == first) else {
                out.push(Warn {
                    file: view.clone(),
                    line: bind.line,
                    text: crate::tfl!(
                        lang,
                        "{attr}: no public property '{first}' in {owner}",
                        "{attr}: nincs publikus property '{first}' a(z) {owner} osztályban"
                    ),
                });
                continue;
            };
            // `wire:model="form.title"`: the second segment is a property of the Form object
            let form = prop.ty.as_deref().and_then(|ty| self.form_of(ty));
            if let (Some(second), Some(form)) = (second, form)
                && form.complete
                && !form.props.iter().any(|p| p.public && p.name == second)
            {
                let owner = form.name.as_str();
                out.push(Warn {
                    file: view.clone(),
                    line: bind.line,
                    text: crate::tfl!(
                        lang,
                        "{attr}: no public property '{second}' in {owner}",
                        "{attr}: nincs publikus property '{second}' a(z) {owner} osztályban"
                    ),
                });
            }
        }
        out
    }

    /// The events a component dispatches and listens to (class and template), without repeats.
    fn events_of(&self, index: usize) -> (Vec<String>, Vec<String>) {
        let class = &self.classes[index];
        let view = self.view_rel(class).and_then(|rel| self.views.get(&rel));
        let dispatches = unique(
            class.dispatches.iter().map(|site| site.name.clone()).chain(
                view.into_iter()
                    .flat_map(|v| v.dispatches.iter().map(|(name, _)| name.clone())),
            ),
        );
        let listens = unique(
            class.listens.iter().map(|site| site.name.clone()).chain(
                view.into_iter()
                    .flat_map(|v| v.listens.iter().map(|(name, _)| name.clone())),
            ),
        );
        (dispatches, listens)
    }

    /// The validation fields of a component, with the ones of its Form objects as `form.field`.
    pub(super) fn rules_of(&self, index: usize) -> Vec<String> {
        let class = &self.classes[index];
        let mut out: Vec<String> = class
            .rules
            .iter()
            .map(|(field, required)| {
                if *required {
                    format!("{field}*")
                } else {
                    field.clone()
                }
            })
            .collect();
        for prop in &class.props {
            let Some(form) = prop.ty.as_deref().and_then(|ty| self.form_of(ty)) else {
                continue;
            };
            out.extend(form.rules.iter().map(|(field, required)| {
                let star = if *required { "*" } else { "" };
                format!("{}.{field}{star}", prop.name)
            }));
        }
        out
    }

    /// The public properties (`count`, `search (url)`) and the actions (public, non-lifecycle, non-computed methods).
    fn props_and_actions(class: &Class) -> (Vec<String>, Vec<String>) {
        let props = class
            .props
            .iter()
            .filter(|p| p.public)
            .map(|p| {
                if p.url {
                    format!("{} (url)", p.name)
                } else {
                    p.name.clone()
                }
            })
            .collect();
        let actions = class
            .methods
            .iter()
            .filter(|m| m.public && !m.computed && !is_lifecycle(&m.name))
            .map(|m| m.name.clone())
            .collect();
        (props, actions)
    }

    /// The skipped items of the components (and of the templates in scope) for the footer.
    fn diagnostics(&self, comps: &[usize]) -> Diagnostics {
        let mut out = Diagnostics::default();
        let mut seen_views: Vec<String> = self
            .views
            .keys()
            .filter(|rel| self.scope.contains(*rel))
            .cloned()
            .collect();
        for &index in comps {
            let class = &self.classes[index];
            for (code, line) in &class.dynamic {
                out.push(code, reason(code), &class.rel, *line);
            }
            if let Some(view) = self.view_rel(class) {
                seen_views.push(view);
            }
        }
        seen_views.sort();
        seen_views.dedup();
        for rel in seen_views {
            if let Some(scan) = self.views.get(&rel) {
                for (code, line) in &scan.dynamic {
                    out.push(code, reason(code), &rel, *line);
                }
            }
        }
        out
    }

    /// The `livewire` view: a header with the counts, one block per component, the footer of the skipped items.
    pub(crate) fn lines(&self, lang: Lang) -> Vec<String> {
        let comps = self.in_scope();
        if comps.is_empty() {
            return Vec::new();
        }
        let mut blocks: Vec<String> = Vec::new();
        let mut uses = 0;
        let mut warned = 0;
        for &index in &comps {
            let class = &self.classes[index];
            let used = self.uses_of(index).len();
            let warnings = self.warnings(index, lang);
            uses += used;
            warned += warnings.len();
            let mut first = format!("{}  {}", class.name, class.rel);
            if let Some(view) = self.view_rel(class) {
                first.push_str(&format!(" → {}", short_view(&view)));
            }
            first.push_str(&format!(" · {}: {used}", lang.pick("used", "használva")));
            if let Some(route) = self.routes_of(index).first() {
                first.push_str(&format!(" · route {} {}", route.verb, route.uri));
            }
            blocks.push(first);
            let (props, actions) = Self::props_and_actions(class);
            let mut parts = Vec::new();
            if !props.is_empty() {
                parts.push(format!("props: {}", capped(&props, MAX_NAMES)));
            }
            if !actions.is_empty() {
                parts.push(format!(
                    "{}: {}",
                    lang.pick("actions", "akciók"),
                    capped(&actions, MAX_NAMES)
                ));
            }
            if !parts.is_empty() {
                blocks.push(format!("  {}", parts.join(" · ")));
            }
            let (dispatches, listens) = self.events_of(index);
            let mut parts = Vec::new();
            if !dispatches.is_empty() {
                parts.push(format!(
                    "{} {}",
                    lang.pick("dispatches", "küld"),
                    capped(&dispatches, MAX_NAMES)
                ));
            }
            if !listens.is_empty() {
                parts.push(format!(
                    "{} {}",
                    lang.pick("listens", "figyel"),
                    capped(&listens, MAX_NAMES)
                ));
            }
            if !parts.is_empty() {
                blocks.push(format!(
                    "  {}: {}",
                    lang.pick("events", "események"),
                    parts.join(" · ")
                ));
            }
            let rules = self.rules_of(index);
            if !rules.is_empty() {
                blocks.push(format!(
                    "  {}: {}",
                    lang.pick("rules", "szabályok"),
                    capped(&rules, MAX_FIELDS)
                ));
            }
            for warn in warnings.iter().take(MAX_WARNINGS) {
                blocks.push(format!("  ⚠ {}  {} L{}", warn.text, warn.file, warn.line));
            }
            if warnings.len() > MAX_WARNINGS {
                blocks.push(format!("  ⚠ +{}", warnings.len() - MAX_WARNINGS));
            }
        }
        let count = comps.len();
        let mut head = match lang {
            Lang::En => format!("livewire: {count} components, {uses} uses"),
            Lang::Hu => format!("livewire: {count} komponens, {uses} használat"),
        };
        if warned > 0 {
            head.push_str(&format!(
                ", {warned} {}",
                lang.pick("warnings", "figyelmeztetés")
            ));
        }
        let mut out = vec![head];
        out.extend(blocks);
        out.extend(self.diagnostics(&comps).footer(&[], lang));
        out
    }

    /// The outline header lines of a file: a component class (`view:`, `used in:`) or the template of a component
    /// (the class, what it calls, what it binds, the findings).
    pub(crate) fn header(&self, file: &str, lang: Lang) -> Vec<String> {
        let Some(rel) = self.rel(file) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        if let Some(index) = self
            .components()
            .find(|&index| self.classes[index].rel == rel)
        {
            let class = &self.classes[index];
            match (self.view_rel(class), &class.view) {
                (Some(view), _) => out.push(format!("view: {view}")),
                (None, ViewRef::Named(name)) if !name.contains("::") && !self.views.is_empty() => {
                    out.push(crate::tfl!(
                        lang,
                        "⚠ view '{name}' not found",
                        "⚠ a(z) '{name}' nézet nem található"
                    ));
                }
                _ => {}
            }
            let uses = self.uses_of(index);
            if !uses.is_empty() {
                out.push(format!(
                    "{}: {}",
                    lang.pick("used in", "használja"),
                    sites(&uses)
                ));
            }
            return out;
        }
        let Some(index) = self
            .components()
            .find(|&index| self.view_rel(&self.classes[index]).as_deref() == Some(rel.as_str()))
        else {
            return out;
        };
        let class = &self.classes[index];
        out.push(format!("livewire: {} ({})", class.name, class.rel));
        let Some(scan) = self.views.get(&rel) else {
            return out;
        };
        let mut calls: Vec<String> = Vec::new();
        for bind in &scan.actions {
            let Some(method) = action_name(&bind.value) else {
                continue;
            };
            let known = class
                .methods
                .iter()
                .any(|m| m.public && m.name.eq_ignore_ascii_case(method));
            let label = format!("{method} L{}", bind.line);
            if known && !calls.iter().any(|c| c.starts_with(&format!("{method} L"))) {
                calls.push(label);
            }
        }
        if !calls.is_empty() {
            out.push(format!(
                "{}: {}",
                lang.pick("calls", "hív"),
                capped(&calls, MAX_SITES + 1)
            ));
        }
        let binds = unique(
            scan.models
                .iter()
                .filter_map(|bind| model_path(&bind.value).map(|(first, _)| first.to_owned())),
        );
        if !binds.is_empty() {
            out.push(format!(
                "{}: {}",
                lang.pick("binds", "köt"),
                capped(&binds, MAX_NAMES)
            ));
        }
        let warnings = self.warnings(index, lang);
        for warn in warnings.iter().take(MAX_WARNINGS + 2) {
            out.push(format!("⚠ {}  L{}", warn.text, warn.line));
        }
        if warnings.len() > MAX_WARNINGS + 2 {
            out.push(format!("⚠ +{}", warnings.len() - (MAX_WARNINGS + 2)));
        }
        out
    }

    /// The component a trace target names: `livewire:counter`, `livewire:admin.user-table`, `Counter`,
    /// `app/Livewire/Counter.php`.
    fn find(&self, target: &str) -> Option<usize> {
        let target = target.trim();
        let bare = target.strip_prefix("livewire:").unwrap_or(target);
        let key = bare.split('.').map(kebab).collect::<Vec<_>>().join(".");
        self.components().find(|&index| {
            let class = &self.classes[index];
            class.comp_name == key || class.name.eq_ignore_ascii_case(bare) || class.rel == target
        })
    }

    /// Every name a trace target could have been (for the "did you mean" hint).
    pub(crate) fn trace_candidates(&self) -> Vec<String> {
        self.components()
            .map(|index| format!("livewire:{}", self.classes[index].comp_name))
            .collect()
    }

    /// The chain of a component: class, route (full-page), template, uses, props, actions, events, rules, models
    /// (`models` gets the class file), findings. `None`: the target is no component.
    pub(crate) fn trace_lines(
        &self,
        target: &str,
        lang: Lang,
        models: &dyn Fn(&str) -> Vec<String>,
    ) -> Option<Vec<String>> {
        let index = self.find(target)?;
        let class = &self.classes[index];
        let mut out = vec![format!(
            "livewire {}  {}:{}",
            class.name, class.rel, class.line
        )];
        for route in self.routes_of(index).iter().take(2) {
            let mut line = format!("route {} {}", route.verb, route.uri);
            if let Some(name) = &route.name {
                line.push_str(&format!("  name={name}"));
            }
            line.push_str(&format!("  {}:{}", route.file, route.line));
            out.push(line);
        }
        match self.view_rel(class) {
            Some(view) => out.push(format!("view {view}")),
            None if class.view == ViewRef::Dynamic => {
                out.push(
                    lang.pick("view built at run time", "a nézet futásidőben épül")
                        .to_owned(),
                );
            }
            None => {}
        }
        let uses = self.uses_of(index);
        if !uses.is_empty() {
            out.push(format!(
                "{} {}: {}",
                lang.pick("used in", "használja"),
                uses.len(),
                sites(&uses)
            ));
        }
        let (props, actions) = Self::props_and_actions(class);
        if !props.is_empty() {
            out.push(format!("props {}", capped(&props, MAX_NAMES)));
        }
        if !actions.is_empty() {
            out.push(format!(
                "{} {}",
                lang.pick("actions", "akciók"),
                capped(&actions, MAX_NAMES)
            ));
        }
        let (dispatches, listens) = self.events_of(index);
        if !dispatches.is_empty() {
            out.push(format!(
                "{} {}",
                lang.pick("dispatches", "küld"),
                capped(&dispatches, MAX_NAMES)
            ));
        }
        if !listens.is_empty() {
            out.push(format!(
                "{} {}",
                lang.pick("listens", "figyel"),
                capped(&listens, MAX_NAMES)
            ));
        }
        let rules = self.rules_of(index);
        if !rules.is_empty() {
            out.push(format!(
                "{} {}",
                lang.pick("rules", "szabályok"),
                capped(&rules, MAX_FIELDS)
            ));
        }
        let used_models = models(&class.rel);
        if !used_models.is_empty() {
            out.push(format!(
                "{} {}",
                lang.pick("models", "modellek"),
                capped(&used_models, MAX_NAMES)
            ));
        }
        for warn in self.warnings(index, lang).iter().take(MAX_WARNINGS) {
            out.push(format!("⚠ {}  {} L{}", warn.text, warn.file, warn.line));
        }
        Some(out)
    }
}

/// `a.blade.php L3, routes/web.php L9, +1`.
fn sites(uses: &[(String, u32)]) -> String {
    let mut text = uses
        .iter()
        .take(MAX_SITES)
        .map(|(file, line)| format!("{file} L{line}"))
        .collect::<Vec<_>>()
        .join(", ");
    if uses.len() > MAX_SITES {
        text.push_str(&format!(", +{}", uses.len() - MAX_SITES));
    }
    text
}

/// The reason of a skipped item `(English, Hungarian)`.
fn reason(code: &str) -> (&'static str, &'static str) {
    match code {
        "lw-event" => (
            "event name is a run-time value",
            "az event neve futásidejű érték",
        ),
        "lw-listeners" => (
            "listeners are built at run time (getListeners)",
            "a listenerek futásidőben épülnek (getListeners)",
        ),
        "lw-component" => (
            "component name is a run-time value",
            "a komponens neve futásidejű érték",
        ),
        _ => (
            "validation rules are not a literal array",
            "a validációs szabályok nem literál tömbben vannak",
        ),
    }
}
