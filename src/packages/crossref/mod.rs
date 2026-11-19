//! Kapcsolatok a fájltípusok között: a fordítási kulcsok (`lang/*.json`, `lang/<nyelv>/*.php`) ↔ a `$t('a.b')` /
//! `__('x')` hívások (`i18n.rs`), és a stíluslapok osztály-szelektorai ↔ a sablonok `class` használata
//! (`classes.rs`). A nézetek (`--format i18n`, `--format classes`) és az outline rövid sorai (nyelvfájl-fejléc,
//! `⚠ missing in hu.json` a hívás mellett, `classes: unused …` a stíluslapnál) ugyanabból a projekt-szintű
//! beolvasásból jönnek; a beolvasás csak akkor fut, ha a kimenet kéri.
//!
//! - `i18n.rs` / `i18n_scan.rs`: a nyelvi halmazok, a hívás-olvasó, a PHP nyelvi tömb olvasója;
//! - `classes.rs` / `classes_scan.rs`: a definiált osztályok és a (lenient) használat-gyűjtés;
//! - `theme.rs`: a Tailwind 4 `@theme` tokenek és az általuk definiált utility-osztályok;
//! - `render.rs`: a nézetek és az outline sorai.
//!
//! Minden hibatűrő: ami nem olvasható / nem ismerhető fel, az kimarad, soha nem hiba. A dinamikus kulcs / osztály
//! (összefűzés, sablon-literál) soha nem riasztás.

mod classes;
mod classes_scan;
mod i18n;
pub(crate) use i18n::is_lang_path;
mod i18n_scan;
mod i18n_script;
mod render;
mod theme;
mod views;

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::io::FileWalker;
use crate::query::QueryResult;

pub(crate) use classes::Classes;
pub(crate) use i18n::I18n;
pub(crate) use views::Views;

/// A beolvasott fájltípusok.
const EXTS: [&str; 12] = [
    "vue", "js", "jsx", "ts", "tsx", "mjs", "php", "html", "htm", "css", "scss", "json",
];

/// A használatot kereső forrás-kiterjesztések.
const SOURCE_EXTS: [&str; 9] = ["vue", "js", "jsx", "ts", "tsx", "mjs", "php", "html", "htm"];

/// Ennél nagyobb fájlt nem olvasunk be.
const MAX_BYTES: u64 = 1_500_000;

/// Mennyit olvasson az i18n-ből.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum I18nNeed {
    #[default]
    None,
    /// Csak a nyelvfájlok (a hívás-figyelmeztetéshez).
    Defs,
    /// A nyelvfájlok és a forrásokban a használatok.
    Full,
}

/// Mit kell beolvasni.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Need {
    pub(crate) i18n: I18nNeed,
    pub(crate) classes: bool,
    /// A Blade-nézetek használata (ki rendereli, ki hivatkozik rá).
    pub(crate) views: bool,
}

/// A nézet hatóköre: egy lekérdezett fájl (a gyökérhez relatív út) és a kért nevek (`fájl::név`).
#[derive(Debug, Clone)]
pub(crate) struct Scope {
    pub(crate) rel: String,
    pub(crate) names: Option<Vec<String>>,
}

/// A projekt-szintű kapcsolatok.
#[derive(Debug, Default)]
pub(crate) struct CrossRef {
    root: PathBuf,
    canonical: Option<PathBuf>,
    pub(crate) i18n: Option<I18n>,
    pub(crate) classes: Option<Classes>,
    pub(crate) views: Option<Views>,
}

fn read(path: &Path) -> Option<String> {
    let size = std::fs::metadata(path).ok()?.len();
    if size > MAX_BYTES {
        return None;
    }
    std::fs::read_to_string(path).ok()
}

impl CrossRef {
    /// A projekt beolvasása a kért mértékben.
    pub(crate) fn open(root: &Path, need: Need) -> Self {
        let canonical = std::fs::canonicalize(root).ok();
        let base = canonical.clone().unwrap_or_else(|| root.to_path_buf());
        let walker = FileWalker::for_extensions(EXTS.to_vec());
        let files: Vec<(String, String, PathBuf)> = walker
            .collect(&[root.to_path_buf()], &mut |_| {})
            .into_iter()
            .filter_map(|path| {
                let ext = path.extension()?.to_str()?.to_ascii_lowercase();
                let rel = path
                    .strip_prefix(&base)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                Some((rel, ext, path))
            })
            .collect();
        let mut i18n = (need.i18n != I18nNeed::None).then(I18n::default);
        let mut classes = need.classes.then(Classes::default);
        if let Some(classes) = classes.as_mut()
            && std::fs::read_to_string(base.join("package.json"))
                .is_ok_and(|text| text.contains("\"tailwindcss\""))
        {
            classes.tailwind = true;
        }
        for (rel, ext, path) in &files {
            if let Some(classes) = classes.as_mut()
                && rel
                    .rsplit('/')
                    .next()
                    .is_some_and(|name| name.starts_with("tailwind.config."))
            {
                classes.tailwind = true;
            }
            match ext.as_str() {
                "json" | "php" | "ts" | "js" | "mjs" => {
                    if let Some(i18n) = i18n.as_mut()
                        && i18n::is_lang_path(rel)
                        && let Some(text) = read(path)
                    {
                        i18n.add_lang(rel, &text);
                    }
                }
                "css" | "scss" => {
                    if let Some(classes) = classes.as_mut()
                        && let Some(text) = read(path)
                    {
                        classes.add_stylesheet(rel, &text);
                    }
                }
                _ => {}
            }
        }
        let views = need.views.then(|| {
            let mut views = Views::default();
            for (rel, ext, path) in &files {
                if ext != "php" || i18n::is_lang_path(rel) {
                    continue;
                }
                let Some(text) = read(path) else { continue };
                if rel.ends_with(".blade.php") {
                    views.add_blade(rel, &text);
                } else {
                    views.add_php(rel, &text);
                }
            }
            views
        });
        // nyelvfájl nélkül nincs mit használatnak minősíteni: a forrás-olvasás kimarad
        let scan_i18n = need.i18n == I18nNeed::Full && i18n.as_ref().is_some_and(I18n::has_sets);
        let scan_classes = classes.as_ref().is_some_and(Classes::has_targets);
        if scan_i18n || scan_classes {
            let defined = classes.as_ref().map(Classes::defined).unwrap_or_default();
            for (rel, ext, path) in &files {
                if !SOURCE_EXTS.contains(&ext.as_str()) || i18n::is_lang_path(rel) {
                    continue;
                }
                let Some(text) = read(path) else { continue };
                if scan_i18n && let Some(i18n) = i18n.as_mut() {
                    i18n.add_source(rel, ext, &text);
                }
                if scan_classes && let Some(classes) = classes.as_mut() {
                    classes.add_source(rel, &text, &defined);
                }
            }
        }
        if let Some(i18n) = i18n.as_mut() {
            i18n.finish();
        }
        if let Some(classes) = classes.as_mut() {
            classes.finish();
        }
        Self {
            root: root.to_path_buf(),
            canonical,
            i18n,
            classes,
            views,
        }
    }

    /// A fájl a gyökérhez relatív, `/` elválasztós útja; a gyökéren kívüli abszolút út `None`.
    pub(crate) fn rel(&self, file: &str) -> Option<String> {
        let path = Path::new(file);
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
}

/// A sorok hatóköre (a lekérdezett fájlok és a kért nevek).
pub(crate) fn scopes(rows: &[QueryResult]) -> Vec<Scope> {
    rows.iter()
        .filter_map(|row| {
            let crossref = row.crossref.as_ref()?;
            Some(Scope {
                rel: crossref.rel(&row.graph.file)?,
                names: row.methods.clone().filter(|names| !names.is_empty()),
            })
        })
        .collect()
}

/// Mit kér a kimenet: a nézetek mindent a saját területükön; az outline csak ott, ahol a sor segít (nyelvfájl,
/// `$t('…')` hívás, stíluslap).
fn need_for(rows: &[QueryResult], format: &str) -> Need {
    let mut need = Need::default();
    match format {
        "i18n" => need.i18n = I18nNeed::Full,
        "classes" => need.classes = true,
        "outline" => {
            for row in rows {
                let file = row.graph.file.replace('\\', "/");
                if i18n::is_lang_path(&file) {
                    need.i18n = I18nNeed::Full;
                } else if need.i18n == I18nNeed::None
                    && row.graph.calls.iter().any(|call| {
                        call.category == "i18n_helper"
                            && call
                                .resolved
                                .as_deref()
                                .is_some_and(|resolved| resolved.starts_with("i18n::"))
                    })
                {
                    need.i18n = I18nNeed::Defs;
                }
                if (file.ends_with(".css") || file.ends_with(".scss")) && rows.len() <= 20 {
                    need.classes = true;
                }
                // a provider registers `Blade::directive(..)`: its outline lists them
                if (file.ends_with(".blade.php") || file.contains("Providers/")) && rows.len() <= 20
                {
                    need.views = true;
                }
            }
        }
        _ => {}
    }
    need
}

/// A sorok a projekt-szintű kapcsolatokkal, ha a kimenet kéri (a `format` szerint); különben érintetlenek.
pub(crate) fn attach(rows: &mut [QueryResult], root: &Path, format: &str) {
    if rows.is_empty() {
        return;
    }
    let need = need_for(rows, format);
    if need.i18n == I18nNeed::None && !need.classes && !need.views {
        return;
    }
    let shared = Arc::new(CrossRef::open(root, need));
    for row in rows {
        row.crossref = Some(Arc::clone(&shared));
    }
}
