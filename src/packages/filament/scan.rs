//! Text scanners of the Filament module: the resource class (`$model`, `getPages()`, the `form()` schema, the
//! `table()` columns / filters / actions, `getRelations()`), relation managers, standalone pages, widgets and panel
//! providers. Works on the source text with the shared scanners (`core::srcscan`): comments are blanked, and a second
//! copy with the string contents blanked (`mask`) serves the structure regexes. Offsets are the same in all copies.
//!
//! A component is a `X::make(..)` call whose class is imported from `Filament\..`; its fluent chain
//! (`->required()->relationship('author')`) is read call by call, so nested schemas (`Section::make()->schema([..])`)
//! need no recursion: every component of the method body is found, and its parent is the innermost component whose
//! argument span holds it.

use std::sync::LazyLock;

use regex::Regex;

use crate::core::LineIndex;
use crate::core::srcscan::{Dialect, matching, split_args, strip_comments};
use crate::packages::livewire::scan::{body_of, mask_strings, single_literal};

/// A regex compiled once.
macro_rules! re {
    ($pattern:literal) => {{
        static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new($pattern).expect("valid regex"));
        &*RE
    }};
}

/// Layout components: they only hold other components (never a field / column / action themselves).
const CONTAINERS: [&str; 15] = [
    "Section",
    "Grid",
    "Fieldset",
    "Tabs",
    "Tab",
    "Wizard",
    "Step",
    "Group",
    "Split",
    "Stack",
    "Card",
    "ColumnGroup",
    "ActionGroup",
    "BulkActionGroup",
    "Panel",
];
/// The chain calls (and `make(..)` itself) whose arguments hold the children of a layout component.
const FLOW: [&str; 7] = [
    "make",
    "schema",
    "tabs",
    "components",
    "steps",
    "childComponents",
    "columns",
];
/// Form components that are no model attribute (never listed, never checked).
const NON_FIELDS: [&str; 5] = ["Placeholder", "ViewField", "View", "Actions", "Livewire"];

/// A field of the resource form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Field {
    pub(super) name: String,
    pub(super) line: u32,
    pub(super) required: bool,
    /// `->relationship('author', ..)` (the field name when there is no argument): checked against the model.
    pub(super) relation: Option<String>,
    /// The name is checked against the model attributes.
    pub(super) check: bool,
}

/// A column of the resource table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Col {
    pub(super) name: String,
    pub(super) line: u32,
    pub(super) check: bool,
}

/// One entry of `getPages()`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PageRef {
    pub(super) key: String,
    pub(super) class: String,
    /// `::route('/{record}/edit')`.
    pub(super) path: String,
}

/// A resource class.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Resource {
    pub(super) rel: String,
    pub(super) name: String,
    pub(super) line: u32,
    /// The short class name of `$model`.
    pub(super) model: Option<String>,
    pub(super) model_line: u32,
    pub(super) slug: Option<String>,
    pub(super) pages: Vec<PageRef>,
    pub(super) form: Vec<Field>,
    pub(super) columns: Vec<Col>,
    pub(super) filters: Vec<String>,
    pub(super) actions: Vec<String>,
    pub(super) bulk: Vec<String>,
    /// The relation manager classes of `getRelations()`.
    pub(super) relations: Vec<String>,
    /// `table()` sets its own query: the columns are not the model's.
    pub(super) own_query: bool,
    /// Skipped items: `(reason code, line)`.
    pub(super) dynamic: Vec<(&'static str, u32)>,
}

/// A relation manager class.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Manager {
    pub(super) rel: String,
    pub(super) name: String,
    pub(super) line: u32,
    pub(super) relationship: Option<String>,
}

/// A standalone page class.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PageClass {
    pub(super) rel: String,
    pub(super) name: String,
    pub(super) slug: Option<String>,
    pub(super) view: Option<String>,
}

/// What a widget shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WidgetKind {
    Stats,
    Chart,
    Table,
    Other,
}

/// A widget class.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Widget {
    pub(super) rel: String,
    pub(super) name: String,
    pub(super) kind: WidgetKind,
    /// The capitalised names the class calls statically (`Post::count()`): the models are among them.
    pub(super) idents: Vec<String>,
}

/// A panel provider.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Panel {
    pub(super) rel: String,
    pub(super) line: u32,
    pub(super) id: Option<String>,
    pub(super) path: Option<String>,
    pub(super) resources: Vec<String>,
    /// The project-relative directories of `discoverResources(in: app_path(..))`.
    pub(super) discover: Vec<String>,
    pub(super) dynamic: Vec<(&'static str, u32)>,
}

/// What a scanned file is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Found {
    Resource(Box<Resource>),
    Manager(Manager),
    Page(PageClass),
    Widget(Widget),
    Panel(Box<Panel>),
}

// ------------------------------------------------------------------------------------------------ helpers

/// The `use` lines of a file: `(alias, full name)`.
fn imports_of(clean: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for caps in re!(r"(?m)^\s*use\s+(?P<fqn>\\?[\w\\]+)(?:\s+as\s+(?P<alias>\w+))?\s*;")
        .captures_iter(clean)
    {
        let Some(fqn) = caps.name("fqn") else {
            continue;
        };
        let fqn = fqn.as_str().trim_start_matches('\\').to_owned();
        let alias = match caps.name("alias") {
            Some(alias) => alias.as_str().to_owned(),
            None => fqn.rsplit('\\').next().unwrap_or("").to_owned(),
        };
        out.push((alias, fqn));
    }
    for caps in
        re!(r"(?m)^\s*use\s+(?P<base>\\?[\w\\]+?)\\\{(?P<items>[^}]*)\}\s*;").captures_iter(clean)
    {
        let (Some(base), Some(items)) = (caps.name("base"), caps.name("items")) else {
            continue;
        };
        let base = base.as_str().trim_start_matches('\\');
        for item in items.as_str().split(',') {
            let item = item.trim();
            let (path, alias) = match item.split_once(" as ") {
                Some((path, alias)) => (path.trim(), alias.trim()),
                None => (item, item.rsplit('\\').next().unwrap_or(item)),
            };
            if !path.is_empty() {
                out.push((alias.to_owned(), format!("{base}\\{path}")));
            }
        }
    }
    out
}

/// The full name of a class written as `X`, `Forms\Components\X` or `\Filament\..\X`.
fn resolve(imports: &[(String, String)], written: &str) -> Option<String> {
    let written = written.trim_start_matches('\\');
    let first = written.split('\\').next()?;
    let rest = &written[first.len()..];
    imports
        .iter()
        .find(|(alias, _)| alias == first)
        .map(|(_, full)| format!("{full}{rest}"))
        .or_else(|| written.contains('\\').then(|| written.to_owned()))
}

/// The last segment of a class name.
fn short_of(name: &str) -> &str {
    name.rsplit('\\').next().unwrap_or(name)
}

/// The braces of the body of `function <name>(`.
fn method_body(mask: &str, name: &str) -> Option<(usize, usize)> {
    let pattern = format!(r"function\s+{name}\s*\(");
    let found = Regex::new(&pattern).ok()?.find(mask)?;
    body_of(mask, found.end() - 1, mask.len())
}

/// The value of a literal property default (`$slug = 'x'`).
fn string_prop(clean: &str, name: &str) -> Option<String> {
    let pattern = format!(r#"\${name}\s*=\s*(?:'([^']*)'|"([^"$]*)")"#);
    let caps = Regex::new(&pattern).ok()?.captures(clean)?;
    caps.get(1)
        .or_else(|| caps.get(2))
        .map(|m| m.as_str().to_owned())
}

// ------------------------------------------------------------------------------------------------ components

/// The name argument of a `make(..)` call.
#[derive(Debug, Clone, PartialEq, Eq)]
enum NameArg {
    /// `Hidden::make()`, `EditAction::make()`.
    Missing,
    Lit(String),
    /// A name built at run time (or no string: `Grid::make(2)`).
    Dynamic,
}

/// One `X::make(..)` component with its fluent chain.
#[derive(Debug)]
struct Comp {
    short: String,
    line: u32,
    start: usize,
    args: (usize, usize),
    name: NameArg,
    chain: Vec<(String, (usize, usize))>,
}

impl Comp {
    fn call(&self, name: &str) -> Option<(usize, usize)> {
        self.chain
            .iter()
            .find(|(call, _)| call == name)
            .map(|(_, span)| *span)
    }

    fn has(&self, name: &str) -> bool {
        self.call(name).is_some()
    }

    /// The call has no argument or `true` (`->required()`), as opposed to a closure.
    fn flag(&self, clean: &str, name: &str) -> bool {
        self.call(name).is_some_and(|(a, b)| {
            let text = clean[a..b].trim();
            text.is_empty() || text == "true"
        })
    }
}

/// The fluent chain after a call: `->name(args)` pairs with the byte range of the arguments.
fn chain_of(clean: &str, mut pos: usize, to: usize) -> Vec<(String, (usize, usize))> {
    let mut out = Vec::new();
    while pos <= to {
        let Some(after) = clean[pos..to].trim_start().strip_prefix("->") else {
            break;
        };
        let after = after.trim_start();
        let len = after
            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(after.len());
        if len == 0 {
            break;
        }
        let tail = after[len..].trim_start();
        if !tail.starts_with('(') {
            break;
        }
        let open = to - tail.len();
        let Some(close) = matching(clean, open, to, Dialect::Php) else {
            break;
        };
        out.push((after[..len].to_owned(), (open + 1, close)));
        pos = close + 1;
    }
    out
}

/// The Filament components (`X::make(..)`, `X` imported from `Filament\..`) between `from` and `to`, in order.
fn comps_in(
    clean: &str,
    mask: &str,
    imports: &[(String, String)],
    lines: &LineIndex,
    from: usize,
    to: usize,
) -> Vec<Comp> {
    let mut out = Vec::new();
    let Some(region) = mask.get(from..to) else {
        return out;
    };
    for caps in re!(r"(\\?[A-Za-z_][\w\\]*)::make\s*\(").captures_iter(region) {
        let (Some(whole), Some(class)) = (caps.get(0), caps.get(1)) else {
            continue;
        };
        let Some(full) = resolve(imports, class.as_str()) else {
            continue;
        };
        if !full.starts_with("Filament\\") {
            continue;
        }
        let start = from + whole.start();
        let open = from + whole.end() - 1;
        let Some(close) = matching(clean, open, to, Dialect::Php) else {
            continue;
        };
        let name = match split_args(clean, open + 1, close, Dialect::Php).first() {
            None => NameArg::Missing,
            Some(&(a, b)) => {
                let text = clean[a..b].trim();
                let text = text.strip_prefix("name:").map_or(text, str::trim);
                single_literal(text).map_or(NameArg::Dynamic, NameArg::Lit)
            }
        };
        out.push(Comp {
            short: short_of(&full).to_owned(),
            line: lines.line_of(start),
            start,
            args: (open + 1, close),
            name,
            chain: chain_of(clean, close + 1, to),
        });
    }
    out
}

/// The innermost component whose arguments (or chain arguments) hold component `i`, and the call they belong to.
fn parent_of(comps: &[Comp], i: usize) -> Option<(usize, String)> {
    let at = comps[i].start;
    let mut best: Option<(usize, String)> = None;
    for (j, other) in comps.iter().enumerate().take(i) {
        let spans = std::iter::once(("make", other.args)).chain(
            other
                .chain
                .iter()
                .map(|(call, span)| (call.as_str(), *span)),
        );
        for (via, (a, b)) in spans {
            if a <= at
                && at < b
                && best
                    .as_ref()
                    .is_none_or(|(k, _)| comps[*k].start < other.start)
            {
                best = Some((j, via.to_owned()));
            }
        }
    }
    best
}

/// `None`: the component sits inside a field / action (a repeater item, a modal form), not in the schema itself;
/// `Some(boundary)`: it is part of the schema, and `boundary` says that a layout above it switches the model
/// (`->relationship()`, `->statePath()`).
fn placement(comps: &[Comp], parents: &[Option<(usize, String)>], mut i: usize) -> Option<bool> {
    let mut boundary = false;
    while let Some((parent, via)) = &parents[i] {
        let holder = &comps[*parent];
        if !CONTAINERS.contains(&holder.short.as_str()) || !FLOW.contains(&via.as_str()) {
            return None;
        }
        boundary |= holder.has("relationship") || holder.has("statePath");
        i = *parent;
    }
    Some(boundary)
}

/// The relation named by `->relationship('author', 'name')`: `None` no such call, `Some(None)` a name built at run
/// time, `Some(Some(name))`; without arguments the relation is the field name.
fn relation_arg(clean: &str, comp: &Comp, field: &str) -> Option<Option<String>> {
    let (a, b) = comp.call("relationship")?;
    let args = split_args(clean, a, b, Dialect::Php);
    let Some(&(x, y)) = args.first() else {
        return Some(Some(field.to_owned()));
    };
    let text = clean[x..y].trim();
    let text = text.strip_prefix("name:").map_or(text, str::trim);
    Some(single_literal(text))
}

/// The action name of `EditAction::make()` / `DeleteBulkAction`: `edit`, `delete`, `force-delete`.
fn derived_name(short: &str, suffixes: &[&str]) -> String {
    let base = suffixes
        .iter()
        .find_map(|suffix| short.strip_suffix(suffix).filter(|rest| !rest.is_empty()))
        .unwrap_or(short);
    crate::packages::livewire::scan::kebab(base)
}

// ------------------------------------------------------------------------------------------------ form / table

/// The fields of the `form()` body.
fn form_fields(
    clean: &str,
    mask: &str,
    imports: &[(String, String)],
    lines: &LineIndex,
    body: (usize, usize),
    dynamic: &mut Vec<(&'static str, u32)>,
) -> Vec<Field> {
    let comps = comps_in(clean, mask, imports, lines, body.0, body.1);
    let parents: Vec<Option<(usize, String)>> =
        (0..comps.len()).map(|i| parent_of(&comps, i)).collect();
    let mut out = Vec::new();
    for (i, comp) in comps.iter().enumerate() {
        let Some(boundary) = placement(&comps, &parents, i) else {
            continue;
        };
        if CONTAINERS.contains(&comp.short.as_str()) || NON_FIELDS.contains(&comp.short.as_str()) {
            continue;
        }
        match &comp.name {
            NameArg::Lit(name) => {
                let relation = relation_arg(clean, comp, name);
                let dehydrated_off = comp
                    .call("dehydrated")
                    .is_some_and(|(a, b)| clean[a..b].trim() == "false");
                let check = !boundary
                    && relation.is_none()
                    && !name.contains('.')
                    && !comp.has("statePath")
                    && !dehydrated_off;
                out.push(Field {
                    name: name.clone(),
                    line: comp.line,
                    required: comp.flag(clean, "required"),
                    relation: relation.flatten().filter(|_| !boundary),
                    check,
                });
            }
            NameArg::Dynamic => dynamic.push(("fi-field", comp.line)),
            NameArg::Missing => {}
        }
    }
    out
}

/// The table sections: the chain calls whose arguments hold the columns / filters / actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    Columns,
    Filters,
    Actions,
    Bulk,
}

/// The sections of the `table()` body with the byte range of their arguments.
fn table_sections(clean: &str, mask: &str, body: (usize, usize)) -> Vec<(Section, (usize, usize))> {
    let mut out = Vec::new();
    let Some(region) = mask.get(body.0..body.1) else {
        return out;
    };
    for caps in re!(
        r"->\s*(columns|filters|actions|recordActions|headerActions|bulkActions|toolbarActions|groupedBulkActions)\s*\("
    )
    .captures_iter(region)
    {
        let (Some(whole), Some(call)) = (caps.get(0), caps.get(1)) else {
            continue;
        };
        let open = body.0 + whole.end() - 1;
        let Some(close) = matching(clean, open, body.1, Dialect::Php) else {
            continue;
        };
        let section = match call.as_str() {
            "columns" => Section::Columns,
            "filters" => Section::Filters,
            "actions" | "recordActions" | "headerActions" => Section::Actions,
            _ => Section::Bulk,
        };
        out.push((section, (open + 1, close)));
    }
    out
}

/// Fills the table facts of the resource from the `table()` body.
fn read_table(
    resource: &mut Resource,
    clean: &str,
    mask: &str,
    imports: &[(String, String)],
    lines: &LineIndex,
    body: (usize, usize),
) {
    let sections = table_sections(clean, mask, body);
    let comps = comps_in(clean, mask, imports, lines, body.0, body.1);
    let parents: Vec<Option<(usize, String)>> =
        (0..comps.len()).map(|i| parent_of(&comps, i)).collect();
    resource.own_query = clean[body.0..body.1].contains("->query(");
    for (i, comp) in comps.iter().enumerate() {
        if placement(&comps, &parents, i).is_none() || CONTAINERS.contains(&comp.short.as_str()) {
            continue;
        }
        let Some(&(section, _)) = sections
            .iter()
            .find(|(_, (a, b))| *a <= comp.start && comp.start < *b)
        else {
            continue;
        };
        match section {
            Section::Columns => match &comp.name {
                NameArg::Lit(name) => {
                    let virtual_column = comp.has("state")
                        || comp.has("getStateUsing")
                        || comp.has("statePath")
                        || comp.short == "ViewColumn";
                    resource.columns.push(Col {
                        name: name.clone(),
                        line: comp.line,
                        check: !virtual_column,
                    });
                }
                NameArg::Dynamic => resource.dynamic.push(("fi-column", comp.line)),
                NameArg::Missing => {}
            },
            Section::Filters => match &comp.name {
                NameArg::Lit(name) => resource.filters.push(name.clone()),
                NameArg::Missing => resource
                    .filters
                    .push(derived_name(&comp.short, &["Filter"])),
                NameArg::Dynamic => resource.dynamic.push(("fi-filter", comp.line)),
            },
            Section::Actions | Section::Bulk => {
                let name = match &comp.name {
                    NameArg::Lit(name) => name.clone(),
                    NameArg::Missing if comp.short != "Action" && comp.short != "BulkAction" => {
                        derived_name(&comp.short, &["BulkAction", "Action"])
                    }
                    NameArg::Missing => continue,
                    NameArg::Dynamic => {
                        resource.dynamic.push(("fi-action", comp.line));
                        continue;
                    }
                };
                if section == Section::Actions {
                    resource.actions.push(name);
                } else {
                    resource.bulk.push(name);
                }
            }
        }
    }
}

// ------------------------------------------------------------------------------------------------ classes

/// The class of a file: name, parent (short), the byte offset of the declaration.
fn class_decl(mask: &str) -> Option<(String, String, usize)> {
    let decl = re!(
        r"(?m)^[ \t]*(?:(?:final|abstract|readonly)\s+)*class\s+(?P<name>\w+)(?:\s+extends\s+(?P<parent>\\?[\w\\]+))?"
    )
    .captures(mask)?;
    let name = decl.name("name")?.as_str().to_owned();
    let parent = short_of(decl.name("parent").map_or("", |m| m.as_str())).to_owned();
    Some((name, parent, decl.get(0)?.start()))
}

/// The resource's `getPages()` entries.
fn read_pages(resource: &mut Resource, clean: &str, lines: &LineIndex, body: (usize, usize)) {
    // the keys are string literals: the comment-free copy, not the masked one
    let Some(region) = clean.get(body.0..body.1) else {
        return;
    };
    for caps in re!(r#"['"]([\w.-]+)['"]\s*=>\s*(\\?[\w\\]+)::route\s*\("#).captures_iter(region) {
        let (Some(whole), Some(key), Some(class)) = (caps.get(0), caps.get(1), caps.get(2)) else {
            continue;
        };
        let open = body.0 + whole.end() - 1;
        let Some(close) = matching(clean, open, body.1, Dialect::Php) else {
            continue;
        };
        let line_at = body.0 + whole.start();
        let args = split_args(clean, open + 1, close, Dialect::Php);
        let path = args
            .first()
            .and_then(|&(a, b)| single_literal(clean[a..b].trim()));
        match path {
            Some(path) => resource.pages.push(PageRef {
                key: key.as_str().to_owned(),
                class: short_of(class.as_str()).to_owned(),
                path,
            }),
            None => resource.dynamic.push(("fi-page", lines.line_of(line_at))),
        }
    }
}

/// The resource class of a file.
fn read_resource(
    rel: &str,
    text: &str,
    clean: &str,
    mask: &str,
    name: String,
    at: usize,
) -> Resource {
    let imports = imports_of(clean);
    let lines = LineIndex::new(text);
    let mut out = Resource {
        rel: rel.to_owned(),
        name,
        line: lines.line_of(at),
        slug: string_prop(clean, "slug"),
        ..Resource::default()
    };
    if let Some(caps) = re!(r"\$model\s*=\s*(\\?[\w\\]+)::class").captures(clean)
        && let Some(model) = caps.get(1)
    {
        out.model = Some(short_of(model.as_str()).to_owned());
        out.model_line = lines.line_of(model.start());
    }
    if let Some(body) = method_body(mask, "getPages") {
        read_pages(&mut out, clean, &lines, body);
    }
    if let Some(body) = method_body(mask, "form") {
        let mut dynamic = Vec::new();
        out.form = form_fields(clean, mask, &imports, &lines, body, &mut dynamic);
        out.dynamic.extend(dynamic);
    }
    if let Some(body) = method_body(mask, "table") {
        read_table(&mut out, clean, mask, &imports, &lines, body);
    }
    if let Some((a, b)) = method_body(mask, "getRelations") {
        for caps in re!(r"(\\?[\w\\]+)::class").captures_iter(&mask[a..b]) {
            if let Some(class) = caps.get(1) {
                out.relations.push(short_of(class.as_str()).to_owned());
            }
        }
    }
    out
}

/// The panel provider: id, path, resource classes, discovery directories.
fn read_panel(rel: &str, text: &str, clean: &str, mask: &str, at: usize) -> Panel {
    let lines = LineIndex::new(text);
    let mut out = Panel {
        rel: rel.to_owned(),
        line: lines.line_of(at),
        ..Panel::default()
    };
    let Some((a, b)) = method_body(mask, "panel") else {
        return out;
    };
    let body = &clean[a..b];
    let line_of = |offset: usize| lines.line_of(a + offset);
    for caps in re!(r"->\s*(id|path)\s*\(").captures_iter(body) {
        let (Some(whole), Some(call)) = (caps.get(0), caps.get(1)) else {
            continue;
        };
        let open = a + whole.end() - 1;
        let Some(close) = matching(clean, open, b, Dialect::Php) else {
            continue;
        };
        let first = split_args(clean, open + 1, close, Dialect::Php)
            .first()
            .map(|&(x, y)| clean[x..y].trim());
        let value = match first {
            Some("''" | "\"\"") => Some(String::new()),
            Some(text) => single_literal(text),
            None => None,
        };
        match (call.as_str(), value) {
            ("id", value) => out.id = value,
            (_, Some(value)) => out.path = Some(value),
            _ => out.dynamic.push(("fi-panel", line_of(whole.start()))),
        }
    }
    for caps in re!(r"->\s*(resources|discoverResources)\s*\(").captures_iter(body) {
        let (Some(whole), Some(call)) = (caps.get(0), caps.get(1)) else {
            continue;
        };
        let open = a + whole.end() - 1;
        let Some(close) = matching(clean, open, b, Dialect::Php) else {
            continue;
        };
        let args = &clean[open + 1..close];
        if call.as_str() == "resources" {
            for class in re!(r"(\\?[\w\\]+)::class").captures_iter(args) {
                if let Some(class) = class.get(1) {
                    out.resources.push(short_of(class.as_str()).to_owned());
                }
            }
        } else if let Some(dir) = re!(r#"app_path\(\s*['"]([^'"]*)['"]"#)
            .captures(args)
            .map(|caps| format!("app/{}", &caps[1]))
            .or_else(|| {
                re!(r#"base_path\(\s*['"]([^'"]*)['"]"#)
                    .captures(args)
                    .map(|caps| caps[1].to_owned())
            })
        {
            out.discover.push(dir.trim_matches('/').to_owned());
        } else {
            out.dynamic.push(("fi-panel", line_of(whole.start())));
        }
    }
    out
}

/// The kind of a widget by its parent class.
fn widget_kind(parent: &str) -> Option<WidgetKind> {
    match parent {
        "StatsOverviewWidget" => Some(WidgetKind::Stats),
        "ChartWidget"
        | "LineChartWidget"
        | "BarChartWidget"
        | "DoughnutChartWidget"
        | "PieChartWidget"
        | "RadarChartWidget"
        | "PolarAreaChartWidget"
        | "BubbleChartWidget"
        | "ScatterChartWidget" => Some(WidgetKind::Chart),
        "TableWidget" => Some(WidgetKind::Table),
        "Widget" => Some(WidgetKind::Other),
        _ => None,
    }
}

/// The Filament class of a file; `None`: the file has none.
pub(super) fn scan_file(rel: &str, text: &str) -> Option<Found> {
    if !text.contains("Filament") {
        return None;
    }
    let clean = strip_comments(text, Dialect::Php);
    let mask = mask_strings(&clean);
    let (name, parent, at) = class_decl(&mask)?;
    // `use Filament\Widgets\StatsOverviewWidget as BaseWidget;`: the parent by its real name
    let parent =
        resolve(&imports_of(&clean), &parent).map_or(parent, |full| short_of(&full).to_owned());
    let lines = LineIndex::new(text);
    match parent.as_str() {
        "Resource" if clean.contains("Filament\\Resources") => Some(Found::Resource(Box::new(
            read_resource(rel, text, &clean, &mask, name, at),
        ))),
        "RelationManager" if clean.contains("Filament\\") => Some(Found::Manager(Manager {
            rel: rel.to_owned(),
            name,
            line: lines.line_of(at),
            relationship: string_prop(&clean, "relationship"),
        })),
        "PanelProvider" => Some(Found::Panel(Box::new(read_panel(
            rel, text, &clean, &mask, at,
        )))),
        "Page" | "Dashboard" if clean.contains("Filament\\Pages") => Some(Found::Page(PageClass {
            rel: rel.to_owned(),
            name,
            slug: string_prop(&clean, "slug"),
            view: string_prop(&clean, "view"),
        })),
        other if clean.contains("Filament\\Widgets") => {
            let kind = widget_kind(other)?;
            let mut idents: Vec<String> = Vec::new();
            for caps in re!(r"\b([A-Z][A-Za-z0-9_]*)::").captures_iter(&mask) {
                if let Some(ident) = caps.get(1)
                    && !idents.iter().any(|seen| seen == ident.as_str())
                {
                    idents.push(ident.as_str().to_owned());
                }
            }
            Some(Found::Widget(Widget {
                rel: rel.to_owned(),
                name,
                kind,
                idents,
            }))
        }
        _ => None,
    }
}

/// The method names of a model file (lowercase, without `_`) and whether its members are not all visible (a trait
/// that may add relations / accessors, `__call`, `__get`).
pub(super) fn model_members(text: &str) -> (Vec<String>, bool) {
    const PLAIN_TRAITS: [&str; 9] = [
        "HasFactory",
        "Notifiable",
        "SoftDeletes",
        "HasApiTokens",
        "Prunable",
        "MassPrunable",
        "HasUuids",
        "HasUlids",
        "TwoFactorAuthenticatable",
    ];
    let clean = strip_comments(text, Dialect::Php);
    let mask = mask_strings(&clean);
    let methods: Vec<String> = re!(r"function\s+(\w+)\s*\(")
        .captures_iter(&mask)
        .filter_map(|caps| caps.get(1))
        .map(|m| m.as_str().replace('_', "").to_lowercase())
        .collect();
    let mut opaque = methods
        .iter()
        .any(|m| m == "call" || m == "get" || m == "callstatic");
    if let Some((_, _, at)) = class_decl(&mask)
        && let Some(open) = mask[at..].find('{')
    {
        for caps in re!(r"(?m)^[ \t]+use\s+([^;{(]+)[;{]").captures_iter(&mask[at + open..]) {
            let Some(list) = caps.get(1) else {
                continue;
            };
            opaque |= list
                .as_str()
                .split(',')
                .map(|name| short_of(name.trim()))
                .any(|name| !PLAIN_TRAITS.contains(&name));
        }
    }
    (methods, opaque)
}
