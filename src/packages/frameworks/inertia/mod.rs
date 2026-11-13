//! Inertia (Laravel + Vue / React / Svelte): a PHP render-hívás és a page-komponens kötése. Facade: `Inertia` — a
//! page-gyökér (`pages.rs`), a page-mappa fájljai, és ha a kérés igényli, a projekt render-helyei (`render.rs`) és
//! az oldalak deklarált props-ai. A kimenetek: az outline / json render-hívásának page-fájlja, a page outline-jának
//! `rendered by:` sora, az `inertia` nézet (`view.rs`) és a `locate` page-indoklása.

mod pages;
pub(super) mod render;
mod view;

#[cfg(test)]
mod tests;

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use super::files::ProjectFiles;
use crate::model::{Call, FileGraph};
use crate::packages::Packages;

pub(crate) use pages::PageRoot;
pub(crate) use render::RenderSite;

/// A framework neve a `FRAMEWORKS` regiszterben: ez kapcsolja be a stratégiát.
pub(crate) const NAME: &str = "inertia";

/// Egy render-hívás page-fájlja: a gyökérhez relatív út (`resources/js/Pages/Board.vue`), a kiírt rövid alak
/// (`Pages/Board.vue`), és hogy a fájl létezik-e.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PageLink {
    pub rel: String,
    pub short: String,
    pub exists: bool,
}

/// Az Inertia-kötések egy projektre.
#[derive(Debug, Clone)]
pub(crate) struct Inertia {
    /// A projekt gyökere, és a kanonikus alakja (a sorok útja bármelyikkel kezdődhet).
    root: PathBuf,
    canonical: Option<PathBuf>,
    pages: PageRoot,
    /// A page-gyökér alatti oldal-fájlok (gyökérhez relatív, `/`-rel).
    files: BTreeSet<String>,
    /// A projekt render-helyei (csak a `scan` után; előtte üres).
    sites: Vec<RenderSite>,
    /// Az oldalak deklarált props-ai (gyökérhez relatív út → nevek; csak a nem üres, a `scan` után).
    declared: HashMap<String, Vec<String>>,
    /// A dinamikus (nem literál page-nevű) render-helyek: (fájl, sor) — ilyenkor a „nem renderelt” lista nem biztos.
    dynamic: Vec<(String, u32)>,
}

impl Inertia {
    /// Csak ha a projekt Inertia-t használ (a `Packages::frameworks()` szerint) és a page-gyökér felismerhető.
    pub(crate) fn detect(
        root: &Path,
        packages: &Packages,
        files: &dyn ProjectFiles,
    ) -> Option<Self> {
        if !packages.frameworks().contains(&NAME) {
            return None;
        }
        let pages = pages::detect(root, files)?;
        let mut inertia = Self::new(root, pages, Vec::new());
        let listed: BTreeSet<String> = files
            .files_under(&root.join(&inertia.pages.dir))
            .iter()
            .filter_map(|path| inertia.relative_path(path))
            .filter(|rel| inertia.pages.page_name(rel).is_some())
            .collect();
        inertia.files = listed;
        Some(inertia)
    }

    /// Kész részekből (`files`: a page-fájlok a gyökérhez relatívan).
    pub(crate) fn new(root: &Path, pages: PageRoot, files: Vec<String>) -> Self {
        Self {
            root: root.to_path_buf(),
            canonical: root.canonicalize().ok().filter(|c| c != root),
            pages,
            files: files.into_iter().collect(),
            sites: Vec::new(),
            declared: HashMap::new(),
            dynamic: Vec::new(),
        }
    }

    /// A projekt render-helyei és az oldalak props-ai a projekt (kód)fájljaiból: a page-fájloknál a property-k (a
    /// `defineProps` / `props:`), a PHP-fájloknál a render-hívások — az argumentumaik a forrásból (`files`), mert a
    /// szűkített tényekben nincs argumentum.
    pub(crate) fn scan<'g>(
        &mut self,
        graphs: impl IntoIterator<Item = (&'g Path, &'g FileGraph)>,
        files: &dyn ProjectFiles,
    ) {
        for (path, graph) in graphs {
            let Some(rel) = self.relative_path(path) else {
                continue;
            };
            if self.pages.page_name(&rel).is_some() {
                let props: Vec<String> = graph.properties.iter().map(|p| p.name.clone()).collect();
                if !props.is_empty() {
                    self.declared.insert(rel, props);
                }
                continue;
            }
            let renders = rel.ends_with(".php")
                && graph
                    .calls
                    .iter()
                    .any(|call| render::RenderKind::of(call).is_some());
            if renders {
                let text = files.read(path);
                let found = render::sites_in(&rel, graph, text.as_deref());
                for call in &graph.calls {
                    let literal = found.iter().any(|site| site.line == call.line);
                    if render::RenderKind::of(call).is_some() && !literal {
                        self.dynamic.push((rel.clone(), call.line));
                    }
                }
                self.sites.extend(found);
            }
        }
    }

    /// Teszt-összeállítás (más modulok egységtesztjeihez): a page-gyökér (`dir`, az alapértelmezett
    /// kiterjesztésekkel), a létező page-fájlok (a gyökérhez relatívan) és a render-helyek
    /// (fájl, osztály, metódus, sor, page-név; statikus props nélkül).
    #[cfg(test)]
    pub(crate) fn fixture(
        root: &Path,
        dir: &str,
        files: &[&str],
        sites: &[(&str, &str, &str, u32, &str)],
    ) -> Self {
        let pages = PageRoot {
            dir: dir.to_owned(),
            extensions: ["vue", "tsx", "jsx", "svelte"]
                .iter()
                .map(|ext| (*ext).to_owned())
                .collect(),
        };
        let mut inertia = Self::new(root, pages, files.iter().map(|f| (*f).to_owned()).collect());
        inertia.sites = sites
            .iter()
            .map(|&(file, container, caller, line, page)| RenderSite {
                file: file.to_owned(),
                container: Some(container.to_owned()),
                caller: Some(caller.to_owned()),
                line,
                page: page.to_owned(),
                props: None,
            })
            .collect();
        inertia
    }

    /// A projekt render-helyei (a `scan` után).
    pub(crate) fn sites(&self) -> &[RenderSite] {
        &self.sites
    }

    /// A (sorokban álló, abszolút vagy relatív) fájl útja a gyökérhez relatívan, `/`-rel.
    pub(crate) fn rel(&self, file: &str) -> Option<String> {
        self.relative_path(Path::new(file))
    }

    fn relative_path(&self, path: &Path) -> Option<String> {
        let rel = if path.is_absolute() {
            path.strip_prefix(&self.root)
                .ok()
                .or_else(|| path.strip_prefix(self.canonical.as_ref()?).ok())?
        } else {
            path
        };
        let text = rel.to_string_lossy().replace('\\', "/");
        let mut text = text.as_str();
        while let Some(rest) = text.strip_prefix("./") {
            text = rest;
        }
        Some(text.to_owned())
    }

    /// A (sorokban álló) fájl page-neve, ha egy oldal-fájl: `Users/Index`.
    pub(crate) fn page_name(&self, file: &str) -> Option<String> {
        self.pages.page_name(&self.rel(file)?)
    }

    /// Oldal-fájl-e (a page-gyökér alatt, ismert kiterjesztéssel).
    pub(crate) fn is_page(&self, file: &str) -> bool {
        self.page_name(file).is_some()
    }

    /// A page-név fájlja: az első létező jelölt; ha egyik sem létezik, az első jelölt (`exists: false`).
    pub(crate) fn resolve(&self, page: &str) -> Option<PageLink> {
        let candidates = self.pages.candidates(page);
        let rel = candidates
            .iter()
            .find(|rel| self.files.contains(*rel))
            .or_else(|| candidates.first())?;
        Some(PageLink {
            short: self.pages.short(rel),
            exists: self.files.contains(rel),
            rel: rel.clone(),
        })
    }

    /// Egy (teljes gráfbeli) hívás page-fájlja, ha Inertia render-hívás string-literál page-névvel.
    pub(crate) fn link(&self, call: &Call) -> Option<PageLink> {
        self.resolve(&render::page_of_call(call)?)
    }

    /// Egy page render-helyei (a `scan` után).
    fn sites_of<'s>(&'s self, page: &'s str) -> impl Iterator<Item = &'s RenderSite> + 's {
        self.sites.iter().filter(move |site| site.page == page)
    }

    /// A page-fájl `rendered by:` sorának szövege: az első render-hely és a többi száma (`BoardController::show L23
    /// (+1)`); nem oldal-fájlnál vagy render-hely nélkül `None`.
    pub(crate) fn rendered_by(&self, file: &str) -> Option<String> {
        let page = self.page_name(file)?;
        let mut sites = self.sites_of(&page);
        let first = sites.next()?;
        let rest = sites.count();
        let mut text = first.label();
        if rest > 0 {
            text.push_str(&format!(" (+{rest})"));
        }
        Some(text)
    }
}
