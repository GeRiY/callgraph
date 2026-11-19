//! The text of the Filament output: the `filament` view (a block of at most five lines per resource, then the
//! standalone pages, widgets and panels), the contract findings and the `trace` chain of a resource.

use super::scan::{Panel, Resource, Widget, WidgetKind};
use super::{Filament, ModelInfo, ModelLookup};
use crate::core::diag::Diagnostics;
use crate::i18n::Lang;
use crate::packages::frameworks::laravel::{plural, snake};

/// Names in one list before `+N`.
const MAX_NAMES: usize = 8;
/// Warnings printed per resource before `+N`.
const MAX_WARNINGS: usize = 3;
/// Models named by a widget before `+N`.
const MAX_MODELS: usize = 3;

/// A finding of the resource <-> model contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Warn {
    pub(super) file: String,
    pub(super) line: u32,
    pub(super) text: String,
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

/// `Post(posts)`.
fn model_label(info: &ModelInfo) -> String {
    format!("{}({})", info.class, info.table)
}

/// A column Eloquent computes (`comments_count`, `votes_sum_score`): never a table column.
fn is_aggregate(name: &str) -> bool {
    name.ends_with("_count")
        || name.ends_with("_exists")
        || ["_sum_", "_avg_", "_min_", "_max_"]
            .iter()
            .any(|part| name.contains(part))
}

/// The default URL segment of a resource: `Blog/PostResource` -> `blog/posts`.
fn default_slug(resource: &Resource) -> String {
    let stem = resource.rel.strip_suffix(".php").unwrap_or(&resource.rel);
    let tail = stem
        .rsplit_once("/Resources/")
        .map_or(resource.name.as_str(), |(_, tail)| tail);
    let parts: Vec<&str> = tail.split('/').collect();
    let last = parts.len().saturating_sub(1);
    parts
        .iter()
        .enumerate()
        .map(|(i, part)| {
            let word = snake(part.strip_suffix("Resource").unwrap_or(*part));
            let word = if i == last { plural(&word) } else { word };
            word.replace('_', "-")
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// `/admin` + `posts` -> `/admin/posts`.
fn join_uri(parts: &[&str]) -> String {
    let joined = parts
        .iter()
        .map(|part| part.trim_matches('/'))
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("/");
    format!("/{joined}")
}

impl Filament {
    /// The panel a class belongs to: the one that lists it, the one whose discovery directory holds it, or the only one.
    fn panel_of(&self, rel: &str, name: &str) -> Option<&Panel> {
        self.panels
            .iter()
            .find(|panel| panel.resources.iter().any(|listed| listed == name))
            .or_else(|| {
                self.panels.iter().find(|panel| {
                    panel
                        .discover
                        .iter()
                        .any(|dir| rel.starts_with(&format!("{dir}/")))
                })
            })
            .or_else(|| (self.panels.len() == 1).then(|| &self.panels[0]))
    }

    /// The URL of the resource index: the panel path and the slug.
    pub(super) fn base_uri(&self, resource: &Resource) -> String {
        let panel = self
            .panel_of(&resource.rel, &resource.name)
            .and_then(|panel| panel.path.as_deref())
            .unwrap_or("");
        let slug = resource
            .slug
            .clone()
            .unwrap_or_else(|| default_slug(resource));
        join_uri(&[panel, &slug])
    }

    /// The model catalogue entry with the members of the class read (once per lookup).
    fn enrich(&self, mut info: ModelInfo) -> ModelInfo {
        if info.members.is_none() {
            match std::fs::read_to_string(self.root.join(&info.file)) {
                Ok(text) => {
                    let (members, opaque) = super::scan::model_members(&text);
                    info.members = Some(members);
                    info.opaque |= opaque;
                }
                Err(_) => {
                    info.members = Some(Vec::new());
                    info.opaque = true;
                }
            }
        }
        info
    }

    /// The resources the queried files hold.
    fn in_scope(&self) -> Vec<usize> {
        (0..self.resources.len())
            .filter(|&i| self.scope.contains(&self.resources[i].rel))
            .collect()
    }

    /// The contract findings of one resource.
    pub(super) fn warnings(
        &self,
        resource: &Resource,
        lookup: &dyn Fn(&str) -> Option<ModelInfo>,
        lang: Lang,
    ) -> Vec<Warn> {
        let mut out = Vec::new();
        let Some(info) = resource.model.as_deref().and_then(lookup) else {
            return out;
        };
        if info.opaque {
            return out;
        }
        let model = model_label(&info);
        let known = info.columns.is_some();
        let mut warn = |file: &str, line: u32, text: String| {
            let item = Warn {
                file: file.to_owned(),
                line,
                text,
            };
            if !out.contains(&item) {
                out.push(item);
            }
        };
        for field in &resource.form {
            let name = &field.name;
            if field.check && known && !info.has_attr(name) {
                warn(
                    &resource.rel,
                    field.line,
                    crate::tfl!(
                        lang,
                        "form field '{name}' is not a column of {model}",
                        "a(z) '{name}' űrlapmező nem oszlopa ennek: {model}"
                    ),
                );
            }
            if let Some(relation) = &field.relation
                && !info.has_relation(relation)
            {
                warn(
                    &resource.rel,
                    field.line,
                    crate::tfl!(
                        lang,
                        "relationship '{relation}' is not a relation of {model}",
                        "a(z) '{relation}' kapcsolat nem reláció ennél: {model}"
                    ),
                );
            }
        }
        for column in resource
            .columns
            .iter()
            .filter(|c| c.check && !resource.own_query)
        {
            let name = &column.name;
            if is_aggregate(name) {
                continue;
            }
            match name.split_once('.') {
                Some((relation, rest)) => {
                    let Some((_, target)) = info.relations.iter().find(|(m, _)| m == relation)
                    else {
                        warn(
                            &resource.rel,
                            column.line,
                            crate::tfl!(
                                lang,
                                "table column '{name}': {model} has no relation '{relation}'",
                                "a(z) '{name}' oszlop: {model}-nek nincs '{relation}' relációja"
                            ),
                        );
                        continue;
                    };
                    let Some(other) = target.as_deref().and_then(lookup) else {
                        continue;
                    };
                    if !rest.contains('.')
                        && !other.opaque
                        && other.columns.is_some()
                        && !other.has_attr(rest)
                    {
                        let other = model_label(&other);
                        warn(
                            &resource.rel,
                            column.line,
                            crate::tfl!(
                                lang,
                                "table column '{name}': '{rest}' is not a column of {other}",
                                "a(z) '{name}' oszlop: '{rest}' nem oszlopa ennek: {other}"
                            ),
                        );
                    }
                }
                None => {
                    if known && !info.has_attr(name) {
                        warn(
                            &resource.rel,
                            column.line,
                            crate::tfl!(
                                lang,
                                "table column '{name}' is not a column of {model}",
                                "a(z) '{name}' tábla-oszlop nem oszlopa ennek: {model}"
                            ),
                        );
                    }
                }
            }
        }
        for class in &resource.relations {
            let Some(manager) = self.managers.iter().find(|m| &m.name == class) else {
                continue;
            };
            if let Some(relation) = &manager.relationship
                && !info.has_relation(relation)
            {
                warn(
                    &manager.rel,
                    manager.line,
                    crate::tfl!(
                        lang,
                        "{class}: {model} has no relation '{relation}'",
                        "{class}: {model}-nek nincs '{relation}' relációja"
                    ),
                );
            }
        }
        out
    }

    /// The `pages:` text of a resource: `index /admin/posts · create · edit /{record}/edit`.
    pub(super) fn pages_text(&self, resource: &Resource) -> String {
        let base = self.base_uri(resource);
        resource
            .pages
            .iter()
            .map(|page| {
                if page.path == "/" || page.path.is_empty() {
                    format!("{} {base}", page.key)
                } else if page.path.trim_start_matches('/') == page.key {
                    page.key.clone()
                } else {
                    format!("{} {}", page.key, page.path)
                }
            })
            .collect::<Vec<_>>()
            .join(" · ")
    }

    /// `title*, author_id (author)`.
    fn form_text(resource: &Resource) -> String {
        let names: Vec<String> = resource
            .form
            .iter()
            .map(|field| {
                let mut text = field.name.clone();
                if field.required {
                    text.push('*');
                }
                if let Some(relation) = &field.relation {
                    text.push_str(&format!(" ({relation})"));
                }
                text
            })
            .collect();
        capped(&names, MAX_NAMES)
    }

    /// `title, author.name · filters: status · actions: edit · bulk: delete`.
    fn table_text(resource: &Resource, lang: Lang) -> String {
        let mut parts = Vec::new();
        let columns: Vec<String> = resource.columns.iter().map(|c| c.name.clone()).collect();
        if !columns.is_empty() {
            parts.push(format!("table: {}", capped(&columns, MAX_NAMES)));
        }
        if !resource.filters.is_empty() {
            parts.push(format!("filters: {}", capped(&resource.filters, MAX_NAMES)));
        }
        if !resource.actions.is_empty() {
            parts.push(format!(
                "{}: {}",
                lang.pick("actions", "akciók"),
                capped(&resource.actions, MAX_NAMES)
            ));
        }
        if !resource.bulk.is_empty() {
            parts.push(format!("bulk: {}", capped(&resource.bulk, MAX_NAMES)));
        }
        parts.join(" · ")
    }

    /// The skipped items for the footer.
    fn diagnostics(&self, comps: &[usize]) -> Diagnostics {
        let mut out = Diagnostics::default();
        for &i in comps {
            let resource = &self.resources[i];
            for (code, line) in &resource.dynamic {
                out.push(code, reason(code), &resource.rel, *line);
            }
        }
        for panel in self.panels.iter().filter(|p| self.scope.contains(&p.rel)) {
            for (code, line) in &panel.dynamic {
                out.push(code, reason(code), &panel.rel, *line);
            }
        }
        out
    }

    /// The models a widget queries (`Post::count()`), as `Post(posts)`.
    fn widget_models(widget: &Widget, lookup: &dyn Fn(&str) -> Option<ModelInfo>) -> Vec<String> {
        widget
            .idents
            .iter()
            .filter_map(|ident| lookup(ident))
            .map(|info| model_label(&info))
            .collect()
    }

    /// The `filament` view: a header with the counts, one block per resource, the standalone pages, widgets and
    /// panels, the footer of the skipped items.
    pub(crate) fn lines(&self, lang: Lang, models: ModelLookup<'_>) -> Vec<String> {
        let lookup = |short: &str| models(short).map(|info| self.enrich(info));
        let comps = self.in_scope();
        let pages: Vec<_> = self
            .pages
            .iter()
            .filter(|p| self.scope.contains(&p.rel))
            .collect();
        let widgets: Vec<_> = self
            .widgets
            .iter()
            .filter(|w| self.scope.contains(&w.rel))
            .collect();
        let panels: Vec<_> = self
            .panels
            .iter()
            .filter(|p| self.scope.contains(&p.rel))
            .collect();
        if comps.is_empty() && pages.is_empty() && widgets.is_empty() && panels.is_empty() {
            return Vec::new();
        }
        let mut blocks: Vec<String> = Vec::new();
        let mut warned = 0;
        for &i in &comps {
            let resource = &self.resources[i];
            let mut first = format!("{}  {}", resource.name, resource.rel);
            if let Some(model) = &resource.model {
                let label = models(model).map_or_else(|| model.clone(), |info| model_label(&info));
                first.push_str(&format!(" → {label}"));
            }
            blocks.push(first);
            if !resource.pages.is_empty() {
                blocks.push(format!(
                    "  {}: {}",
                    lang.pick("pages", "oldalak"),
                    self.pages_text(resource)
                ));
            }
            if !resource.form.is_empty() {
                blocks.push(format!(
                    "  {}: {}",
                    lang.pick("form", "űrlap"),
                    Self::form_text(resource)
                ));
            }
            let table = Self::table_text(resource, lang);
            if !table.is_empty() {
                blocks.push(format!("  {table}"));
            }
            if !resource.relations.is_empty() {
                blocks.push(format!(
                    "  {}: {}",
                    lang.pick("relations", "kapcsolatok"),
                    capped(&resource.relations, MAX_NAMES)
                ));
            }
            let warnings = self.warnings(resource, &lookup, lang);
            warned += warnings.len();
            for warn in warnings.iter().take(MAX_WARNINGS) {
                blocks.push(format!("  ⚠ {}  {} L{}", warn.text, warn.file, warn.line));
            }
            if warnings.len() > MAX_WARNINGS {
                blocks.push(format!("  ⚠ +{}", warnings.len() - MAX_WARNINGS));
            }
        }
        for page in &pages {
            let panel = self.panel_of(&page.rel, &page.name);
            let prefix = panel.and_then(|p| p.path.as_deref()).unwrap_or("");
            let slug = page
                .slug
                .clone()
                .unwrap_or_else(|| snake(&page.name).replace('_', "-"));
            let uri = if page.name == "Dashboard" {
                join_uri(&[prefix])
            } else {
                join_uri(&[prefix, &slug])
            };
            let mut line = format!("page {}  {} · {uri}", page.name, page.rel);
            if let Some(view) = &page.view {
                line.push_str(&format!(" · view {view}"));
            }
            blocks.push(line);
        }
        for widget in &widgets {
            let kind = match widget.kind {
                WidgetKind::Stats => "stats",
                WidgetKind::Chart => "chart",
                WidgetKind::Table => "table",
                WidgetKind::Other => "widget",
            };
            let mut line = format!("widget {} ({kind})  {}", widget.name, widget.rel);
            let used = Self::widget_models(widget, &lookup);
            if !used.is_empty() {
                line.push_str(&format!(
                    " · {}: {}",
                    lang.pick("models", "modellek"),
                    capped(&used, MAX_MODELS)
                ));
            }
            blocks.push(line);
        }
        for panel in &panels {
            let mut line = format!("panel {}", panel.id.as_deref().unwrap_or("?"));
            if let Some(path) = &panel.path {
                line.push_str(&format!(" {}", join_uri(&[path])));
            }
            line.push_str(&format!("  {}", panel.rel));
            let names: Vec<String> = self
                .resources
                .iter()
                .filter(|r| {
                    self.panel_of(&r.rel, &r.name)
                        .is_some_and(|p| p.rel == panel.rel)
                })
                .map(|r| r.name.clone())
                .collect();
            if !names.is_empty() {
                line.push_str(&format!(
                    " · {}: {}",
                    lang.pick("resources", "resource-ok"),
                    capped(&names, MAX_NAMES)
                ));
            }
            blocks.push(line);
        }
        let mut counts = vec![
            (comps.len(), lang.pick("resources", "resource")),
            (pages.len(), lang.pick("pages", "oldal")),
            (widgets.len(), "widgets"),
            (panels.len(), "panels"),
            (warned, lang.pick("warnings", "figyelmeztetés")),
        ];
        counts.retain(|(n, _)| *n > 0);
        let head = counts
            .iter()
            .map(|(n, word)| format!("{word} {n}"))
            .collect::<Vec<_>>()
            .join(", ");
        let mut out = vec![format!("filament: {head}")];
        out.extend(blocks);
        out.extend(self.diagnostics(&comps).footer(&[], lang));
        out
    }

    /// The resource a trace target names: `filament:posts`, `PostResource`, `filament:Post`, the file path.
    fn find(&self, target: &str) -> Option<usize> {
        let target = target.trim();
        let bare = target.strip_prefix("filament:").unwrap_or(target);
        let low = bare.to_lowercase();
        self.resources.iter().position(|resource| {
            let slug = resource
                .slug
                .clone()
                .unwrap_or_else(|| default_slug(resource));
            resource.rel == target
                || resource.name.to_lowercase() == low
                || slug.rsplit('/').next() == Some(low.as_str())
                || resource
                    .model
                    .as_deref()
                    .is_some_and(|model| model.to_lowercase() == low)
        })
    }

    /// Every name a trace target could have been (for the "did you mean" hint).
    pub(crate) fn trace_candidates(&self) -> Vec<String> {
        self.resources
            .iter()
            .map(|resource| {
                let slug = resource
                    .slug
                    .clone()
                    .unwrap_or_else(|| default_slug(resource));
                format!("filament:{}", slug.rsplit('/').next().unwrap_or(&slug))
            })
            .collect()
    }

    /// The chain of a resource: class, model + table, panel, pages with URIs, form fields, table columns, filters,
    /// actions, relation managers, findings. `None`: the target is no resource.
    pub(crate) fn trace_lines(
        &self,
        target: &str,
        lang: Lang,
        models: ModelLookup<'_>,
    ) -> Option<Vec<String>> {
        let resource = &self.resources[self.find(target)?];
        let lookup = |short: &str| models(short).map(|info| self.enrich(info));
        let mut out = vec![format!(
            "filament {}  {}:{}",
            resource.name, resource.rel, resource.line
        )];
        if let Some(model) = &resource.model {
            out.push(match models(model) {
                Some(info) => format!("model {}  {}", model_label(&info), info.file),
                None => format!("model {model}"),
            });
        }
        if let Some(panel) = self.panel_of(&resource.rel, &resource.name) {
            let mut line = format!("panel {}", panel.id.as_deref().unwrap_or("?"));
            if let Some(path) = &panel.path {
                line.push_str(&format!(" {}", join_uri(&[path])));
            }
            line.push_str(&format!("  {}:{}", panel.rel, panel.line));
            out.push(line);
        }
        let base = self.base_uri(resource);
        let pages: Vec<String> = resource
            .pages
            .iter()
            .map(|page| {
                let uri = if page.path == "/" || page.path.is_empty() {
                    base.clone()
                } else {
                    format!("{base}{}", page.path)
                };
                format!("{} {uri} ({})", page.key, page.class)
            })
            .collect();
        if !pages.is_empty() {
            out.push(format!(
                "{} {}",
                lang.pick("pages", "oldalak"),
                capped(&pages, MAX_NAMES)
            ));
        }
        if !resource.form.is_empty() {
            out.push(format!(
                "{} {}",
                lang.pick("form", "űrlap"),
                Self::form_text(resource)
            ));
        }
        let table = Self::table_text(resource, lang).replacen("table: ", "table ", 1);
        if !table.is_empty() {
            out.push(table.replace(" · ", " | "));
        }
        let managers: Vec<String> = resource
            .relations
            .iter()
            .map(|class| {
                match self
                    .managers
                    .iter()
                    .find(|m| &m.name == class)
                    .and_then(|m| m.relationship.as_deref())
                {
                    Some(relationship) => format!("{class} → {relationship}"),
                    None => class.clone(),
                }
            })
            .collect();
        if !managers.is_empty() {
            out.push(format!(
                "{} {}",
                lang.pick("relations", "kapcsolatok"),
                capped(&managers, MAX_NAMES)
            ));
        }
        for warn in self
            .warnings(resource, &lookup, lang)
            .iter()
            .take(MAX_WARNINGS)
        {
            out.push(format!("⚠ {}  {} L{}", warn.text, warn.file, warn.line));
        }
        Some(out)
    }
}

/// The reason of a skipped item `(English, Hungarian)`.
fn reason(code: &str) -> (&'static str, &'static str) {
    match code {
        "fi-field" => (
            "form field name is a run-time value",
            "az űrlapmező neve futásidejű érték",
        ),
        "fi-column" => (
            "table column name is a run-time value",
            "a tábla-oszlop neve futásidejű érték",
        ),
        "fi-filter" => (
            "filter name is a run-time value",
            "a szűrő neve futásidejű érték",
        ),
        "fi-action" => (
            "action name is a run-time value",
            "az akció neve futásidejű érték",
        ),
        "fi-page" => (
            "page route is a run-time value",
            "az oldal route-ja futásidejű érték",
        ),
        _ => (
            "panel path / resources are run-time values",
            "a panel útvonala / resource-ai futásidejű értékek",
        ),
    }
}
