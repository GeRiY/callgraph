//! Az Inertia-oldalak gyökere és a page-név ↔ fájl megfeleltetés. A gyökér a JS-belépőből jön (az a fájl, amely
//! `createInertiaApp`-ot hív): az `import.meta.glob('./Pages/**/*.vue')` (a `resolvePageComponent` második
//! argumentuma is ez) vagy a `` `./Pages/${name}.vue` `` sablon mappája és kiterjesztése; ha egyik sincs, a
//! `resources/js/Pages` / `resources/js/pages` mappa. A felismerés tiszta (a szövegből), a fájl-hozzáférés DI.

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use super::super::files::ProjectFiles;

/// A JS-belépő jelöltjei (a gyökérhez relatívan), ebben a sorrendben; az első, amely `createInertiaApp`-ot hív és
/// a page-mappája felismerhető, nyer.
const ENTRIES: &[&str] = &[
    "resources/js/app.ts",
    "resources/js/app.js",
    "resources/js/app.tsx",
    "resources/js/app.jsx",
    "resources/js/main.ts",
    "resources/js/main.js",
    "resources/js/ssr.ts",
    "resources/js/ssr.js",
    "resources/ts/app.ts",
];

/// A tartalék page-mappák (ha a belépőből nem derül ki), ebben a sorrendben; az első nem üres nyer.
const FALLBACK_DIRS: &[&str] = &["resources/js/Pages", "resources/js/pages"];

/// Az alapértelmezett page-kiterjesztések (ha a glob / sablon nem mondja meg).
const DEFAULT_EXTENSIONS: &[&str] = &["vue", "tsx", "jsx", "svelte"];

/// `import.meta.glob('<minta>')` / `import.meta.glob<…>(['<minta>'])`: az első minta.
static GLOB: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"import\.meta\.glob\s*(?:<[^>]*>)?\(\s*\[?\s*['"`]([^'"`]+)['"`]"#)
        .expect("érvényes regex")
});

/// `` `./Pages/${name}.vue` ``: a perjelre végződő előtag és a név utáni rész.
static TEMPLATE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"`([^`$]*/)\$\{[^}]*\}([^`]*)`").expect("érvényes regex"));

/// Az oldalak gyökere: a mappa (a projekt-gyökérhez relatív, `/`-rel, `./` nélkül) és a kiterjesztések (pont
/// nélkül, a felismerés sorrendjében — a név → fájl ebben a sorrendben keres).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PageRoot {
    pub dir: String,
    pub extensions: Vec<String>,
}

impl PageRoot {
    /// A page-név (`Users/Index`) lehetséges fájljai a gyökérhez relatívan (`resources/js/Pages/Users/Index.vue`),
    /// a kiterjesztések sorrendjében; érvénytelen névre (üres, abszolút, `..`) üres.
    pub(crate) fn candidates(&self, name: &str) -> Vec<String> {
        if !valid_name(name) {
            return Vec::new();
        }
        self.extensions
            .iter()
            .map(|ext| format!("{}/{name}.{ext}", self.dir))
            .collect()
    }

    /// A (gyökérhez relatív) fájl page-neve, ha a page-gyökér alatt van és ismert a kiterjesztése:
    /// `resources/js/Pages/Users/Index.vue` → `Users/Index`.
    pub(crate) fn page_name(&self, rel: &str) -> Option<String> {
        let inner = rel.strip_prefix(self.dir.as_str())?.strip_prefix('/')?;
        let (stem, ext) = inner.rsplit_once('.')?;
        let known = self.extensions.iter().any(|known| known == ext);
        (known && valid_name(stem)).then(|| stem.to_owned())
    }

    /// A kiírt rövid alak: a page-mappa neve és az alatta lévő út (`Pages/Users/Index.vue`); a gyökéren kívüli út
    /// változatlan.
    pub(crate) fn short(&self, rel: &str) -> String {
        let base = self.dir.rsplit('/').next().unwrap_or(&self.dir);
        match rel
            .strip_prefix(self.dir.as_str())
            .and_then(|inner| inner.strip_prefix('/'))
        {
            Some(inner) => format!("{base}/{inner}"),
            None => rel.to_owned(),
        }
    }
}

/// A projekt page-gyökere: a JS-belépőből, különben a tartalék mappákból; ha egyik sincs, `None`.
pub(crate) fn detect(root: &Path, files: &dyn ProjectFiles) -> Option<PageRoot> {
    for entry in ENTRIES {
        let Some(text) = files.read(&root.join(entry)) else {
            continue;
        };
        let dir = entry.rsplit_once('/').map_or("", |(dir, _)| dir);
        if let Some(found) = from_entry(&text, dir) {
            return Some(found);
        }
    }
    FALLBACK_DIRS
        .iter()
        .find(|dir| !files.files_under(&root.join(dir)).is_empty())
        .map(|dir| PageRoot {
            dir: (*dir).to_owned(),
            extensions: defaults(),
        })
}

/// A belépő szövegéből (`entry_dir`: a belépő mappája a gyökérhez relatívan) a page-gyökér, ha a fájl
/// `createInertiaApp`-ot hív és a glob vagy a sablon felismerhető.
pub(crate) fn from_entry(text: &str, entry_dir: &str) -> Option<PageRoot> {
    if !text.contains("createInertiaApp") {
        return None;
    }
    if let Some(found) = GLOB.captures(text) {
        let (dir, extensions) = split_glob(&found[1]);
        return Some(PageRoot {
            dir: resolve_dir(&dir, entry_dir)?,
            extensions: or_defaults(extensions),
        });
    }
    let found = TEMPLATE.captures(text)?;
    let extensions = found[2]
        .strip_prefix('.')
        .filter(|ext| is_extension(ext))
        .map(|ext| vec![ext.to_owned()])
        .unwrap_or_default();
    Some(PageRoot {
        dir: resolve_dir(found[1].trim_end_matches('/'), entry_dir)?,
        extensions: or_defaults(extensions),
    })
}

/// A glob-minta mappája (az első helyettesítő tag előtti rész) és kiterjesztései (`*.vue`, `*.{vue,tsx}`).
fn split_glob(pattern: &str) -> (String, Vec<String>) {
    let segments: Vec<&str> = pattern.split('/').collect();
    let wild = segments
        .iter()
        .position(|segment| segment.contains(['*', '{', '?', '[']))
        .unwrap_or(segments.len());
    let dir = segments[..wild].join("/");
    let last = segments.last().copied().unwrap_or_default();
    let extensions = last
        .rsplit_once('.')
        .map(|(_, ext)| {
            ext.trim_start_matches('{')
                .trim_end_matches('}')
                .split(',')
                .map(str::trim)
                .filter(|ext| is_extension(ext))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    (dir, extensions)
}

/// A glob / sablon mappája a gyökérhez relatívan: `./Pages` a belépő mappájához, `@/Pages` / `~/Pages` a
/// `resources/js`-hez, `/resources/js/Pages` a gyökérhez (a Vite gyökere); a gyökér fölé mutató vagy üres: `None`.
fn resolve_dir(dir: &str, entry_dir: &str) -> Option<String> {
    let joined = if let Some(rest) = dir.strip_prefix("@/").or_else(|| dir.strip_prefix("~/")) {
        format!("resources/js/{rest}")
    } else if dir == "@" || dir == "~" {
        "resources/js".to_owned()
    } else if let Some(rest) = dir.strip_prefix('/') {
        rest.to_owned()
    } else {
        format!("{entry_dir}/{dir}")
    };
    normalize(&joined).filter(|dir| !dir.is_empty())
}

/// `a/./b/../c` → `a/c`; a gyökér fölé mutató út: `None`.
fn normalize(path: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    Some(parts.join("/"))
}

/// Érvényes page-név: nem üres, relatív, `.` / `..` / üres tag és visszaperjel nélkül.
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && !name.contains('\\')
        && name
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

fn is_extension(ext: &str) -> bool {
    !ext.is_empty() && ext.chars().all(|c| c.is_ascii_alphanumeric())
}

fn defaults() -> Vec<String> {
    DEFAULT_EXTENSIONS
        .iter()
        .map(|ext| (*ext).to_owned())
        .collect()
}

fn or_defaults(extensions: Vec<String>) -> Vec<String> {
    if extensions.is_empty() {
        defaults()
    } else {
        extensions
    }
}
