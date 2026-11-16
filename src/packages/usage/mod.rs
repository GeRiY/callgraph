//! `uses <csomag>` / MCP `package_usage`: hol és hogyan használja a projekt egy függőségét. A csomagot a `locate`
//! csomag-egyezésével választja ki (teljes név, címke, gyártó / scope, utolsó tag — `Attribution::matches_term`;
//! előbb a közvetlen függőségek, a címke-egyezés előnyben). A projekt-gráf kódfájljainak hívásait (újraelemzés
//! nélkül) a feloldó lánc (`Packages::package_of_call`) rendeli a csomaghoz; a hívás nélküli `use` / `import`
//! (csak típus: `catch`, típusjelölés) külön számít (`refs`, a kimenetben `(type)`).
//!
//! Alapból tömör összesítés: API-szimbólumonként a darabszám, az első fájl és `+N` (legfeljebb 20 sor); a `--calls`
//! / `calls: true` fájlonként minden hívás-sort ad.
//!
//! ```text
//! guzzlehttp/guzzle 7.9.2 · 14 calls · 1 ref · 6 files
//!   Client::request   7  app/Services/PaymentGateway.php +2
//!   RequestException  1  (type) app/Services/PaymentGateway.php
//! ```

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use super::Packages;
use super::model::Package;
use super::resolve::{Attribution, SymbolRef};
use crate::i18n::Lang;
use crate::model::{Call, FileGraph};

/// Legfeljebb ennyi API-szimbólum-sor az összesítésben (utána `+N more`).
const MAX_SYMBOLS: usize = 20;
/// Az ismeretlen névre legfeljebb ennyi hasonló csomagnév.
const MAX_SUGGESTIONS: usize = 5;
/// A többértelmű névnél legfeljebb ennyi jelölt (utána `+N`).
const MAX_CANDIDATES: usize = 10;

/// A kérdezett csomag a csomagtérkép szerint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Target {
    /// Egy ismert (manifest / lock) csomag.
    Known(Attribution),
    /// Több ismert csomag illik rá (a nevük, ábécében).
    Ambiguous(Vec<String>),
    /// Egy ismert csomag sem illik rá: a hívások között keresendő (a nem deklarált JS-import is csomag).
    Search,
    /// A projektnek nincs manifestje (`composer.json` / `package.json`).
    NoManifest,
}

/// Egy használat: a fájl (a gyökérhez relatív, `/`-rel), a sor (az importnál nincs), az API-szimbólum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Site {
    pub file: String,
    pub line: Option<u32>,
    pub symbol: String,
}

/// Egy csomag használatai.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Usage {
    pub version: Option<String>,
    pub calls: Vec<Site>,
    /// A hívás nélküli importok (csak típus: `catch`, típusjelölés, `instanceof`).
    pub refs: Vec<Site>,
}

/// Egy `uses` lekérdezés: a csomagtérkép, a gyökér (a relatív utakhoz) és a kiválasztott csomag.
pub(crate) struct UsageQuery<'a> {
    packages: &'a Packages,
    root: &'a Path,
    term: String,
    target: Target,
}

impl<'a> UsageQuery<'a> {
    /// A kifejezésre illő csomag kiválasztása (fájl-bejárás nélkül).
    pub(crate) fn new(packages: &'a Packages, root: &'a Path, term: &str) -> Self {
        let term = term.trim().to_owned();
        let target = pick(packages, &term);
        Self {
            packages,
            root,
            term,
            target,
        }
    }

    /// Kell-e a projekt fájljait bejárni (a többértelmű névnél és manifest nélkül nem).
    pub(crate) fn needs_files(&self) -> bool {
        matches!(self.target, Target::Known(_) | Target::Search)
    }

    /// A válasz szövege a projekt kódfájljaiból (abszolút út, gráf); `calls`: fájlonként minden hívás-sor.
    pub(crate) fn render<'g>(
        &self,
        files: impl IntoIterator<Item = (&'g Path, &'g FileGraph)>,
        calls: bool,
        lang: Lang,
    ) -> String {
        match &self.target {
            Target::NoManifest => no_manifest(&self.term, lang),
            Target::Ambiguous(names) => ambiguous(&self.term, names, lang),
            Target::Known(package) => {
                let mut found = self.scan(files, |candidate| candidate.name == package.name);
                let mut usage = found.remove(&package.name).unwrap_or_default();
                if package.version.is_some() {
                    usage.version.clone_from(&package.version);
                }
                render_usage(&package.name, &usage, calls, lang)
            }
            Target::Search => {
                let mut found = self.scan(files, |candidate| candidate.matches_term(&self.term));
                if found.len() > 1 {
                    let names: Vec<String> = found.into_keys().collect();
                    return ambiguous(&self.term, &names, lang);
                }
                match found.pop_first() {
                    Some((name, usage)) => render_usage(&name, &usage, calls, lang),
                    None => unknown(&self.term, &suggestions(self.packages, &self.term), lang),
                }
            }
        }
    }

    /// A fájlok hívásai és hívás nélküli importjai csomagonként (név szerint), az `accept`-nek megfelelő csomagokra.
    fn scan<'g>(
        &self,
        files: impl IntoIterator<Item = (&'g Path, &'g FileGraph)>,
        accept: impl Fn(&Attribution) -> bool,
    ) -> BTreeMap<String, Usage> {
        let mut found: BTreeMap<String, Usage> = BTreeMap::new();
        for (path, graph) in files {
            let rel = relative(self.root, path);
            let mut used: Vec<&Call> = Vec::new();
            for call in &graph.calls {
                let Some(package) = self.packages.package_of_call(path, call) else {
                    continue;
                };
                if !accept(&package) {
                    continue;
                }
                entry(&mut found, package).calls.push(Site {
                    file: rel.clone(),
                    line: Some(call.line),
                    symbol: api_symbol(call),
                });
                used.push(call);
            }
            let (imports, _) = graph.imports.parts();
            for (alias, full) in imports {
                let Some(package) = self.packages.package_of_import(path, full) else {
                    continue;
                };
                if !accept(&package) || used.iter().any(|call| covers(path, call, alias, full)) {
                    continue;
                }
                entry(&mut found, package).refs.push(Site {
                    file: rel.clone(),
                    line: None,
                    symbol: ref_symbol(alias, full),
                });
            }
        }
        found
    }
}

/// A kifejezésre illő ismert csomag: a pontos név (kis- / nagybetű-független); különben előbb a közvetlen
/// függőségek, utána a csak a lockban szereplők közül a címkére (`guzzle`, `laravel`), majd a `locate`
/// csomag-egyezésére (gyártó / scope, utolsó tag, annak első szava) illők — egy találat dönt, több többértelmű.
fn pick(packages: &Packages, term: &str) -> Target {
    if !packages.has_parts() || term.is_empty() {
        return Target::NoManifest;
    }
    let (direct, locked) = known(packages);
    if let Some(exact) = direct
        .iter()
        .chain(&locked)
        .find(|package| package.name.eq_ignore_ascii_case(term))
    {
        return Target::Known(Attribution::of(exact));
    }
    for group in [&direct, &locked] {
        let by_label = select(group, |found| found.label().eq_ignore_ascii_case(term));
        let hits = if by_label.is_empty() {
            select(group, |found| found.matches_term(term))
        } else {
            by_label
        };
        match hits.as_slice() {
            [] => {}
            [one] => return Target::Known(Attribution::of(one)),
            many => {
                let mut names: Vec<String> =
                    many.iter().map(|package| package.name.clone()).collect();
                names.sort();
                return Target::Ambiguous(names);
            }
        }
    }
    Target::Search
}

/// A csoport csomagjai, amelyekre a próba (a csomag hívás-alakján) igaz.
fn select<'p>(group: &[&'p Package], test: impl Fn(&Attribution) -> bool) -> Vec<&'p Package> {
    group
        .iter()
        .copied()
        .filter(|package| test(&Attribution::of(package)))
        .collect()
}

/// Az ismert csomagok, név szerint egyszer, a részek sorrendjében: (közvetlen függőségek, csak a lockban szereplők).
fn known(packages: &Packages) -> (Vec<&Package>, Vec<&Package>) {
    let mut direct: Vec<&Package> = Vec::new();
    for package in packages.map.all().flat_map(|part| &part.packages) {
        if !direct.iter().any(|seen| seen.name == package.name) {
            direct.push(package);
        }
    }
    let mut locked: Vec<&Package> = Vec::new();
    for package in packages.map.all().flat_map(|part| &part.installed_only) {
        if !direct
            .iter()
            .chain(&locked)
            .any(|seen| seen.name == package.name)
        {
            locked.push(package);
        }
    }
    (direct, locked)
}

/// A csomag bejegyzése (az első ismert verzióval).
fn entry(found: &mut BTreeMap<String, Usage>, package: Attribution) -> &mut Usage {
    let usage = found.entry(package.name).or_default();
    if usage.version.is_none() {
        usage.version = package.version;
    }
    usage
}

/// Az út a gyökérhez képest, `/`-rel.
fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// A hívott API-szimbólum röviden. PHP: a feloldott cél névtér nélkül (`Client::request`, `new Client`,
/// `json_encode`); JS / TS / Vue: a hívott kifejezés (`axios.get`, `useMouse`), a típusos fogadónál (`this.client.get()`)
/// a feloldott `Típus::metódus`.
fn api_symbol(call: &Call) -> String {
    if call.from.is_some() {
        return match call.resolved.as_deref() {
            Some(resolved) if call.target.starts_with("this.") => resolved.to_owned(),
            _ => call.target.clone(),
        };
    }
    let full = call
        .resolved
        .as_deref()
        .unwrap_or(&call.target)
        .trim_start_matches('\\');
    let (class, member) = match full.split_once("::") {
        Some((class, member)) => (class, Some(member)),
        None => (full, None),
    };
    let short = class.rsplit('\\').next().unwrap_or(class);
    match member {
        Some(member) => format!("{short}::{member}"),
        None if call.kind == "new" => format!("new {short}"),
        None => short.to_owned(),
    }
}

/// Az import szimbóluma: PHP-ban a `use` utolsó tagja (`RequestException`), JS-ben az importált név (`useMouse`;
/// a default és a névtér-importnál a helyi név).
fn ref_symbol(alias: &str, full: &str) -> String {
    match full.split_once("::") {
        Some((_, name)) if name != "*" => name.to_owned(),
        Some(_) => alias.to_owned(),
        None if full.contains('\\') => full.rsplit('\\').next().unwrap_or(full).to_owned(),
        None => alias.to_owned(),
    }
}

/// Lefed-e egy (a csomaghoz rendelt) hívás egy importot: PHP-ban a hívás osztálya / függvénye maga az import
/// (`use GuzzleHttp\Client` ↔ `new Client`, `$client->get()`); JS-ben ugyanabból a forrásból, a helyi vagy az
/// importált néven át hívják (`import { useMouse } from '@vueuse/core'` ↔ `useMouse()`, típusos fogadó).
fn covers(path: &Path, call: &Call, alias: &str, full: &str) -> bool {
    let Some(spec) = call.from.as_deref() else {
        let full = full.trim_start_matches('\\');
        return SymbolRef::of_call(path, call)
            .is_some_and(|sym| sym.qualifier.eq_ignore_ascii_case(full));
    };
    let (source, imported) = match full.split_once("::") {
        Some((source, name)) if name != "*" => (source, name),
        Some((source, _)) => (source, alias),
        None => (full, alias),
    };
    if source != spec {
        return false;
    }
    let resolved = call.resolved.as_deref().map(first_name);
    first_name(&call.target) == alias || resolved == Some(imported) || resolved == Some(alias)
}

/// A kifejezés első neve (`axios.get` → `axios`, `Client::get` → `Client`).
fn first_name(text: &str) -> &str {
    text.split(['.', ':', '(', '?']).next().unwrap_or(text)
}

/// Darabszám a főnévvel (angolul egyes / többes szám).
fn counted(n: usize, en: (&str, &str), hu: &str, lang: Lang) -> String {
    let noun = match lang {
        Lang::En if n == 1 => en.0,
        Lang::En => en.1,
        Lang::Hu => hu,
    };
    format!("{n} {noun}")
}

/// A válasz egy csomagra: a fejléc, utána az összesítés (vagy `calls`: fájlonként a hívás-sorok); használat
/// nélkül egy sor.
fn render_usage(name: &str, usage: &Usage, calls: bool, lang: Lang) -> String {
    let title = match &usage.version {
        Some(version) => format!("{name} {version}"),
        None => name.to_owned(),
    };
    if usage.calls.is_empty() && usage.refs.is_empty() {
        return match lang {
            Lang::En => format!("{title}: not used in the code (no call or import found)\n"),
            Lang::Hu => format!("{title}: a kód nem használja (nincs hívás vagy import)\n"),
        };
    }
    let mut files: Vec<&str> = usage
        .calls
        .iter()
        .chain(&usage.refs)
        .map(|site| site.file.as_str())
        .collect();
    files.sort_unstable();
    files.dedup();
    let mut out = format!(
        "{title} · {}",
        counted(usage.calls.len(), ("call", "calls"), "hívás", lang)
    );
    if !usage.refs.is_empty() {
        let refs = counted(usage.refs.len(), ("ref", "refs"), "hivatkozás", lang);
        let _ = write!(out, " · {refs}");
    }
    let _ = writeln!(
        out,
        " · {}",
        counted(files.len(), ("file", "files"), "fájl", lang)
    );
    if calls {
        listing(&mut out, usage, lang);
    } else {
        summary(&mut out, usage, lang);
    }
    out
}

/// A típus-hivatkozás jelölése.
fn type_mark(lang: Lang) -> &'static str {
    lang.pick("(type)", "(típus)")
}

/// Egy összesítő sor: szimbólum, típus-hivatkozás-e, darabszám, fájlonként a darabszám (út szerint).
struct Row<'u> {
    symbol: &'u str,
    reference: bool,
    count: usize,
    files: BTreeMap<&'u str, usize>,
}

/// API-szimbólumonként egy sor (darabszám szerint csökkenő, utána név szerint): az első fájl (a legtöbb
/// használattal, egyenlőségnél az út szerint első) és `+N` további fájl; legfeljebb `MAX_SYMBOLS` sor.
fn summary(out: &mut String, usage: &Usage, lang: Lang) {
    let mut groups: BTreeMap<(&str, bool), BTreeMap<&str, usize>> = BTreeMap::new();
    for (sites, reference) in [(&usage.calls, false), (&usage.refs, true)] {
        for site in sites {
            *groups
                .entry((site.symbol.as_str(), reference))
                .or_default()
                .entry(site.file.as_str())
                .or_default() += 1;
        }
    }
    let mut rows: Vec<Row<'_>> = groups
        .into_iter()
        .map(|((symbol, reference), files)| Row {
            symbol,
            reference,
            count: files.values().sum(),
            files,
        })
        .collect();
    rows.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| a.symbol.cmp(b.symbol))
            .then_with(|| a.reference.cmp(&b.reference))
    });
    let hidden = rows.len().saturating_sub(MAX_SYMBOLS);
    rows.truncate(MAX_SYMBOLS);
    let width = rows
        .iter()
        .map(|row| row.symbol.chars().count())
        .max()
        .unwrap_or(0);
    let digits = rows
        .iter()
        .map(|row| row.count.to_string().len())
        .max()
        .unwrap_or(1);
    for row in &rows {
        let mut first: Option<(&str, usize)> = None;
        for (&file, &n) in &row.files {
            if first.is_none_or(|(_, best)| n > best) {
                first = Some((file, n));
            }
        }
        let first = first.map_or("", |(file, _)| file);
        let mark = if row.reference {
            format!("{} ", type_mark(lang))
        } else {
            String::new()
        };
        let mut line = format!(
            "  {:<width$}  {:>digits$}  {mark}{first}",
            row.symbol, row.count
        );
        let more = row.files.len().saturating_sub(1);
        if more > 0 {
            let _ = write!(line, " +{more}");
        }
        out.push_str(&line);
        out.push('\n');
    }
    if hidden > 0 {
        let _ = match lang {
            Lang::En => writeln!(out, "  +{hidden} more"),
            Lang::Hu => writeln!(out, "  +{hidden} további"),
        };
    }
}

/// `calls`: fájlonként (út szerint) a hívás-sorok (`L42 Client::request`, sor szerint), utána a típus-hivatkozások.
fn listing(out: &mut String, usage: &Usage, lang: Lang) {
    let mut by_file: BTreeMap<&str, (Vec<&Site>, Vec<&Site>)> = BTreeMap::new();
    for site in &usage.calls {
        by_file.entry(site.file.as_str()).or_default().0.push(site);
    }
    for site in &usage.refs {
        by_file.entry(site.file.as_str()).or_default().1.push(site);
    }
    for (file, (mut calls, mut refs)) in by_file {
        out.push_str(file);
        out.push('\n');
        calls.sort_by(|a, b| a.line.cmp(&b.line).then_with(|| a.symbol.cmp(&b.symbol)));
        for site in calls {
            let _ = writeln!(out, "  L{} {}", site.line.unwrap_or(0), site.symbol);
        }
        refs.sort_by(|a, b| a.symbol.cmp(&b.symbol));
        refs.dedup_by(|a, b| a.symbol == b.symbol);
        for site in refs {
            let _ = writeln!(out, "  {} {}", type_mark(lang), site.symbol);
        }
    }
}

/// Manifest nélküli projekt.
fn no_manifest(term: &str, lang: Lang) -> String {
    match lang {
        Lang::En => {
            format!("unknown package: {term} (no composer.json / package.json in the project)\n")
        }
        Lang::Hu => {
            format!("ismeretlen csomag: {term} (a projektben nincs composer.json / package.json)\n")
        }
    }
}

/// Többértelmű név: a jelöltek (legfeljebb `MAX_CANDIDATES`, utána `+N`).
fn ambiguous(term: &str, names: &[String], lang: Lang) -> String {
    let mut list = names
        .iter()
        .take(MAX_CANDIDATES)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    let hidden = names.len().saturating_sub(MAX_CANDIDATES);
    if hidden > 0 {
        let _ = write!(list, " +{hidden}");
    }
    match lang {
        Lang::En => format!("ambiguous package name: {term} → {list} (give the full name)\n"),
        Lang::Hu => {
            format!("többértelmű csomagnév: {term} → {list} (add meg a teljes nevet)\n")
        }
    }
}

/// Ismeretlen név: egy sor, a végén a leghasonlóbb csomagnevek (ha vannak).
fn unknown(term: &str, suggestions: &[String], lang: Lang) -> String {
    let mut line = match lang {
        Lang::En => format!(
            "unknown package: {term} (not in composer.json / package.json or a lock, and no call uses it)"
        ),
        Lang::Hu => format!(
            "ismeretlen csomag: {term} (nincs a composer.json / package.json-ban vagy a lockban, és egy hívás sem használja)"
        ),
    };
    if !suggestions.is_empty() {
        let list = suggestions.join(", ");
        let _ = match lang {
            Lang::En => write!(line, "; closest: {list}"),
            Lang::Hu => write!(line, "; leghasonlóbb: {list}"),
        };
    }
    line.push('\n');
    line
}

/// A kifejezéshez leghasonlóbb ismert csomagnevek (a teljes névhez és az utolsó taghoz mérve).
fn suggestions(packages: &Packages, term: &str) -> Vec<String> {
    let (direct, locked) = known(packages);
    let items: Vec<(&str, &str)> = direct
        .iter()
        .chain(&locked)
        .flat_map(|package| {
            let name = package.name.as_str();
            let last = name.rsplit('/').next().unwrap_or(name);
            [(name, name), (last, name)]
        })
        .collect();
    crate::query::hint::closest(term, items, MAX_SUGGESTIONS)
}

#[cfg(test)]
mod tests;
