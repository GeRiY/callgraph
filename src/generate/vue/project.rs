//! Projekt-ismeret a Vue-generátornak, a fájlrendszer DI-felületén át: TypeScript-e a projekt (`tsconfig.json` vagy
//! `lang="ts"` a meglévő `.vue` fájlokban), pontosvesszős-e a stílus, és hol van a `<Név>.vue` komponens (az
//! `@/…` alias a `resources/js` vagy a `src` mappára mutat).

use std::cell::{OnceCell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::LazyLock;

use regex::Regex;

use super::edit::indent_unit;
use crate::generate::edit::split_lines;
use crate::generate::fs::{FileSystem, MemoWalk};

/// Hány `.vue` fájlt mintázunk a stílushoz.
const SAMPLE: usize = 30;
/// A komponens-keresés gyökerei (az `@/` alias célja).
const ROOTS: [&str; 2] = ["resources/js", "src"];

/// Az alias-beállítások fájljai (a `tsconfig` `paths` / a Vite `resolve.alias`).
const ALIAS_FILES: [&str; 6] = [
    "tsconfig.json",
    "tsconfig.app.json",
    "jsconfig.json",
    "vite.config.ts",
    "vite.config.js",
    "vite.config.mjs",
];

/// A projekt kódstílusa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodeStyle {
    pub ts: bool,
    pub semi: bool,
}

/// A projekt-ismeret forrása. A bejárásokat és a beolvasott fájlokat memoizálja: egy példány (egy kérés) minden mappát
/// legfeljebb egyszer listáz, és minden fájlt legfeljebb egyszer olvas, akárhány kérdés érkezik.
pub struct VueProject<'a> {
    fs: &'a dyn FileSystem,
    root: &'a Path,
    walks: RefCell<HashMap<PathBuf, Rc<MemoWalk<'a>>>>,
    texts: RefCell<HashMap<PathBuf, Option<Rc<String>>>>,
    sample_roots: OnceCell<Vec<PathBuf>>,
    sample: OnceCell<Vec<PathBuf>>,
    aliases: OnceCell<Vec<(String, PathBuf)>>,
}

/// A beállítás-szövegből `(alias-előtag, a gyökérhez relatív cél)` párok: `"@ui/*": ["ui/src/*"]` (tsconfig, a
/// `baseUrl`-lel), `'@ui': path.resolve(__dirname, 'ui/src')` / `{ find: '@ui', replacement: '…' }` (Vite). Az
/// egyszerű `@` (a `resources/js` / `src`) kimarad. Csak literál értékek; más: kimarad.
fn parse_aliases(file: &str, text: &str) -> Vec<(String, String)> {
    static TS_PATH: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#""([^"*]+?)/\*"\s*:\s*\[\s*"([^"]*?)/?\*""#).expect("valid regex")
    });
    static BASE_URL: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#""baseUrl"\s*:\s*"([^"]+)""#).expect("valid regex"));
    static VITE_KV: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r#"['"](@[\w./-]*)['"]\s*:\s*[^}
]*?['"]([^'"]+)['"]"#,
        )
        .expect("valid regex")
    });
    static VITE_FIND: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r#"find:\s*['"]([^'"]+)['"][^}]*?replacement:\s*[^}
]*?['"]([^'"]+)['"]"#,
        )
        .expect("valid regex")
    });
    let clean = |path: &str| {
        path.trim_start_matches("./")
            .trim_start_matches('/')
            .trim_end_matches('/')
            .to_owned()
    };
    let mut out: Vec<(String, String)> = Vec::new();
    if file.ends_with(".json") {
        let base = BASE_URL
            .captures(text)
            .map(|c| clean(&c[1]))
            .filter(|b| !b.is_empty() && b != ".");
        for found in TS_PATH.captures_iter(text) {
            let target = clean(&found[2]);
            let target = match &base {
                Some(base) => format!("{base}/{target}"),
                None => target,
            };
            out.push((found[1].to_owned(), target));
        }
    } else {
        for found in VITE_KV
            .captures_iter(text)
            .chain(VITE_FIND.captures_iter(text))
        {
            out.push((found[1].to_owned(), clean(&found[2])));
        }
    }
    out.retain(|(prefix, target)| prefix != "@" && !target.is_empty());
    out
}

fn skipped_dir(name: &str) -> bool {
    name.starts_with('.')
        || matches!(
            name,
            "vendor" | "node_modules" | "storage" | "cache" | "dist"
        )
}

/// A mappa `.vue` fájljai lustán (mélységi bejárás, mappánként rendezve).
fn walk_vue<'a>(fs: &'a dyn FileSystem, dir: PathBuf) -> impl Iterator<Item = PathBuf> + 'a {
    let mut stack = vec![dir];
    let mut pending: std::vec::IntoIter<PathBuf> = Vec::new().into_iter();
    std::iter::from_fn(move || {
        loop {
            if let Some(path) = pending.next() {
                return Some(path);
            }
            let current = stack.pop()?;
            let Ok(mut entries) = fs.list(&current) else {
                continue;
            };
            entries.sort_by(|a, b| a.path.cmp(&b.path));
            let mut files = Vec::new();
            for entry in entries.into_iter().rev() {
                let name = entry
                    .path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_owned();
                if entry.is_dir {
                    if !skipped_dir(&name) {
                        stack.push(entry.path);
                    }
                } else if name.ends_with(".vue") {
                    files.push(entry.path);
                }
            }
            files.reverse();
            pending = files.into_iter();
        }
    })
}

/// A script-blokk nyitó tagje `lang="ts"`-t tartalmaz-e.
pub fn script_is_ts(text: &str) -> bool {
    text.lines().any(|l| {
        let t = l.trim_start();
        t.starts_with("<script") && (t.contains("lang=\"ts\"") || t.contains("lang='ts'"))
    })
}

/// Pontosvesszős-e a szöveg: az `import` / `const` sorok többsége `;`-re végződik (nincs minta → `None`).
pub fn uses_semicolons(text: &str) -> Option<bool> {
    let (mut with, mut without) = (0usize, 0usize);
    for line in text.lines() {
        let t = line.trim_end();
        let single = t.starts_with("import ") && (t.contains(" from ") || t.contains('\''));
        let decl =
            t.starts_with("const ") && t.contains(" = ") && !t.ends_with(['{', '(', '[', ',']);
        if single || decl {
            if t.ends_with(';') {
                with += 1;
            } else {
                without += 1;
            }
        }
    }
    (with + without > 0).then_some(with >= without)
}

impl<'a> VueProject<'a> {
    pub fn new(fs: &'a dyn FileSystem, root: &'a Path) -> Self {
        Self {
            fs,
            root,
            walks: RefCell::default(),
            texts: RefCell::default(),
            sample_roots: OnceCell::new(),
            sample: OnceCell::new(),
            aliases: OnceCell::new(),
        }
    }

    /// Az import-aliasok `(előtag, létező mappa)`: a `tsconfig` `paths` és a Vite `resolve.alias` literáljaiból.
    pub fn aliases(&self) -> &[(String, PathBuf)] {
        self.aliases.get_or_init(|| {
            let mut out: Vec<(String, PathBuf)> = Vec::new();
            for name in ALIAS_FILES {
                let Ok(text) = self.fs.read(&self.root.join(name)) else {
                    continue;
                };
                for (prefix, target) in parse_aliases(name, &text) {
                    let dir = self.root.join(target);
                    if !out.iter().any(|(have, _)| *have == prefix) && self.fs.list(&dir).is_ok() {
                        out.push((prefix, dir));
                    }
                }
            }
            out
        })
    }

    /// A mintafájlok `.vue` importjainak forrásai (ahogy írva vannak), egyszer-egyszer.
    pub fn sample_vue_imports(&self) -> Vec<String> {
        static IMPORT: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(r#"import\s+[\w$]+\s+from\s+['"]([^'"]+\.vue)['"]"#).expect("valid regex")
        });
        let mut out: Vec<String> = Vec::new();
        for file in self.sample_files() {
            let Some(text) = self.text(file) else {
                continue;
            };
            for found in IMPORT.captures_iter(&text) {
                let source = found[1].to_owned();
                if !out.contains(&source) {
                    out.push(source);
                }
            }
        }
        out
    }

    /// Az alias-mappák komponensei `(import-út az alias-szal, fájl)`: a `components` almappa (ha van, különben a
    /// mappa) `.vue` fájljai, aliasonként legfeljebb `limit` darab.
    pub fn alias_components(&self, limit: usize) -> Vec<(String, PathBuf)> {
        let mut out = Vec::new();
        for (prefix, dir) in self.aliases() {
            let components = dir.join("components");
            let base = if self.fs.list(&components).is_ok() {
                components
            } else {
                dir.clone()
            };
            for file in self.walk(&base).iter().take(limit) {
                let Ok(rel) = file.strip_prefix(dir) else {
                    continue;
                };
                let rel: Vec<String> = rel
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect();
                out.push((format!("{prefix}/{}", rel.join("/")), file));
            }
        }
        out
    }

    /// A mappa `.vue` fájljainak memoizált, lusta bejárása.
    fn walk(&self, dir: &Path) -> Rc<MemoWalk<'a>> {
        let mut walks = self.walks.borrow_mut();
        let fs = self.fs;
        Rc::clone(
            walks
                .entry(dir.to_path_buf())
                .or_insert_with(|| Rc::new(MemoWalk::new(walk_vue(fs, dir.to_path_buf())))),
        )
    }

    /// A fájl szövege, legfeljebb egyszer olvasva (az olvasási hiba is megjegyződik).
    pub fn text(&self, path: &Path) -> Option<Rc<String>> {
        let mut texts = self.texts.borrow_mut();
        texts
            .entry(path.to_path_buf())
            .or_insert_with(|| self.fs.read(path).ok().map(Rc::new))
            .clone()
    }

    /// A gyökérhez képesti mappa `.vue` fájljai (legfeljebb `limit` darab), a memoizált bejárással; nem létező
    /// mappára üres.
    pub fn vue_files(&self, rel: &str, limit: usize) -> Vec<PathBuf> {
        self.walk(&self.root.join(rel)).iter().take(limit).collect()
    }

    /// Igaz-e valamelyik mintafájl (az első `SAMPLE` `.vue` fájl) szövegére a feltétel; a szövegeket a közös memó adja.
    pub fn sample_any(&self, pred: impl Fn(&str) -> bool) -> bool {
        self.sample_files()
            .iter()
            .any(|file| self.text(file).is_some_and(|text| pred(&text)))
    }

    fn sample_roots(&self) -> &[PathBuf] {
        self.sample_roots.get_or_init(|| {
            let mut roots: Vec<PathBuf> = ROOTS
                .iter()
                .map(|r| self.root.join(r))
                .filter(|p| self.fs.list(p).is_ok())
                .collect();
            if roots.is_empty() {
                roots.push(self.root.to_path_buf());
            }
            roots
        })
    }

    /// A stílus-mintavétel fájljai: az első `SAMPLE` `.vue` fájl a mintavételi gyökerek alatt (egyszer bejárva).
    fn sample_files(&self) -> &[PathBuf] {
        self.sample.get_or_init(|| {
            let mut files = Vec::new();
            for dir in self.sample_roots() {
                files.extend(self.walk(dir).iter().take(SAMPLE - files.len()));
                if files.len() >= SAMPLE {
                    break;
                }
            }
            files
        })
    }

    /// TypeScript-e a projekt, és pontosvesszős-e a stílusa.
    pub fn code_style(&self) -> CodeStyle {
        let mut ts = self.fs.exists(&self.root.join("tsconfig.json"));
        let (mut with, mut without) = (0usize, 0usize);
        for file in self.sample_files() {
            let Some(text) = self.text(file) else {
                continue;
            };
            ts |= script_is_ts(&text);
            match uses_semicolons(&text) {
                Some(true) => with += 1,
                Some(false) => without += 1,
                None => {}
            }
        }
        CodeStyle {
            ts,
            semi: with >= without,
        }
    }

    /// A projekt behúzás-egysége (2 vagy 4): az első olyan `.vue` fájlból, amelyben van behúzott sor; minta nélkül 4
    /// (a Laravel / Breeze alapértéke).
    pub fn indent(&self) -> usize {
        for file in self.sample_files() {
            let Some(text) = self.text(file) else {
                continue;
            };
            let (lines, _) = split_lines(&text);
            if lines.iter().any(|l| l.starts_with(' ')) {
                return indent_unit(&lines);
            }
        }
        4
    }

    /// Az `@/…` import célfájlja (`.ts`, `.js`, `/index.ts`, `/index.js`), szövegestől.
    fn read_module(&self, source: &str) -> Option<String> {
        let rest = source.strip_prefix("@/")?;
        if rest.ends_with(".vue") {
            return None;
        }
        for root in ROOTS {
            let base = self.root.join(root).join(rest);
            let mut candidates = vec![base.clone()];
            for suffix in [".ts", ".js"] {
                let mut name = base.clone().into_os_string();
                name.push(suffix);
                candidates.push(PathBuf::from(name));
            }
            candidates.push(base.join("index.ts"));
            candidates.push(base.join("index.js"));
            for path in candidates {
                // az alap-út maga (kiterjesztés nélkül) nem fájl: a `read` hibája átlépi
                if self.fs.exists(&path)
                    && let Ok(text) = self.fs.read(&path)
                {
                    return Some(text);
                }
            }
        }
        None
    }

    /// A cél modulnak nincs `export default`-ja, de van `name` nevű nevesített exportja (`export function useX`,
    /// `export const useX`, `export async function useX`)? Ha a fájl nem található, `false` (default import marad).
    pub fn exports_named_only(&self, source: &str, name: &str) -> bool {
        let Some(text) = self.read_module(source) else {
            return false;
        };
        let (mut has_default, mut has_named) = (false, false);
        for line in text.lines() {
            let t = line.trim_start();
            if t.starts_with("export default") {
                has_default = true;
                continue;
            }
            let Some(rest) = t.strip_prefix("export ") else {
                continue;
            };
            let rest = rest.trim_start();
            let rest = rest.strip_prefix("async ").unwrap_or(rest).trim_start();
            for keyword in ["function* ", "function ", "const ", "let "] {
                if let Some(after) = rest.strip_prefix(keyword)
                    && let Some(tail) = after.trim_start().strip_prefix(name)
                    && !tail
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_alphanumeric() || matches!(c, '_' | '$'))
                {
                    has_named = true;
                }
            }
        }
        has_named && !has_default
    }

    /// A `<Név>.vue` komponens import-útja (`@/Components/Card.vue`), ha a projektben van ilyen fájl.
    pub fn find_component(&self, name: &str) -> Option<String> {
        let wanted = format!("{name}.vue");
        for root in ROOTS {
            let base = self.root.join(root);
            let matches: Vec<PathBuf> = self
                .walk(&base)
                .iter()
                .filter(|p| {
                    p.file_name()
                        .is_some_and(|n| n.to_str() == Some(wanted.as_str()))
                })
                .take(20)
                .collect();
            let pick = matches
                .iter()
                .find(|p| {
                    p.to_string_lossy()
                        .to_ascii_lowercase()
                        .contains("components")
                })
                .or_else(|| matches.first());
            let Some(path) = pick else {
                continue;
            };
            let Ok(rel) = path.strip_prefix(&base) else {
                continue;
            };
            let rel: Vec<String> = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect();
            return Some(format!("@/{}", rel.join("/")));
        }
        None
    }
}
