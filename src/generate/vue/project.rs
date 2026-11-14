//! Projekt-ismeret a Vue-generátornak, a fájlrendszer DI-felületén át: TypeScript-e a projekt (`tsconfig.json` vagy
//! `lang="ts"` a meglévő `.vue` fájlokban), pontosvesszős-e a stílus, és hol van a `<Név>.vue` komponens (az
//! `@/…` alias a `resources/js` vagy a `src` mappára mutat).

use std::path::{Path, PathBuf};

use super::edit::indent_unit;
use crate::generate::edit::split_lines;
use crate::generate::fs::FileSystem;

/// Hány `.vue` fájlt mintázunk a stílushoz.
const SAMPLE: usize = 30;
/// A komponens-keresés gyökerei (az `@/` alias célja).
const ROOTS: [&str; 2] = ["resources/js", "src"];

/// A projekt kódstílusa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodeStyle {
    pub ts: bool,
    pub semi: bool,
}

/// A projekt-ismeret forrása.
pub struct VueProject<'a> {
    fs: &'a dyn FileSystem,
    root: &'a Path,
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
        Self { fs, root }
    }

    fn sample_roots(&self) -> Vec<PathBuf> {
        let mut roots: Vec<PathBuf> = ROOTS
            .iter()
            .map(|r| self.root.join(r))
            .filter(|p| self.fs.list(p).is_ok())
            .collect();
        if roots.is_empty() {
            roots.push(self.root.to_path_buf());
        }
        roots
    }

    /// TypeScript-e a projekt, és pontosvesszős-e a stílusa.
    pub fn code_style(&self) -> CodeStyle {
        let mut ts = self.fs.exists(&self.root.join("tsconfig.json"));
        let (mut with, mut without) = (0usize, 0usize);
        let files = self
            .sample_roots()
            .into_iter()
            .flat_map(|dir| walk_vue(self.fs, dir))
            .take(SAMPLE);
        for file in files {
            let Ok(text) = self.fs.read(&file) else {
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
        let files = self
            .sample_roots()
            .into_iter()
            .flat_map(|dir| walk_vue(self.fs, dir))
            .take(SAMPLE);
        for file in files {
            let Ok(text) = self.fs.read(&file) else {
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
            let matches: Vec<PathBuf> = walk_vue(self.fs, base.clone())
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
