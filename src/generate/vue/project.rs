//! Projekt-ismeret a Vue-generátornak, a fájlrendszer DI-felületén át: TypeScript-e a projekt (`tsconfig.json` vagy
//! `lang="ts"` a meglévő `.vue` fájlokban), pontosvesszős-e a stílus, és hol van a `<Név>.vue` komponens (az
//! `@/…` alias a `resources/js` vagy a `src` mappára mutat).

use std::path::{Path, PathBuf};

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
