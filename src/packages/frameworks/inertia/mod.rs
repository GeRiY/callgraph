//! Inertia (Laravel + Vue / React / Svelte): a PHP render-hívás és a page-komponens kötése. Facade: `Inertia` — a
//! page-gyökér (`pages.rs`), a page-mappa fájljai, és ha a kérés igényli, a projekt render-helyei (`render.rs`) és
//! az oldalak deklarált props-ai. A kimenetek: az outline / json render-hívásának page-fájlja, a page outline-jának
//! `rendered by:` sora, az `inertia` nézet (`view.rs`) és a `locate` page-indoklása.

mod pages;
pub(super) mod render;
mod shared;
mod view;

#[cfg(test)]
mod tests;

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use super::files::ProjectFiles;
use crate::model::{Call, FileGraph, FrameworkFacts};
use crate::packages::Packages;

pub(crate) use pages::PageRoot;
pub(crate) use render::{RenderSite, facts_of};

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
    /// Az oldalak kötelező props-ai (a `defineProps` nem opcionális, alapérték nélküli tagjai; csak a nem üres).
    required: HashMap<String, Vec<String>>,
    /// A `HandleInertiaRequests::share` kulcsai: minden oldalnak átadottak.
    shared: SharedProps,
}

/// A megosztott props: a kulcsok és hogy a `share` végig statikusan olvasható volt-e.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SharedProps {
    keys: Vec<String>,
    complete: bool,
}

impl Default for SharedProps {
    fn default() -> Self {
        Self {
            keys: Vec::new(),
            complete: true,
        }
    }
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
            required: HashMap::new(),
            shared: SharedProps::default(),
        }
    }

    /// A projekt render-helyei és az oldalak props-ai a projekt (kód)fájljaiból: a page-fájloknál a property-k (a
    /// `defineProps` / `props:`), a PHP-fájloknál a render-helyek — az elemzéskor kinyert tényekből (`FileGraph::facts`);
    /// tény nélküli gráfnál (régi cache, kézzel épített gráf) az argumentumaik a forrásból (`files`), mert a szűkített
    /// gráfban nincs hívás-argumentum. A `CALLGRAPH_FACTS_DIFF=1` mindkét utat futtatja, és az eltérést a stderr-re írja.
    pub(crate) fn scan<'g>(
        &mut self,
        graphs: impl IntoIterator<Item = (&'g Path, &'g FileGraph)>,
        files: &dyn ProjectFiles,
    ) {
        let compare = std::env::var_os("CALLGRAPH_FACTS_DIFF").is_some();
        for (path, graph) in graphs {
            let Some(rel) = self.relative_path(path) else {
                continue;
            };
            if self.pages.page_name(&rel).is_some() {
                self.add_page(rel, graph);
                continue;
            }
            if !rel.ends_with(".php") {
                continue;
            }
            match &graph.facts {
                Some(facts) => {
                    self.add_facts(&rel, facts);
                    if compare {
                        let text = files.read(path);
                        report_diff(&rel, graph, facts, text.as_deref());
                    }
                }
                None => {
                    let renders = graph
                        .calls
                        .iter()
                        .any(|call| render::RenderKind::of(call).is_some());
                    let shares = graph
                        .methods
                        .iter()
                        .any(|method| method.name.eq_ignore_ascii_case("share"));
                    if renders || shares {
                        let text = files.read(path);
                        self.add_text(&rel, graph, text.as_deref());
                    }
                }
            }
        }
    }

    /// Egy page-fájl deklarált és kötelező props-ai.
    fn add_page(&mut self, rel: String, graph: &FileGraph) {
        let props: Vec<String> = graph.properties.iter().map(|p| p.name.clone()).collect();
        if props.is_empty() {
            return;
        }
        let required: Vec<String> = graph
            .properties
            .iter()
            .filter(|p| p.js && p.readonly)
            .map(|p| p.name.clone())
            .collect();
        if !required.is_empty() {
            self.required.insert(rel.clone(), required);
        }
        self.declared.insert(rel, props);
    }

    /// Egy PHP-fájl tényei (az elemzés kinyerte: nincs forrásolvasás).
    fn add_facts(&mut self, rel: &str, facts: &FrameworkFacts) {
        self.sites.extend(render::sites_of_facts(rel, facts));
        self.dynamic.extend(
            facts
                .dynamic_lines
                .iter()
                .map(|line| (rel.to_owned(), *line)),
        );
        if let Some(shared) = &facts.shared {
            self.add_shared(&shared.keys, shared.complete);
        }
    }

    /// Egy PHP-fájl render-helyei a forrásból (a régi, szöveg-olvasó út: tény nélküli gráfnál).
    fn add_text(&mut self, rel: &str, graph: &FileGraph, text: Option<&str>) {
        let found = render::sites_in(rel, graph, text);
        self.dynamic.extend(
            render::dynamic_lines(graph, &found)
                .into_iter()
                .map(|line| (rel.to_owned(), line)),
        );
        self.sites.extend(found);
        if let Some(shared) = text.and_then(|text| shared::shared_in(graph, text)) {
            self.add_shared(&shared.keys, shared.complete);
        }
    }

    fn add_shared(&mut self, keys: &[String], complete: bool) {
        for key in keys {
            if !self.shared.keys.contains(key) {
                self.shared.keys.push(key.clone());
            }
        }
        self.shared.complete &= complete;
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

    /// További render-helyek (a konfigból generált route-ok Inertia-komponensei): a már ismert (oldal, fájl, sor)
    /// hármas nem kerül be kétszer.
    pub(crate) fn add_sites(&mut self, sites: Vec<RenderSite>) {
        for site in sites {
            let known = self.sites.iter().any(|have| {
                have.page == site.page && have.file == site.file && have.line == site.line
            });
            if !known {
                self.sites.push(site);
            }
        }
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
        // a gyökér levágása az abszolútság vizsgálata előtt: Windows-on a `/p/x` nem abszolút (nincs meghajtó), de
        // ugyanúgy a gyökér alatt van
        let stripped = path
            .strip_prefix(&self.root)
            .ok()
            .or_else(|| path.strip_prefix(self.canonical.as_ref()?).ok());
        let rel = match stripped {
            Some(rel) => rel,
            None if path.is_absolute() => return None,
            None => path,
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

    /// Every page file in its short form (`Pages/Board.vue`), sorted.
    pub(crate) fn page_shorts(&self) -> Vec<String> {
        self.files.iter().map(|rel| self.pages.short(rel)).collect()
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

/// A tény-alapú és a szöveg-olvasó út összevetése (`CALLGRAPH_FACTS_DIFF=1`): eltérésnél egy sor a stderr-re.
fn report_diff(rel: &str, graph: &FileGraph, facts: &FrameworkFacts, text: Option<&str>) {
    let Some(text) = text else { return };
    let other = text_facts(rel, graph, text);
    if other != *facts {
        eprintln!("facts diff: {rel}: {facts:?} != {other:?}");
    }
}

/// A régi szöveg-olvasó út eredménye tényként (az A / B összevetéshez).
pub(crate) fn text_facts(rel: &str, graph: &FileGraph, text: &str) -> FrameworkFacts {
    let found = render::sites_in(rel, graph, Some(text));
    FrameworkFacts {
        dynamic_lines: render::dynamic_lines(graph, &found),
        render_sites: found.into_iter().map(render::fact_of_site).collect(),
        shared: shared::shared_in(graph, text),
    }
}
