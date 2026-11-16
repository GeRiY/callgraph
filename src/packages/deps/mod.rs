//! `deps` / MCP `project_profile` `deps: true`: a függőségek ritka, igény szerinti átvizsgálása (takarítás, audit, „cannot
//! find module” build-hiba). Ugyanaz a projekt-bejárás, mint a `uses`-nál (hívások ÉS importok, a feloldó lánccal), de
//! csomagonként és részenként (a fájlhoz legközelebbi manifest) összesítve:
//!
//! - **fölösleges?** — deklarált (prod vagy dev), de se hívás, se import nincs rá, és egyik kivétel sem illik
//!   (`rules.rs`: eszköz, framework-autoload, composer-plugin, csak CSS / asset, `deps-ignore`);
//! - **hiányzik** — a kód használja, de a legközelebbi rész manifestje nem deklarálja (JS: a csomag-import; PHP: csak
//!   a lockban szereplő, tranzitív csomag névtere), és nem beépített modul / a framework ökoszisztémája / saját
//!   workspace-csomag;
//! - **csak dev** — csak `require-dev` / `devDependencies`, de az alkalmazás-kód használja (tesztek, factory, seeder,
//!   `stubs/`, `*.config.*` kívül; bundlerrel épülő frontendnél nem jelez).
//!
//! A kimenet tömör: soronként egy találat, fajtánként csoportosítva, rövid indoklással; az üres fajta nem jelenik
//! meg; ha nincs találat, egy sor. Több csomag-mappánál a sor elején `[mappa]`.
//!
//! ```text
//! unused?   spatie/laravel-backup  no reference in code
//! missing   dayjs                  imported in resources/js/utils/date.ts (+1), not in package.json
//! dev-only  fakerphp/faker         used in app/Support/DemoData.php
//! ```

mod rules;

#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use super::Packages;
use super::model::{Ecosystem, Package};
use super::project::EcosystemPackages;
use super::resolve::{Resolution, SymbolRef, package_name};
use crate::i18n::Lang;
use crate::model::FileGraph;

use rules::{
    AnyOf, BuiltinModule, BundledFrontend, ComposerPlugin, CssOrAssetOnly, FrameworkAutoload,
    FrameworkBundled, NoNamespace, PackageRule, ToolingPackage, UserIgnore, WorkspacePackage,
};

/// Egy csomag használata egy csomag-részben.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Usage {
    /// A csomaghoz rendelt hívások száma.
    pub calls: usize,
    /// A csomaghoz rendelt importok (`use`, `import`) száma.
    pub refs: usize,
    /// `.css` / `.scss` `@import` / `@use` hivatkozások (nem számítanak kód-használatnak).
    pub css: usize,
    /// A hívó / importáló fájlok (a gyökérhez relatív, `/`-rel), rendezve.
    pub files: BTreeSet<String>,
}

impl Usage {
    /// Használja-e a kód (hívás vagy import).
    pub(crate) fn used(&self) -> bool {
        self.calls + self.refs > 0
    }

    /// A teszteken kívüli fájlok.
    fn files_outside_tests(&self) -> impl Iterator<Item = &str> {
        self.files
            .iter()
            .map(String::as_str)
            .filter(|f| !is_test_path(f))
    }

    /// Az alkalmazás-kód fájljai: a teszteken kívül, a fejlesztői segédfájlok (factory, seeder, `stubs/`,
    /// `*.config.*`) nélkül.
    fn app_files(&self) -> Vec<&str> {
        self.files_outside_tests()
            .filter(|f| !is_dev_support_path(f))
            .collect()
    }
}

/// A találat fajtája (a kimenet sorrendjében).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Unused,
    Missing,
    DevOnly,
}

impl Kind {
    fn label(self, lang: Lang) -> &'static str {
        match self {
            Self::Unused => lang.pick("unused?", "fölösleges?"),
            Self::Missing => lang.pick("missing", "hiányzik"),
            Self::DevOnly => lang.pick("dev-only", "csak dev"),
        }
    }
}

/// Egy találat: fajta, rész (index a csomagtérképben), csomagnév, indoklás.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Finding {
    kind: Kind,
    part: usize,
    name: String,
    reason: Reason,
}

/// Az indoklás adatai.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Reason {
    /// Se hívás, se import.
    NoReference,
    /// Használt, de nem deklarált: az első fájl, a további fájlok száma; `transitive`: csak a lockban van.
    Undeclared {
        ecosystem: Ecosystem,
        file: String,
        more: usize,
        transitive: bool,
    },
    /// Dev-függőség az alkalmazás-kódban: az első fájl, a további fájlok száma.
    DevInApp { file: String, more: usize },
}

/// Egy `deps` vizsgálat: a csomagtérkép, a gyökér (a relatív utakhoz) és a `deps-ignore` minták.
pub(crate) struct DepsQuery<'a> {
    packages: &'a Packages,
    root: &'a Path,
    ignore: UserIgnore,
}

/// A gyökér `.callgraphignore`-jának `deps-ignore` mintái (hiányzó / olvashatatlan fájl: üres).
pub(crate) fn user_ignores(root: &Path) -> Vec<String> {
    std::fs::read_to_string(root.join(crate::io::IGNORE_FILE))
        .map(|text| {
            crate::io::deps_ignore_patterns(&text)
                .into_iter()
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

impl<'a> DepsQuery<'a> {
    pub(crate) fn new(packages: &'a Packages, root: &'a Path, ignore: Vec<String>) -> Self {
        Self {
            packages,
            root,
            ignore: UserIgnore::new(ignore),
        }
    }

    /// Kell-e a projekt fájljait bejárni (manifest nélkül nem).
    pub(crate) fn needs_files(&self) -> bool {
        self.packages.has_parts()
    }

    /// A válasz szövege a projekt kódfájljaiból (abszolút út, gráf).
    pub(crate) fn render<'g>(
        &self,
        files: impl IntoIterator<Item = (&'g Path, &'g FileGraph)>,
        lang: Lang,
    ) -> String {
        if !self.packages.has_parts() {
            return lang
                .pick(
                    "deps: no composer.json / package.json in the project\n",
                    "függőségek: a projektben nincs composer.json / package.json\n",
                )
                .to_owned();
        }
        let usage = self.scan(files);
        let mut findings = self.unused(&usage);
        findings.extend(self.missing(&usage));
        findings.extend(self.dev_only(&usage));
        findings.sort_by(|a, b| {
            a.kind
                .cmp(&b.kind)
                .then(a.part.cmp(&b.part))
                .then_with(|| a.name.cmp(&b.name))
        });
        self.format(&findings, lang)
    }

    /// A fájlok hívásai, importjai és CSS-importjai csomagonként, a tulajdonos rész szerint.
    fn scan<'g>(
        &self,
        files: impl IntoIterator<Item = (&'g Path, &'g FileGraph)>,
    ) -> BTreeMap<(usize, String), Usage> {
        let mut found: BTreeMap<(usize, String), Usage> = BTreeMap::new();
        for (path, graph) in files {
            let (imports, _) = graph.imports.parts();
            if is_stylesheet(path) {
                for (_, spec) in imports {
                    let Some(name) = package_name(spec.trim_start_matches('~')) else {
                        continue;
                    };
                    if let Some(part) = self.owner(path, Ecosystem::Node, name) {
                        found.entry((part, name.to_owned())).or_default().css += 1;
                    }
                }
                continue;
            }
            let rel = relative(self.root, path);
            for call in &graph.calls {
                let Some(sym) = SymbolRef::of_call(path, call) else {
                    continue;
                };
                if let Some(key) = self.attribute(&sym, true) {
                    let usage = found.entry(key).or_default();
                    usage.calls += 1;
                    usage.files.insert(rel.clone());
                }
            }
            for (_, full) in imports {
                let Some(sym) = SymbolRef::of_import(path, full) else {
                    continue;
                };
                if let Some(key) = self.attribute(&sym, false) {
                    let usage = found.entry(key).or_default();
                    usage.refs += 1;
                    usage.files.insert(rel.clone());
                }
            }
        }
        found
    }

    /// A szimbólum csomagja és tulajdonos része. `call`: egy hívásé (JS-ben a `from` csak a nem helyi forrás, az
    /// mindig csomag). A JS-import értéke a helyi import feloldott útja is lehet (`src/lib/a.js`, csomag-alakú), így
    /// ott csak a rész manifestjében / lockjában ismert csomag, és a `@scope/…` vagy a `/` és `.` nélküli specifikátor
    /// (`dayjs`) számít.
    fn attribute(&self, sym: &SymbolRef<'_>, call: bool) -> Option<(usize, String)> {
        let Resolution::Package(found) = self.packages.resolve(sym) else {
            return None;
        };
        if sym.ecosystem == Ecosystem::Node && !call {
            let known = self
                .packages
                .map
                .nearest(sym.path, Ecosystem::Node)
                .is_some_and(|part| part.package(&found.name).is_some());
            let spec = sym.qualifier;
            let bare = spec.starts_with('@') || !spec.contains(['/', '.']);
            if !known && !bare {
                return None;
            }
        }
        let part = self.owner(sym.path, sym.ecosystem, &found.name)?;
        Some((part, found.name))
    }

    /// A fájlhoz legközelebbi rész az ökoszisztémában — vagy, ha az nem deklarálja a csomagot, a legközelebbi
    /// ős-rész, amelyik igen (monorepo: a gyökér `package.json`-jából felhúzott függőség).
    fn owner(&self, path: &Path, ecosystem: Ecosystem, name: &str) -> Option<usize> {
        let mut chain: Vec<(usize, &EcosystemPackages)> = self
            .packages
            .map
            .parts
            .iter()
            .enumerate()
            .filter(|(_, part)| part.ecosystem == ecosystem && path.starts_with(&part.root))
            .collect();
        chain.sort_by_key(|(_, part)| std::cmp::Reverse(part.root.components().count()));
        let nearest = chain.first()?.0;
        let declaring = chain
            .iter()
            .find(|(_, part)| part.packages.iter().any(|p| p.name == name))
            .map(|(index, _)| *index);
        Some(declaring.unwrap_or(nearest))
    }

    /// Deklarált, de se hívás, se import: a kivételek nélkül.
    fn unused(&self, found: &BTreeMap<(usize, String), Usage>) -> Vec<Finding> {
        let rules: [&dyn PackageRule; 6] = [
            &ToolingPackage,
            &FrameworkAutoload,
            &ComposerPlugin,
            &CssOrAssetOnly,
            &NoNamespace,
            &self.ignore,
        ];
        let exempt = AnyOf(&rules);
        let empty = Usage::default();
        let mut out = Vec::new();
        for (index, part) in self.packages.map.parts.iter().enumerate() {
            for package in &part.packages {
                let usage = found.get(&(index, package.name.clone())).unwrap_or(&empty);
                if usage.used() || exempt.first_match(package, usage).is_some() {
                    continue;
                }
                out.push(Finding {
                    kind: Kind::Unused,
                    part: index,
                    name: package.name.clone(),
                    reason: Reason::NoReference,
                });
            }
        }
        out
    }

    /// Használt, de a rész manifestje nem deklarálja: a kivételek nélkül.
    fn missing(&self, found: &BTreeMap<(usize, String), Usage>) -> Vec<Finding> {
        let workspace = WorkspacePackage::of(&self.packages.map);
        let mut out = Vec::new();
        for ((index, name), usage) in found {
            let Some(part) = self.packages.map.parts.get(*index) else {
                continue;
            };
            if !usage.used() || part.packages.iter().any(|p| p.name == *name) {
                continue;
            }
            let locked = part.installed_only.iter().find(|p| p.name == *name);
            let package = locked
                .cloned()
                .unwrap_or_else(|| undeclared(name, part.ecosystem));
            let bundled = FrameworkBundled::of(part);
            let rules: [&dyn PackageRule; 4] = [&BuiltinModule, &bundled, &workspace, &self.ignore];
            if AnyOf(&rules).matches(&package, usage) {
                continue;
            }
            let files: Vec<&str> = usage.files.iter().map(String::as_str).collect();
            let Some((file, more)) = first_and_more(&files) else {
                continue;
            };
            out.push(Finding {
                kind: Kind::Missing,
                part: *index,
                name: name.clone(),
                reason: Reason::Undeclared {
                    ecosystem: part.ecosystem,
                    file,
                    more,
                    transitive: locked.is_some(),
                },
            });
        }
        out
    }

    /// Csak dev-függőség, de az alkalmazás-kód használja.
    fn dev_only(&self, found: &BTreeMap<(usize, String), Usage>) -> Vec<Finding> {
        let mut out = Vec::new();
        for (index, part) in self.packages.map.parts.iter().enumerate() {
            let bundled = BundledFrontend::of(part);
            let rules: [&dyn PackageRule; 2] = [&bundled, &self.ignore];
            let exempt = AnyOf(&rules);
            for package in part.packages.iter().filter(|p| p.dev) {
                let Some(usage) = found.get(&(index, package.name.clone())) else {
                    continue;
                };
                if exempt.matches(package, usage) {
                    continue;
                }
                let Some((file, more)) = first_and_more(&usage.app_files()) else {
                    continue;
                };
                out.push(Finding {
                    kind: Kind::DevOnly,
                    part: index,
                    name: package.name.clone(),
                    reason: Reason::DevInApp { file, more },
                });
            }
        }
        out
    }

    /// A találatok sorai (a `findings` már rendezett); több csomag-mappánál `[mappa]` előtaggal.
    fn format(&self, findings: &[Finding], lang: Lang) -> String {
        if findings.is_empty() {
            return lang
                .pick("no dependency issues found\n", "nincs függőség-probléma\n")
                .to_owned();
        }
        let folders: Vec<String> = self
            .packages
            .map
            .parts
            .iter()
            .map(|part| folder(&self.packages.map.root, &part.root))
            .collect();
        let several = folders.iter().collect::<BTreeSet<_>>().len() > 1;
        let label_width = [Kind::Unused, Kind::Missing, Kind::DevOnly]
            .iter()
            .map(|kind| kind.label(lang).chars().count())
            .max()
            .unwrap_or(0)
            + 2;
        let name_width = findings
            .iter()
            .map(|finding| finding.name.chars().count())
            .max()
            .unwrap_or(0)
            + 2;
        let mut out = String::new();
        for finding in findings {
            if several && let Some(folder) = folders.get(finding.part) {
                out.push('[');
                out.push_str(folder);
                out.push_str("] ");
            }
            let line = format!(
                "{:<label_width$}{:<name_width$}{}",
                finding.kind.label(lang),
                finding.name,
                reason_text(&finding.reason, lang)
            );
            out.push_str(line.trim_end());
            out.push('\n');
        }
        out
    }
}

/// Egy nem deklarált, a lockban sem szereplő csomag a szabályokhoz.
fn undeclared(name: &str, ecosystem: Ecosystem) -> Package {
    Package {
        name: name.to_owned(),
        ecosystem,
        declared: None,
        installed: None,
        dev: false,
        namespaces: Vec::new(),
        auto_discovered: false,
        plugin: false,
    }
}

/// Az első fájl és a többi darabszáma; üresre `None`.
fn first_and_more(files: &[&str]) -> Option<(String, usize)> {
    let (first, rest) = files.split_first()?;
    Some(((*first).to_owned(), rest.len()))
}

/// Az indoklás szövege.
fn reason_text(reason: &Reason, lang: Lang) -> String {
    match reason {
        Reason::NoReference => lang
            .pick("no reference in code", "nincs hivatkozás a kódban")
            .to_owned(),
        Reason::Undeclared {
            ecosystem,
            file,
            more,
            transitive,
        } => {
            let more = plus(*more);
            match (ecosystem, lang) {
                (Ecosystem::Node, Lang::En) => {
                    format!("imported in {file}{more}, not in package.json")
                }
                (Ecosystem::Node, Lang::Hu) => {
                    format!("importálva: {file}{more}, nincs a package.json-ban")
                }
                (Ecosystem::Php, Lang::En) if *transitive => {
                    format!("used in {file}{more}, not in composer.json (transitive)")
                }
                (Ecosystem::Php, Lang::Hu) if *transitive => {
                    format!("használva: {file}{more}, nincs a composer.json-ban (tranzitív)")
                }
                (Ecosystem::Php, Lang::En) => {
                    format!("used in {file}{more}, not in composer.json")
                }
                (Ecosystem::Php, Lang::Hu) => {
                    format!("használva: {file}{more}, nincs a composer.json-ban")
                }
            }
        }
        Reason::DevInApp { file, more } => {
            let more = plus(*more);
            match lang {
                Lang::En => format!("used in {file}{more}"),
                Lang::Hu => format!("használva: {file}{more}"),
            }
        }
    }
}

/// ` (+N)`, ha `N > 0`.
fn plus(more: usize) -> String {
    if more > 0 {
        format!(" (+{more})")
    } else {
        String::new()
    }
}

/// A rész mappája a gyökérhez képest (`.` a gyökér).
fn folder(root: &Path, dir: &Path) -> String {
    dir.strip_prefix(root)
        .ok()
        .filter(|rel| !rel.as_os_str().is_empty())
        .map_or_else(
            || ".".to_owned(),
            |rel| rel.to_string_lossy().replace('\\', "/"),
        )
}

/// Az út a gyökérhez képest, `/`-rel.
fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Stíluslap-e (`.css`, `.scss`, `.sass`, `.less`): az importja csak CSS-hivatkozás.
fn is_stylesheet(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            ["css", "scss", "sass", "less"]
                .iter()
                .any(|known| ext.eq_ignore_ascii_case(known))
        })
}

/// Teszt-fájl-e (a gyökérhez relatív út): `tests/`, `test/`, `__tests__/`, `spec/` alatt, vagy `*.test.*`,
/// `*.spec.*`, `*Test.php`.
fn is_test_path(rel: &str) -> bool {
    let mut segments: Vec<&str> = rel.split('/').collect();
    let name = segments.pop().unwrap_or_default();
    segments
        .iter()
        .any(|dir| matches!(*dir, "tests" | "test" | "__tests__" | "spec"))
        || name.contains(".test.")
        || name.contains(".spec.")
        || name.ends_with("Test.php")
}

/// Fejlesztői segédfájl-e (a gyökérhez relatív út): `database/factories`, `database/seeders` (`seeds`), `stubs/`
/// alatt, vagy `*.config.*` (`vite.config.ts`, `tailwind.config.js`).
fn is_dev_support_path(rel: &str) -> bool {
    let mut segments: Vec<&str> = rel.split('/').collect();
    let name = segments.pop().unwrap_or_default();
    segments.contains(&"stubs")
        || segments.windows(2).any(|pair| {
            pair[0] == "database" && matches!(pair[1], "factories" | "seeders" | "seeds")
        })
        || name.contains(".config.")
}
