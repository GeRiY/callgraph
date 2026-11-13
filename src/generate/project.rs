//! Projekt-ismeret, hálózat nélkül: a névtér a `composer.json` `autoload.psr-4` térképéből (a `src/packages` modultól
//! függetlenül, itt kicsiben), és a projekt stílusa (`declare(strict_types=1)`, ha a PHP-fájlok többsége használja).

use std::path::Path;

use super::fs::{FileSystem, walk_php};

/// Hány fájlt mintázunk a stílus megállapításához.
const STYLE_SAMPLE: usize = 50;

/// A `composer.json` PSR-4 térképe: `(névtér-előtag `\` nélkül, mappa `/` nélkül a végén)` párok.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Psr4 {
    entries: Vec<(String, String)>,
}

impl Psr4 {
    /// A projekt-gyökér `composer.json`-ja (`autoload` + `autoload-dev`); hiányzó / hibás fájl → üres térkép.
    pub fn load(fs: &dyn FileSystem, root: &Path) -> Self {
        fs.read(&root.join("composer.json"))
            .map(|text| Self::parse(&text))
            .unwrap_or_default()
    }

    pub fn parse(json: &str) -> Self {
        let mut entries = Vec::new();
        let Ok(doc) = serde_json::from_str::<serde_json::Value>(json) else {
            return Self::default();
        };
        for section in ["autoload", "autoload-dev"] {
            let Some(map) = doc[section]["psr-4"].as_object() else {
                continue;
            };
            for (prefix, dirs) in map {
                let dirs: Vec<&str> = match dirs {
                    serde_json::Value::String(d) => vec![d.as_str()],
                    serde_json::Value::Array(items) => {
                        items.iter().filter_map(|d| d.as_str()).collect()
                    }
                    _ => Vec::new(),
                };
                for dir in dirs {
                    let dir = dir.trim_start_matches("./").trim_end_matches('/');
                    entries.push((prefix.trim_end_matches('\\').to_owned(), dir.to_owned()));
                }
            }
        }
        Self { entries }
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// A PSR-4 gyökérmappák (a projekt-gyökérhez képest); térkép nélkül maga a gyökér (`""`).
    pub fn roots(&self) -> Vec<String> {
        if self.entries.is_empty() {
            return vec![String::new()];
        }
        let mut dirs: Vec<String> = self.entries.iter().map(|(_, d)| d.clone()).collect();
        dirs.dedup();
        dirs
    }

    /// A legjobban (leghosszabb mappa-előtaggal) illeszkedő bejegyzés: `(előtag, mappa)`.
    fn best(&self, rel: &str) -> Option<&(String, String)> {
        self.entries
            .iter()
            .filter(|(_, dir)| dir.is_empty() || rel.starts_with(&format!("{dir}/")))
            .max_by_key(|(_, dir)| dir.len())
    }

    /// A fájl névtere az útjából (`app/Services/Pay.php` → `App\Services`); `rel`: a gyökérhez képesti, `/`-es út.
    pub fn namespace_of(&self, rel: &str) -> Option<String> {
        let (prefix, dir) = self.best(rel)?;
        let tail = if dir.is_empty() {
            rel
        } else {
            &rel[dir.len() + 1..]
        };
        let mut segments: Vec<&str> = tail.split('/').collect();
        segments.pop();
        let mut ns = prefix.clone();
        for segment in segments {
            if !ns.is_empty() {
                ns.push('\\');
            }
            ns.push_str(segment);
        }
        (!ns.is_empty()).then_some(ns)
    }

    /// A fájl osztályának teljes neve (`app/Models/Order.php` → `App\Models\Order`).
    pub fn fqcn_of(&self, rel: &str) -> Option<String> {
        let ns = self.namespace_of(rel)?;
        let stem = rel.rsplit('/').next()?.strip_suffix(".php")?;
        Some(format!("{ns}\\{stem}"))
    }
}

/// Az útvonal `/`-es, a gyökérhez képesti alakja (a gyökéren kívüli út → `None`).
pub fn relative(root: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
    Some(
        rel.components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/"),
    )
}

/// `declare(strict_types=1)` a mintázott PHP-fájlok többségében (nincs minta → igen, a modern alapérték).
pub fn detect_strict_types(fs: &dyn FileSystem, root: &Path, psr4: &Psr4) -> bool {
    let (mut sampled, mut strict) = (0usize, 0usize);
    let files = psr4
        .roots()
        .into_iter()
        .flat_map(|dir| walk_php(fs, root.join(dir)))
        .take(STYLE_SAMPLE);
    for file in files {
        let Ok(text) = fs.read(&file) else {
            continue;
        };
        sampled += 1;
        if text.contains("strict_types=1") || text.contains("strict_types = 1") {
            strict += 1;
        }
    }
    sampled == 0 || strict * 2 > sampled
}
