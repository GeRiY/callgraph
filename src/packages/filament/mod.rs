//! Filament (`--format filament`, `trace filament:posts` / `trace PostResource`): the admin panel the project
//! declares in PHP — resources, their pages, form fields, table columns, filters, actions and relation managers,
//! standalone pages, widgets and the panel providers that register them.
//!
//! - resource <-> model: `$model = Post::class` (the table, the columns and the relations come from the Laravel
//!   models view); the graph key `filament::resource:Post` makes `--grep Post` find the resource from the model side;
//! - pages: `getPages()` (`'edit' => EditPost::route('/{record}/edit')`) under the panel path of the provider that
//!   registers the resource (`->resources([..])`, `->discoverResources(in: app_path(..))`, or the only panel);
//! - checks (conservative): a form field / table column name must be a column of the model table (migration), a
//!   fillable / cast / accessor or a relation (`author.name`: the relation, then the attribute on its target);
//!   `->relationship('author')` and `RelationManager::$relationship` must be relation methods of the model. Only when
//!   the model class and its migration were found, only static names, and never for `statePath`, `dehydrated(false)`,
//!   placeholders, view fields, `getStateUsing` columns, aggregate columns (`comments_count`), repeater items, models
//!   with unknown traits or `__call`;
//! - a name built at run time (`TextInput::make($name)`, `->path($x)`) is counted in the footer, never reported.
//!
//! Everything is read from the source text (`scan.rs`) once per query, only when the view asks for it.

mod keys;
mod render;
mod scan;
#[cfg(test)]
mod tests;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rayon::prelude::*;

use crate::io::FileWalker;
use crate::query::QueryResult;
use scan::{Found, Manager, PageClass, Panel, Resource, Widget};

pub(crate) use keys::php_calls;

/// Files bigger than this are not read.
const MAX_BYTES: u64 = 1_500_000;

/// What the Eloquent catalogue says about a model (`Laravel::model_info`).
#[derive(Debug, Clone, Default)]
pub(crate) struct ModelInfo {
    /// The short class name.
    pub class: String,
    pub table: String,
    /// The model file, project-relative.
    pub file: String,
    /// The columns of the table; `None`: no migration for it.
    pub columns: Option<Vec<String>>,
    pub fillable: Vec<String>,
    pub casts: Vec<String>,
    /// `(method, target class)` of the relation methods.
    pub relations: Vec<(String, Option<String>)>,
    /// The method names of the class (lowercase, without `_`); `None`: not read yet.
    pub members: Option<Vec<String>>,
    /// Members that are not all visible (an unknown trait, `__call`): nothing is reported against the model.
    pub opaque: bool,
}

impl ModelInfo {
    /// A column, a fillable / cast name, a relation or an accessor.
    pub(super) fn has_attr(&self, name: &str) -> bool {
        if matches!(name, "id" | "created_at" | "updated_at" | "deleted_at") {
            return true;
        }
        if self
            .columns
            .as_ref()
            .is_some_and(|columns| columns.iter().any(|column| column == name))
            || self.fillable.iter().chain(&self.casts).any(|n| n == name)
            || self.has_relation(name)
        {
            return true;
        }
        let norm = name.replace('_', "").to_lowercase();
        let getter = format!("get{norm}attribute");
        self.members
            .as_ref()
            .is_some_and(|members| members.iter().any(|m| *m == norm || *m == getter))
    }

    pub(super) fn has_relation(&self, name: &str) -> bool {
        self.relations.iter().any(|(method, _)| method == name)
    }
}

/// The model of a short class name (`Laravel::model_info`).
pub(crate) type ModelLookup<'a> = &'a dyn Fn(&str) -> Option<ModelInfo>;

/// The project's Filament resources and everything around them.
#[derive(Debug, Default)]
pub(crate) struct Filament {
    root: PathBuf,
    resources: Vec<Resource>,
    managers: Vec<Manager>,
    pages: Vec<PageClass>,
    widgets: Vec<Widget>,
    panels: Vec<Panel>,
    /// The files of the queried rows.
    scope: HashSet<String>,
}

impl Filament {
    /// Scan the project below `root`; empty when the project has no Filament directory.
    pub(crate) fn open(root: &Path, base: &Path) -> Self {
        let mut out = Self {
            root: root.to_path_buf(),
            ..Self::default()
        };
        if !root.join("app/Filament").is_dir() && !root.join("app/Providers/Filament").is_dir() {
            return out;
        }
        let walker = FileWalker::for_extensions(vec!["php"]);
        let files: Vec<(String, PathBuf)> = walker
            .collect(&[root.to_path_buf()], &mut |_| {})
            .into_iter()
            .filter_map(|path| Some((relative(&path, root, base)?, path)))
            .filter(|(rel, _)| !rel.ends_with(".blade.php") && !rel.starts_with("vendor/"))
            .collect();
        let found: Vec<Found> = files
            .par_iter()
            .filter_map(|(rel, path)| {
                if std::fs::metadata(path).ok()?.len() > MAX_BYTES {
                    return None;
                }
                scan::scan_file(rel, &std::fs::read_to_string(path).ok()?)
            })
            .collect();
        for item in found {
            out.add(item);
        }
        out.resources.sort_by(|a, b| a.rel.cmp(&b.rel));
        out.pages.sort_by(|a, b| a.rel.cmp(&b.rel));
        out.widgets.sort_by(|a, b| a.rel.cmp(&b.rel));
        out.panels.sort_by(|a, b| a.rel.cmp(&b.rel));
        out
    }

    fn add(&mut self, item: Found) {
        match item {
            Found::Resource(resource) => self.resources.push(*resource),
            Found::Manager(manager) => self.managers.push(manager),
            Found::Page(page) => self.pages.push(page),
            Found::Widget(widget) => self.widgets.push(widget),
            Found::Panel(panel) => self.panels.push(*panel),
        }
    }

    /// Nothing of Filament was found.
    pub(crate) fn is_empty(&self) -> bool {
        self.resources.is_empty()
            && self.pages.is_empty()
            && self.widgets.is_empty()
            && self.panels.is_empty()
    }
}

/// The project-relative, `/`-separated path.
fn relative(path: &Path, root: &Path, base: &Path) -> Option<String> {
    let stripped = path
        .strip_prefix(base)
        .ok()
        .or_else(|| path.strip_prefix(root).ok());
    let rel = match stripped {
        Some(rel) => rel,
        None if path.is_absolute() => return None,
        None => path,
    };
    let text = rel.to_string_lossy().replace('\\', "/");
    Some(text.trim_start_matches("./").to_owned())
}

/// The rows with the Filament facts, when the output asks for them (the `filament` view).
pub(crate) fn attach(rows: &mut [QueryResult], root: &Path, format: &str) {
    if rows.is_empty() || format != "filament" {
        return;
    }
    let base = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let mut found = Filament::open(root, &base);
    if found.is_empty() {
        return;
    }
    found.scope = rows
        .iter()
        .filter_map(|row| relative(Path::new(&row.graph.file), root, &base))
        .collect();
    let shared = Arc::new(found);
    for row in rows {
        row.filament = Some(Arc::clone(&shared));
    }
}
