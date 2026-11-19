//! Livewire (`--format livewire`, the outline header lines, `trace livewire:x`): the component class, its view and
//! the places that use it, and the contract between the template and the class.
//!
//! - class <-> view: `render()` returning `view('livewire.x')`, or the default
//!   `resources/views/livewire/<kebab>.blade.php` when there is no `render()`; uses: `<livewire:x>`,
//!   `<livewire:dir.x>`, `@livewire('x')`, `Route::get('/x', X::class)` (full-page components);
//! - template <-> class: `wire:model[.live]="prop"` against the public properties (and the `Livewire\Form` object
//!   behind `form.title`), `wire:click|submit|...="method(args)"` against the public methods;
//! - events: `$this->dispatch('x')`, `$dispatch('x')`, `wire:click="$dispatch('x')"` against `#[On('x')]` and the
//!   Livewire 2 `$listeners`; one key `livewire::event:x` in the graph, so `--grep` finds both sides;
//! - validation: `#[Validate('required|min:3')]`, `rules()`, `$rules`, `$this->validate([...])`.
//!
//! Everything is read from the source text (`scan.rs`) once per query, only when the view asks for it. It is
//! conservative: a finding needs a statically known name and a class whose members are all visible (no traits,
//! no `__call`); the magic actions (`$refresh`, `$set`, `$toggle`, `$dispatch`, `$parent...`) are never checked, and
//! a name built at run time is counted in the footer, never reported. Volt (anonymous classes inside a Blade file) is
//! not covered.

mod events;
mod render;
pub(crate) mod scan;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rayon::prelude::*;

use crate::io::FileWalker;
use crate::model::FileGraph;
use crate::query::QueryResult;
use scan::{Class, RouteUse, Target, ViewRef, ViewScan};

pub(crate) use events::{blade_event_calls, php_event_calls};

/// Files bigger than this are not read.
const MAX_BYTES: u64 = 1_500_000;
/// The outline gets Livewire header lines only for this few files (a directory outline does not).
const MAX_OUTLINE_ROWS: usize = 20;

/// The project's Livewire components and everything around them.
#[derive(Debug, Default)]
pub(crate) struct Livewire {
    root: PathBuf,
    base: PathBuf,
    /// Component and form classes, by file.
    classes: Vec<Class>,
    /// Every Blade template (project-relative path) with what it says about Livewire.
    views: BTreeMap<String, ViewScan>,
    routes: Vec<RouteUse>,
    /// The files of the queried rows.
    scope: HashSet<String>,
}

/// What one scanned file is.
enum Scanned {
    Class(Box<Class>),
    View(String, Box<ViewScan>),
    Routes(Vec<RouteUse>),
}

impl Livewire {
    /// Scan the project below `root`; empty when the project has no component directory.
    pub(crate) fn open(root: &Path, base: &Path) -> Self {
        let mut out = Self {
            root: root.to_path_buf(),
            base: base.to_path_buf(),
            ..Self::default()
        };
        if !root.join("app/Livewire").is_dir() && !root.join("app/Http/Livewire").is_dir() {
            return out;
        }
        let walker = FileWalker::for_extensions(vec!["php"]);
        let files: Vec<(String, PathBuf)> = walker
            .collect(&[root.to_path_buf()], &mut |_| {})
            .into_iter()
            .filter_map(|path| Some((relative(&path, root, base)?, path)))
            .collect();
        let scanned: Vec<Scanned> = files
            .par_iter()
            .filter_map(|(rel, path)| scan_file(rel, path))
            .collect();
        for item in scanned {
            match item {
                Scanned::Class(class) => out.classes.push(*class),
                Scanned::View(rel, view) => {
                    out.views.insert(rel, *view);
                }
                Scanned::Routes(routes) => out.routes.extend(routes),
            }
        }
        out.classes.sort_by(|a, b| a.rel.cmp(&b.rel));
        out.routes
            .sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));
        out
    }

    /// No component class was found.
    pub(crate) fn is_empty(&self) -> bool {
        self.classes.iter().all(|class| class.form)
    }

    /// The project-relative path of a graph file.
    fn rel(&self, file: &str) -> Option<String> {
        relative(Path::new(file), &self.root, &self.base)
    }

    /// The component classes (forms are no components).
    fn components(&self) -> impl Iterator<Item = usize> + '_ {
        self.classes
            .iter()
            .enumerate()
            .filter(|(_, class)| !class.form)
            .map(|(index, _)| index)
    }

    /// The template of a component: the file exists among the scanned templates.
    fn view_rel(&self, class: &Class) -> Option<String> {
        let rel = match &class.view {
            ViewRef::Named(name) => format!("resources/views/{}.blade.php", name.replace('.', "/")),
            ViewRef::Convention => format!(
                "resources/views/livewire/{}.blade.php",
                class.comp_name.replace('.', "/")
            ),
            ViewRef::Dynamic => return None,
        };
        self.views.contains_key(&rel).then_some(rel)
    }

    /// The component a tag / directive names.
    fn resolve(&self, target: &Target) -> Option<usize> {
        match target {
            Target::Name(name) => {
                let key = name
                    .split('.')
                    .map(scan::kebab)
                    .collect::<Vec<_>>()
                    .join(".");
                self.components()
                    .find(|&index| self.classes[index].comp_name == key)
            }
            Target::Class(class) => self.by_class(class),
        }
    }

    /// The component of a class written in full or by its short name (unique).
    fn by_class(&self, written: &str) -> Option<usize> {
        let written = written.trim_start_matches('\\');
        if written.contains('\\') {
            return self
                .components()
                .find(|&index| self.classes[index].fqn == written);
        }
        let mut hits = self
            .components()
            .filter(|&index| self.classes[index].name == written);
        let first = hits.next()?;
        hits.next().is_none().then_some(first)
    }

    /// The `Livewire\Form` class a property type names.
    fn form_of(&self, ty: &str) -> Option<&Class> {
        let short = ty.rsplit('\\').next().unwrap_or(ty);
        let mut hits = self
            .classes
            .iter()
            .filter(|class| class.form && class.name == short);
        let first = hits.next()?;
        hits.next().is_none().then_some(first)
    }

    /// `(file, line)` of every use of the component: template tags / directives and routes.
    fn uses_of(&self, index: usize) -> Vec<(String, u32)> {
        let mut out: Vec<(String, u32)> = Vec::new();
        for (file, view) in &self.views {
            for item in &view.uses {
                if self.resolve(&item.target) == Some(index) {
                    out.push((file.clone(), item.line));
                }
            }
        }
        for route in &self.routes {
            if self.by_class(&route.class) == Some(index) {
                out.push((route.file.clone(), route.line));
            }
        }
        out.sort();
        out.dedup();
        out
    }

    /// The routes that name the component as their handler.
    fn routes_of(&self, index: usize) -> Vec<&RouteUse> {
        self.routes
            .iter()
            .filter(|route| self.by_class(&route.class) == Some(index))
            .collect()
    }
}

/// Read and scan one file.
fn scan_file(rel: &str, path: &Path) -> Option<Scanned> {
    if std::fs::metadata(path).ok()?.len() > MAX_BYTES {
        return None;
    }
    let text = std::fs::read_to_string(path).ok()?;
    if rel.ends_with(".blade.php") {
        return Some(Scanned::View(
            rel.to_owned(),
            Box::new(scan::scan_view(&text)),
        ));
    }
    if rel.starts_with("routes/") {
        let routes = scan::scan_routes(rel, &text);
        return (!routes.is_empty()).then_some(Scanned::Routes(routes));
    }
    scan::scan_class(rel, &text).map(|class| Scanned::Class(Box::new(class)))
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

/// Could the file be a component class or a template of one (the outline only loads the scan for those).
fn is_livewire_file(graph: &FileGraph) -> bool {
    graph.file.ends_with(".blade.php")
        || (graph.file.ends_with(".php")
            && (graph.file.contains("Livewire")
                || graph.containers.iter().any(|container| {
                    container
                        .extends
                        .iter()
                        .flatten()
                        .any(|parent| parent.contains("Livewire"))
                })))
}

/// The rows with the Livewire facts, when the output asks for them: the `livewire` view, and the outline of a few
/// component classes / templates.
pub(crate) fn attach(rows: &mut [QueryResult], root: &Path, format: &str) {
    if rows.is_empty() {
        return;
    }
    let wanted = match format {
        "livewire" => true,
        "outline" => {
            rows.len() <= MAX_OUTLINE_ROWS && rows.iter().any(|row| is_livewire_file(&row.graph))
        }
        _ => false,
    };
    if !wanted {
        return;
    }
    let base = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let mut found = Livewire::open(root, &base);
    if found.is_empty() {
        return;
    }
    found.scope = rows
        .iter()
        .filter_map(|row| relative(Path::new(&row.graph.file), root, &base))
        .collect();
    let shared = Arc::new(found);
    for row in rows {
        row.livewire = Some(Arc::clone(&shared));
    }
}
