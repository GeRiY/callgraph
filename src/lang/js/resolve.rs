//! Import-forrás feloldás: jsconfig/tsconfig keresés (baseUrl, paths) és a modul-útvonalak (JE/JD is ezt használja).

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};

use jsonc_parser::{CollectOptions, ParseOptions, ast::Value, parse_to_ast};

/// Egy mappához tartozó feloldási konfiguráció (a legközelebbi jsconfig/tsconfig vagy package.json).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolveConfig {
    /// A projekt-gyökér: az a mappa, amihez a `display` és a `module` viszonyul.
    pub root: PathBuf,
    base: PathBuf,
    /// `paths` bejegyzések deklarációs sorrendben: (minta, első cél).
    paths: Vec<(String, String)>,
}

/// Egy feloldott import-forrás.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    /// A megjelenített forrás: lokális fájlnál a gyökérhez relatív út, csomagnál az eredeti specifikátor.
    pub display: String,
    /// `true`, ha a forrás a projekt egy (alias-szal vagy relatívan elért) útjára mutat.
    pub local: bool,
}

/// Mappánként gyorsítótárazott import-feloldó (szálbiztos, megosztható `Arc`-kal).
#[derive(Debug, Default)]
pub struct Resolver {
    cache: Mutex<HashMap<PathBuf, Arc<ResolveConfig>>>,
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

    /// A fájl projekt-gyökere.
    pub fn root(&self, file: &Path) -> PathBuf {
        self.config(file).root.clone()
    }

    /// A fájl útja a projekt-gyökérhez képest (a `module` mező).
    pub fn module_path(&self, file: &Path) -> String {
        relative(&self.root(file), &absolute(file))
    }

    /// Az import-forrás feloldása a `from` fájlból nézve.
    pub fn resolve(&self, spec: &str, from: &Path) -> Resolved {
        let cfg = self.config(from);
        let abs = if spec.starts_with('.') {
            let dir = absolute(from).parent().map(Path::to_path_buf);
            Some(normalize(&dir.unwrap_or_default().join(spec)))
        } else {
            cfg.alias_target(spec)
        };
        match abs {
            None => Resolved {
                display: spec.to_string(),
                local: false,
            },
            Some(abs) => {
                let file = try_file(&abs).unwrap_or(abs);
                Resolved {
                    display: relative(&cfg.root, &file),
                    local: true,
                }
            }
        }
    }

    /// A feloldott forrás megjelenített alakja.
    pub fn display(&self, spec: &str, from: &Path) -> String {
        self.resolve(spec, from).display
    }

    fn config_for_dir(&self, dir: &Path) -> Arc<ResolveConfig> {
        let mut visited = Vec::new();
        let mut cur = dir.to_path_buf();
        let found = loop {
            if let Some(hit) = self.lock().get(&cur) {
                break hit.clone();
            }
            visited.push(cur.clone());
            if let Some(cfg) = read_config(&cur) {
                break Arc::new(cfg);
            }
            let at_top = cur.parent().is_none();
            if cur.join("package.json").exists() || at_top {
                break Arc::new(ResolveConfig::plain(&cur));
            }
            cur = cur.parent().map(Path::to_path_buf).unwrap_or_default();
        };
        let mut cache = self.lock();
        for d in visited {
            cache.insert(d, found.clone());
        }
        found
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<PathBuf, Arc<ResolveConfig>>> {
        self.cache.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl ResolveConfig {
    fn plain(dir: &Path) -> Self {
        Self {
            root: dir.to_path_buf(),
            base: dir.to_path_buf(),
            paths: Vec::new(),
        }
    }

    /// Az első illeszkedő `paths` minta szerinti abszolút cél (a `*` végű minta előtag-illesztés).
    fn alias_target(&self, spec: &str) -> Option<PathBuf> {
        for (pattern, target) in &self.paths {
            let prefix = pattern.strip_suffix('*').unwrap_or(pattern);
            let hit = if pattern.ends_with('*') {
                spec.starts_with(prefix)
            } else {
                spec == pattern
            };
            if hit {
                let tail = &spec[prefix.len()..];
                let target = target.strip_suffix('*').unwrap_or(target);
                return Some(normalize(&self.base.join(format!("{target}{tail}"))));
            }
        }
        None
    }
}

/// A `dir`-ben lévő jsconfig.json, majd tsconfig.json; hibás tartalomnál üres `paths`.
fn read_config(dir: &Path) -> Option<ResolveConfig> {
    for name in ["jsconfig.json", "tsconfig.json"] {
        let file = dir.join(name);
        if !file.exists() {
            continue;
        }
        let text = std::fs::read_to_string(&file).unwrap_or_default();
        return Some(parse_config(dir, &text).unwrap_or_else(|| ResolveConfig::plain(dir)));
    }
    None
}

/// `compilerOptions.baseUrl` és `paths` (komment- és trailing-comma-tűrő olvasás).
fn parse_config(dir: &Path, text: &str) -> Option<ResolveConfig> {
    let ast = parse_to_ast(text, &CollectOptions::default(), &ParseOptions::default()).ok()?;
    let Value::Object(root) = ast.value? else {
        return None;
    };
    let mut cfg = ResolveConfig::plain(dir);
    let Some(Value::Object(options)) = root.get("compilerOptions").map(|p| &p.value) else {
        return Some(cfg);
    };
    if let Some(Value::StringLit(base)) = options.get("baseUrl").map(|p| &p.value) {
        cfg.base = normalize(&dir.join(base.value.as_ref()));
    }
    if let Some(Value::Object(paths)) = options.get("paths").map(|p| &p.value) {
        for prop in &paths.properties {
            let first = match &prop.value {
                Value::Array(a) => match a.elements.first() {
                    Some(Value::StringLit(s)) => s.value.to_string(),
                    _ => String::new(),
                },
                _ => String::new(),
            };
            cfg.paths.push((prop.name.as_str().to_string(), first));
        }
    }
    Some(cfg)
}

/// Kiterjesztés-próba: p, p.js, p.ts, p.vue, p.mjs, p/index.js, p/index.ts.
fn try_file(p: &Path) -> Option<PathBuf> {
    let s = p.to_string_lossy();
    let candidates = [
        p.to_path_buf(),
        PathBuf::from(format!("{s}.js")),
        PathBuf::from(format!("{s}.ts")),
        PathBuf::from(format!("{s}.vue")),
        PathBuf::from(format!("{s}.mjs")),
        p.join("index.js"),
        p.join("index.ts"),
    ];
    candidates.into_iter().find(|c| c.is_file())
}

fn absolute(p: &Path) -> PathBuf {
    normalize(&std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf()))
}

/// Lexikális normalizálás (`.` és `..` kezelése), mint a `path.resolve`.
fn normalize(p: &Path) -> PathBuf {
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

/// A `path.relative(from, to)` megfelelője (mindkét út abszolút, normalizált).
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

    #[test]
    fn relative_paths_climb_with_dotdot() {
        assert_eq!(relative(Path::new("/a/b"), Path::new("/a/c/d")), "../c/d");
        assert_eq!(relative(Path::new("/"), Path::new("/x/y")), "x/y");
    }
}
