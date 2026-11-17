//! Import-forrás feloldás: jsconfig/tsconfig keresés (`extends`, `references`, `baseUrl`, `paths`), Vite `resolve.alias`
//! és a modul-útvonalak (JE/JD is ezt használja). A megjelenített kulcs a projekt-gyökérhez (workspace) relatív.

use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use jsonc_parser::{CollectOptions, ParseOptions, ast::Value, parse_to_ast};

use super::vite_alias::vite_aliases;

/// Egy mappa-listázás ennyi ideig érvényes (a hosszan futó szerverben az új fájlok így is előkerülnek).
const LISTING_TTL: Duration = Duration::from_secs(5);

/// Kiterjesztés-próba sorrendje (`./a` -> `a.<ext>`).
const EXTENSIONS: [&str; 9] = ["js", "ts", "vue", "mjs", "tsx", "jsx", "mts", "cjs", "cts"];

/// Mappa-index fájlok (`./dir` -> `dir/<index>`).
const INDEXES: [&str; 6] = [
    "index.js",
    "index.ts",
    "index.vue",
    "index.tsx",
    "index.jsx",
    "index.mjs",
];

/// A projekt-gyökér (workspace) jelzői: ha van ilyen fájl egy mappában, a mappa a projekt része.
const WORKSPACE_MARKERS: [&str; 9] = [
    "package.json",
    "pnpm-workspace.yaml",
    "lerna.json",
    "nx.json",
    "turbo.json",
    "composer.json",
    "tsconfig.json",
    "jsconfig.json",
    "tsconfig.base.json",
];

/// Egy `paths` bejegyzés: minta (legfeljebb egy `*`) és a (már abszolút) célok.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PathRule {
    prefix: String,
    /// `None`: nincs `*` (pontos egyezés).
    suffix: Option<String>,
    targets: Vec<String>,
}

impl PathRule {
    fn new(pattern: &str, targets: Vec<String>) -> Self {
        match pattern.split_once('*') {
            Some((prefix, suffix)) => Self {
                prefix: prefix.to_string(),
                suffix: Some(suffix.to_string()),
                targets,
            },
            None => Self {
                prefix: pattern.to_string(),
                suffix: None,
                targets,
            },
        }
    }

    /// A `*` helyére illeszkedő rész (pontos mintánál üres), ha a minta illeszkedik.
    fn matches<'a>(&self, spec: &'a str) -> Option<&'a str> {
        match &self.suffix {
            None => (spec == self.prefix).then_some(""),
            Some(suffix) => {
                let min = self.prefix.len() + suffix.len();
                (spec.len() >= min && spec.starts_with(&self.prefix) && spec.ends_with(suffix))
                    .then(|| &spec[self.prefix.len()..spec.len() - suffix.len()])
            }
        }
    }
}

/// Egy mappához tartozó feloldási konfiguráció (a legközelebbi jsconfig/tsconfig vagy package.json).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolveConfig {
    /// A csomag-gyökér: a legközelebbi jsconfig/tsconfig/package.json mappája (a `paths` és az alias ehhez tartozik).
    pub root: PathBuf,
    /// A projekt-gyökér: ehhez viszonyul a megjelenített kulcs és a `module` (monorepóban a csomagok közös őse).
    pub workspace: PathBuf,
    /// `paths` bejegyzések deklarációs sorrendben (a `references` által hivatkozott configokkal együtt).
    rules: Vec<PathRule>,
    /// Vite `resolve.alias`: (név, abszolút cél).
    aliases: Vec<(String, PathBuf)>,
}

/// Egy feloldott import-forrás.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    /// A megjelenített forrás: lokális fájlnál a projekt-gyökérhez relatív út, csomagnál az eredeti specifikátor.
    pub display: String,
    /// `true`, ha a forrás a projekt egy (alias-szal vagy relatívan elért) útjára mutat.
    pub local: bool,
}

/// Egy mappa tartalma (fájlnevek és almappák), az import-feloldás `is_file` hívásai helyett.
#[derive(Debug)]
struct Listing {
    at: Instant,
    files: HashSet<String>,
    dirs: HashSet<String>,
}

/// Mappánként gyorsítótárazott import-feloldó (szálbiztos, megosztható `Arc`-kal).
#[derive(Debug, Default)]
pub struct Resolver {
    cache: Mutex<HashMap<PathBuf, Arc<ResolveConfig>>>,
    listings: Mutex<HashMap<PathBuf, Arc<Listing>>>,
}

impl Resolver {
    pub fn new() -> Self {
        Self::default()
    }

    /// A fájlhoz tartozó konfiguráció (felfelé keresi a jsconfig/tsconfig/package.json mappát).
    pub fn config(&self, file: &Path) -> Arc<ResolveConfig> {
        let dir = absolute(file)
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();
        self.config_for_dir(&dir)
    }

    /// A fájl csomag-gyökere (a legközelebbi jsconfig/tsconfig/package.json mappája).
    pub fn root(&self, file: &Path) -> PathBuf {
        self.config(file).root.clone()
    }

    /// A fájl útja a projekt-gyökérhez képest (a `module` mező).
    pub fn module_path(&self, file: &Path) -> String {
        relative(&self.config(file).workspace, &absolute(file))
    }

    /// Az import-forrás feloldása a `from` fájlból nézve.
    pub fn resolve(&self, spec: &str, from: &Path) -> Resolved {
        let cfg = self.config(from);
        match self.resolve_abs(spec, from, &cfg) {
            Some(file) => Resolved {
                display: relative(&cfg.workspace, &file),
                local: true,
            },
            None => Resolved {
                display: spec.to_string(),
                local: false,
            },
        }
    }

    /// A helyi import abszolút célfájlja (`None`: csomag).
    pub fn resolve_path(&self, spec: &str, from: &Path) -> Option<PathBuf> {
        let cfg = self.config(from);
        self.resolve_abs(spec, from, &cfg)
    }

    /// A feloldott forrás megjelenített alakja.
    pub fn display(&self, spec: &str, from: &Path) -> String {
        self.resolve(spec, from).display
    }

    fn resolve_abs(&self, spec: &str, from: &Path, cfg: &ResolveConfig) -> Option<PathBuf> {
        if spec.starts_with('.') {
            let dir = absolute(from).parent().map(Path::to_path_buf);
            let abs = normalize(&dir.unwrap_or_default().join(spec));
            return Some(self.try_file(&abs).unwrap_or(abs));
        }
        // 1. tsconfig `paths` (a leghosszabb előtag előbb), 2. Vite alias, 3. a nem létező `paths` cél (régi viselkedés)
        let mut phantom = None;
        for (candidate, may_be_missing) in cfg.path_candidates(spec) {
            if let Some(file) = self.try_file(&candidate) {
                return Some(file);
            }
            if may_be_missing && phantom.is_none() {
                phantom = Some(candidate);
            }
        }
        for candidate in cfg.alias_candidates(spec) {
            if let Some(file) = self.try_file(&candidate) {
                return Some(file);
            }
        }
        phantom
    }

    /// Kiterjesztés-próba: p, p.<ext>, p/index.<ext>; a TS-es `./a.js` -> `a.ts`. Mappánként egy listázás.
    fn try_file(&self, p: &Path) -> Option<PathBuf> {
        let parent = p.parent()?;
        let name = p.file_name()?.to_string_lossy().into_owned();
        let list = self.listing(parent);
        if list.files.contains(&name) {
            return Some(p.to_path_buf());
        }
        if let Some((stem, ext)) = name.rsplit_once('.') {
            let swaps: &[&str] = match ext {
                "js" => &["ts", "tsx"],
                "jsx" => &["tsx"],
                "mjs" => &["mts"],
                "cjs" => &["cts"],
                _ => &[],
            };
            for swap in swaps {
                let alt = format!("{stem}.{swap}");
                if list.files.contains(&alt) {
                    return Some(parent.join(alt));
                }
            }
        }
        for ext in EXTENSIONS {
            let alt = format!("{name}.{ext}");
            if list.files.contains(&alt) {
                return Some(parent.join(alt));
            }
        }
        if list.dirs.contains(&name) {
            let inner = self.listing(p);
            for index in INDEXES {
                if inner.files.contains(index) {
                    return Some(p.join(index));
                }
            }
        }
        None
    }

    /// A mappa tartalma (memoizálva, rövid érvényességgel).
    fn listing(&self, dir: &Path) -> Arc<Listing> {
        if let Some(hit) = self.lock_listings().get(dir)
            && hit.at.elapsed() < LISTING_TTL
        {
            return hit.clone();
        }
        let mut files = HashSet::new();
        let mut dirs = HashSet::new();
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                // symlinknél a cél típusa számít
                let (is_file, is_dir) = match entry.file_type() {
                    Ok(kind) if kind.is_symlink() => {
                        let meta = std::fs::metadata(entry.path()).ok();
                        (
                            meta.as_ref().is_some_and(|m| m.is_file()),
                            meta.as_ref().is_some_and(|m| m.is_dir()),
                        )
                    }
                    Ok(kind) => (kind.is_file(), kind.is_dir()),
                    Err(_) => (false, false),
                };
                if is_file {
                    files.insert(name);
                } else if is_dir {
                    dirs.insert(name);
                }
            }
        }
        let listing = Arc::new(Listing {
            at: Instant::now(),
            files,
            dirs,
        });
        self.lock_listings()
            .insert(dir.to_path_buf(), listing.clone());
        listing
    }

    fn config_for_dir(&self, dir: &Path) -> Arc<ResolveConfig> {
        let mut visited = Vec::new();
        let mut cur = dir.to_path_buf();
        // nincs jsconfig/tsconfig/package.json felfelé: a gyökér a fájl saját mappája (nem a fájlrendszer teteje)
        let mut fallback = false;
        let found = loop {
            if let Some(hit) = self.lock().get(&cur) {
                break hit.clone();
            }
            visited.push(cur.clone());
            if let Some(cfg) = read_config(&cur) {
                break Arc::new(cfg.finish());
            }
            if cur.join("package.json").exists() {
                break Arc::new(ResolveConfig::plain(&cur).finish());
            }
            if cur.parent().is_none() {
                fallback = true;
                break Arc::new(ResolveConfig::plain(dir).finish());
            }
            cur = cur.parent().map(Path::to_path_buf).unwrap_or_default();
        };
        let mut cache = self.lock();
        if fallback {
            // a tartalék csak a kiinduló mappára érvényes, a közös ősökre nem
            cache.insert(dir.to_path_buf(), found.clone());
            return found;
        }
        for d in visited {
            cache.insert(d, found.clone());
        }
        found
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<PathBuf, Arc<ResolveConfig>>> {
        self.cache.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn lock_listings(&self) -> std::sync::MutexGuard<'_, HashMap<PathBuf, Arc<Listing>>> {
        self.listings.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl ResolveConfig {
    fn plain(dir: &Path) -> Self {
        Self {
            root: dir.to_path_buf(),
            workspace: dir.to_path_buf(),
            rules: Vec::new(),
            aliases: Vec::new(),
        }
    }

    /// A projekt-gyökér és a Vite alias kitöltése (mappánként egyszer, a `config_for_dir` gyorsítótárából).
    fn finish(mut self) -> Self {
        self.workspace = workspace_root(&self.root);
        self.aliases = vite_aliases(&self.root);
        self
    }

    /// A `paths` szerinti abszolút célok a leghosszabb illeszkedő előtagtól; a bool: a cél hiányozhat is
    /// (a puszta `*` minta nem: az minden csomag-importot elnyelne).
    fn path_candidates(&self, spec: &str) -> Vec<(PathBuf, bool)> {
        let mut hits: Vec<(usize, bool, &PathRule, &str)> = self
            .rules
            .iter()
            .filter_map(|rule| {
                let middle = rule.matches(spec)?;
                let rank = if rule.suffix.is_none() {
                    usize::MAX
                } else {
                    rule.prefix.len()
                };
                Some((rank, !rule.prefix.is_empty(), rule, middle))
            })
            .collect();
        // stabil rendezés: az egyenlő előtagok a deklarációs sorrendben maradnak
        hits.sort_by_key(|hit| std::cmp::Reverse(hit.0));
        hits.into_iter()
            .flat_map(|(_, may_be_missing, rule, middle)| {
                rule.targets.iter().map(move |target| {
                    (
                        PathBuf::from(target.replacen('*', middle, 1)),
                        may_be_missing,
                    )
                })
            })
            .collect()
    }

    /// A Vite alias szerinti abszolút célok (`@kit` -> `<mappa>/src/kit`, `@kit/Tag.vue` -> `…/Tag.vue`).
    fn alias_candidates(&self, spec: &str) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for (find, target) in &self.aliases {
            if spec == find {
                out.push(target.clone());
            } else if let Some(rest) = spec.strip_prefix(find.as_str())
                && (find.ends_with('/') || rest.starts_with('/'))
            {
                out.push(normalize(&target.join(rest.trim_start_matches('/'))));
            }
        }
        out
    }
}

/// A projekt-gyökér a csomag-gyökértől felfelé: a legközelebbi `.git` / `.callgraph` mappa; ha nincs, a
/// csomag-gyökértől összefüggően felfelé a legfelső mappa, amelyben workspace-jelző (`package.json`, …) van.
fn workspace_root(start: &Path) -> PathBuf {
    let mut top = start.to_path_buf();
    let mut chain = true;
    for dir in start.ancestors() {
        if dir.join(".git").exists() || dir.join(".callgraph").exists() {
            return dir.to_path_buf();
        }
        if chain {
            if dir == start || WORKSPACE_MARKERS.iter().any(|m| dir.join(m).exists()) {
                top = dir.to_path_buf();
            } else {
                chain = false;
            }
        }
    }
    top
}

/// A `dir`-ben lévő jsconfig.json, majd tsconfig.json; hibás tartalomnál üres `paths`.
fn read_config(dir: &Path) -> Option<ResolveConfig> {
    for name in ["jsconfig.json", "tsconfig.json"] {
        let file = dir.join(name);
        if !file.exists() {
            continue;
        }
        let mut cfg = ResolveConfig::plain(dir);
        if let Some((opts, references)) = read_ts(&file, 0) {
            cfg.rules = opts.rules();
            // a `references` által hivatkozott configok `paths`-a a sajátok után (az elsőként illeszkedő nyer)
            for reference in references {
                if let Some((opts, _)) = read_ts(&reference, 1) {
                    cfg.rules.extend(opts.rules());
                }
            }
        }
        return Some(cfg);
    }
    None
}

/// `paths` bejegyzések: minta → célok.
type PathRules = Vec<(String, Vec<String>)>;

/// Egy tsconfig `compilerOptions` része az `extends` lánc összefésülése után.
#[derive(Debug, Default)]
struct TsOptions {
    base: Option<PathBuf>,
    /// A `paths` bejegyzések és a meghatározó config mappája (`baseUrl` nélkül ehhez viszonyulnak).
    paths: Option<(PathRules, PathBuf)>,
}

impl TsOptions {
    /// A `paths` szabályok abszolút célokkal (`baseUrl`, ennek híján a `paths`-t meghatározó config mappája).
    fn rules(&self) -> Vec<PathRule> {
        let Some((paths, dir)) = &self.paths else {
            return Vec::new();
        };
        let base = self.base.clone().unwrap_or_else(|| dir.clone());
        paths
            .iter()
            .map(|(pattern, targets)| {
                let targets = targets
                    .iter()
                    .map(|t| normalize(&base.join(t)).to_string_lossy().into_owned())
                    .collect();
                PathRule::new(pattern, targets)
            })
            .collect()
    }
}

/// A JSON-tömb string elemei.
fn strings(value: &Value<'_>) -> Vec<String> {
    match value {
        Value::StringLit(s) => vec![s.value.to_string()],
        Value::Array(a) => a
            .elements
            .iter()
            .filter_map(|e| match e {
                Value::StringLit(s) => Some(s.value.to_string()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// Egy tsconfig/jsconfig `baseUrl` + `paths` az `extends` láncon át (komment- és trailing-comma-tűrő olvasás);
/// a második elem a `references` fájljai. Hibás tartalomra `None`.
fn read_ts(file: &Path, depth: u8) -> Option<(TsOptions, Vec<PathBuf>)> {
    let text = std::fs::read_to_string(file).ok()?;
    let ast = parse_to_ast(&text, &CollectOptions::default(), &ParseOptions::default()).ok()?;
    let Value::Object(root) = ast.value? else {
        return None;
    };
    let dir = file.parent()?.to_path_buf();
    let mut opts = TsOptions::default();
    if depth < 8
        && let Some(extends) = root.get("extends")
    {
        for spec in strings(&extends.value) {
            if let Some(parent) = extends_file(&dir, &spec)
                && let Some((parent, _)) = read_ts(&parent, depth + 1)
            {
                // a későbbi felülírja a korábbit
                if parent.base.is_some() {
                    opts.base = parent.base;
                }
                if parent.paths.is_some() {
                    opts.paths = parent.paths;
                }
            }
        }
    }
    if let Some(Value::Object(options)) = root.get("compilerOptions").map(|p| &p.value) {
        if let Some(Value::StringLit(base)) = options.get("baseUrl").map(|p| &p.value) {
            opts.base = Some(normalize(&dir.join(base.value.as_ref())));
        }
        if let Some(Value::Object(paths)) = options.get("paths").map(|p| &p.value) {
            let entries = paths
                .properties
                .iter()
                .map(|prop| (prop.name.as_str().to_string(), strings(&prop.value)))
                .collect();
            opts.paths = Some((entries, dir.clone()));
        }
    }
    let mut references = Vec::new();
    if let Some(Value::Array(list)) = root.get("references").map(|p| &p.value) {
        for item in &list.elements {
            if let Value::Object(o) = item
                && let Some(Value::StringLit(path)) = o.get("path").map(|p| &p.value)
            {
                let target = normalize(&dir.join(path.value.as_ref()));
                references.push(if target.is_dir() {
                    target.join("tsconfig.json")
                } else {
                    target
                });
            }
        }
    }
    Some((opts, references))
}

/// Az `extends` relatív (vagy abszolút) fájlja; a `.json` kiterjesztés elhagyható. Csomagnévre `None`.
fn extends_file(dir: &Path, spec: &str) -> Option<PathBuf> {
    if !(spec.starts_with('.') || Path::new(spec).is_absolute()) {
        return None;
    }
    let path = normalize(&dir.join(spec));
    if path.is_file() {
        return Some(path);
    }
    let with_json = PathBuf::from(format!("{}.json", path.to_string_lossy()));
    with_json.is_file().then_some(with_json)
}

pub(super) fn absolute(p: &Path) -> PathBuf {
    normalize(&std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf()))
}

/// Lexikális normalizálás (`.` és `..` kezelése), mint a `path.resolve`.
pub(super) fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// A `path.relative(from, to)` megfelelője (mindkét út abszolút, normalizált); az elválasztó mindig `/`.
fn relative(from: &Path, to: &Path) -> String {
    let f: Vec<_> = from.components().collect();
    let t: Vec<_> = to.components().collect();
    let common = f.iter().zip(&t).take_while(|(a, b)| a == b).count();
    let mut parts: Vec<String> = vec!["..".into(); f.len() - common];
    parts.extend(
        t[common..]
            .iter()
            .map(|c| c.as_os_str().to_string_lossy().into_owned()),
    );
    parts.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("rpg_resolve_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn relative_and_alias_sources_resolve_to_root_relative_paths() {
        let d = temp("alias");
        fs::create_dir_all(d.join("src/lib")).unwrap();
        fs::write(
            d.join("jsconfig.json"),
            "{ // komment\n \"compilerOptions\": { \"baseUrl\": \".\", \"paths\": { \"@/*\": [\"src/*\",], }, } }",
        )
        .unwrap();
        fs::write(d.join("src/lib/a.js"), "").unwrap();
        fs::write(d.join("src/b.vue"), "").unwrap();
        let r = Resolver::new();
        let from = d.join("src/lib/x.js");
        assert_eq!(r.display("./a", &from), "src/lib/a.js");
        assert_eq!(r.display("@/b", &from), "src/b.vue");
        assert_eq!(r.display("@/missing", &from), "src/missing");
        let pkg = r.resolve("vue", &from);
        assert_eq!((pkg.display.as_str(), pkg.local), ("vue", false));
        assert_eq!(r.module_path(&from), "src/lib/x.js");
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn package_json_marks_root_and_broken_config_has_no_paths() {
        let d = temp("pkg");
        fs::create_dir_all(d.join("a/b")).unwrap();
        fs::write(d.join("package.json"), "{}").unwrap();
        let r = Resolver::new();
        assert_eq!(r.root(&d.join("a/b/x.js")), d);
        fs::write(d.join("a/tsconfig.json"), "{ nem json").unwrap();
        let r2 = Resolver::new();
        assert_eq!(r2.root(&d.join("a/b/x.js")), d.join("a"));
        assert_eq!(r2.display("@/x", &d.join("a/b/x.js")), "@/x");
        let _ = fs::remove_dir_all(d);
    }

    /// jsconfig/tsconfig/package.json nélkül a gyökér a fájl mappája, nem a fájlrendszer teteje (`home/…` modulút).
    #[test]
    fn no_config_falls_back_to_the_file_folder() {
        let d = temp("noroot");
        fs::create_dir_all(d.join("sub")).unwrap();
        let r = Resolver::new();
        let file = d.join("sub/Setup.vue");
        let root = r.root(&file);
        if root.starts_with(&d) {
            assert_eq!(root, d.join("sub"));
            assert_eq!(r.module_path(&file), "Setup.vue");
            assert_eq!(r.display("./Child.vue", &file), "Child.vue");
        }
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn relative_paths_climb_with_dotdot() {
        assert_eq!(relative(Path::new("/a/b"), Path::new("/a/c/d")), "../c/d");
        assert_eq!(relative(Path::new("/"), Path::new("/x/y")), "x/y");
    }

    /// Monorepó: a kulcs a projekt-gyökérhez relatív, a két csomag ugyanazt a fájlt ugyanazon a kulcson látja;
    /// `extends` lánc, `references`, Vite alias, `.tsx` és `index.vue` feloldás.
    #[test]
    fn monorepo_keys_are_workspace_relative_with_extends_references_and_vite_alias() {
        let d = temp("mono");
        for dir in [
            "common",
            "ui/src/lib",
            "ui/src/kit",
            "ui/src/widgets/Card",
            "svc/src",
        ] {
            fs::create_dir_all(d.join(dir)).unwrap();
        }
        fs::write(
            d.join("package.json"),
            "{\"workspaces\": [\"ui\", \"svc\"]}",
        )
        .unwrap();
        fs::write(
            d.join("tsconfig.base.json"),
            "{\"compilerOptions\": {\"baseUrl\": \".\", \"paths\": {\"@common/*\": [\"common/*\"]}}}",
        )
        .unwrap();
        fs::write(d.join("ui/package.json"), "{}").unwrap();
        fs::write(
            d.join("ui/tsconfig.json"),
            "{\"files\": [], \"references\": [{\"path\": \"./tsconfig.app.json\"}]}",
        )
        .unwrap();
        fs::write(
            d.join("ui/tsconfig.app.json"),
            "{\"extends\": \"../tsconfig.base\", \"compilerOptions\": {\"baseUrl\": \".\", \"paths\": {\"@/*\": [\"./src/missing/*\", \"./src/*\"], \"@common/*\": [\"../common/*\"]}}}",
        )
        .unwrap();
        fs::write(
            d.join("ui/vite.config.ts"),
            "import { fileURLToPath, URL } from 'node:url';\nexport default { resolve: { alias: { '@kit': fileURLToPath(new URL('./src/kit', import.meta.url)), 'wid': path.resolve(__dirname, 'src/widgets') } } };\n",
        )
        .unwrap();
        fs::write(
            d.join("svc/tsconfig.json"),
            "{\"extends\": \"../tsconfig.base.json\"}",
        )
        .unwrap();
        fs::write(d.join("svc/package.json"), "{}").unwrap();
        fs::write(d.join("common/calendar.ts"), "").unwrap();
        fs::write(d.join("ui/src/lib/seed.ts"), "").unwrap();
        fs::write(d.join("ui/src/lib/panel.tsx"), "").unwrap();
        fs::write(d.join("ui/src/kit/Tag.vue"), "").unwrap();
        fs::write(d.join("ui/src/widgets/Card/index.vue"), "").unwrap();
        let r = Resolver::new();
        let ui = d.join("ui/src/lib/use.ts");
        let svc = d.join("svc/src/rest.ts");
        assert_eq!(r.module_path(&ui), "ui/src/lib/use.ts");
        assert_eq!(r.display("@common/calendar", &ui), "common/calendar.ts");
        assert_eq!(r.display("@common/calendar", &svc), "common/calendar.ts");
        assert_eq!(
            r.display("../../../common/calendar", &ui),
            "common/calendar.ts"
        );
        assert_eq!(r.display("./seed", &ui), "ui/src/lib/seed.ts");
        assert_eq!(r.display("@/lib/seed", &ui), "ui/src/lib/seed.ts");
        assert_eq!(r.display("./panel", &ui), "ui/src/lib/panel.tsx");
        assert_eq!(r.display("@kit/Tag.vue", &ui), "ui/src/kit/Tag.vue");
        assert_eq!(r.display("wid/Card", &ui), "ui/src/widgets/Card/index.vue");
        assert!(!r.resolve("vue", &ui).local);
        assert!(!r.resolve("@kit-other/x", &ui).local);
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn js_extension_in_a_ts_project_resolves_to_the_ts_file() {
        let d = temp("tsjs");
        fs::create_dir_all(d.join("src")).unwrap();
        fs::write(d.join("package.json"), "{}").unwrap();
        fs::write(d.join("src/a.ts"), "").unwrap();
        let r = Resolver::new();
        assert_eq!(r.display("./a.js", &d.join("src/b.ts")), "src/a.ts");
        let _ = fs::remove_dir_all(d);
    }
}
