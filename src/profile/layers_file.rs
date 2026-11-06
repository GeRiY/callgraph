//! A projekt saját elnevezései: az opcionális, commitolható `.callgraph-layers` fájl (a `.callgraphignore`
//! mintájára). Soronként `szerep = mappa, mappa`; `ignore-role = mappa`: a mappa fájljai nem kapnak szerepet.
//! A `#`-tel kezdődő sor megjegyzés. Az utak a fájl mappájához képest értendők.
//!
//! ```text
//! service     = app/Szolgaltatasok, app/Logic
//! repository  = app/Adateleres
//! ignore-role = app/Legacy
//! ```

use std::path::{Path, PathBuf};

use super::role::Role;

/// A fájl neve a projektben.
pub(crate) const LAYERS_FILE: &str = ".callgraph-layers";

/// Egy saját hozzárendelés: mappa-előtag (a projekt-gyökérhez képest, `/`-rel) → szerep; `None` = `ignore-role`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Mapping {
    pub prefix: String,
    pub role: Option<Role>,
}

/// A beolvasott hozzárendelések és a fel nem ismert sorok (figyelmeztetésnek).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Overrides {
    pub mappings: Vec<Mapping>,
    pub warnings: Vec<String>,
    /// Honnan jött (a profil megemlíti); `None`, ha nincs fájl.
    pub source: Option<PathBuf>,
}

impl Overrides {
    /// A gyökértől felfelé a legközelebbi `.callgraph-layers`; az útjai a gyökérhez képest (ami a gyökéren kívülre
    /// mutat, kimarad).
    pub(crate) fn for_root(root: &Path) -> Self {
        let mut at = Some(root);
        while let Some(dir) = at {
            let file = dir.join(LAYERS_FILE);
            if let Ok(text) = std::fs::read_to_string(&file) {
                let base = relative_base(root, dir);
                let mut overrides = parse(&text, base.as_deref());
                overrides.source = Some(file);
                return overrides;
            }
            at = dir.parent();
        }
        Self::default()
    }

    /// A relatív útra (`/`-rel) illeszkedő leghosszabb előtag hozzárendelése.
    pub(crate) fn lookup(&self, rel: &str) -> Option<&Mapping> {
        self.mappings
            .iter()
            .filter(|m| under(rel, &m.prefix))
            .max_by_key(|m| m.prefix.len())
    }
}

/// A `.callgraph-layers` mappájának útja a gyökér felől: a gyökérben `""`, feljebb a gyökér útja a fájl mappájához
/// képest (`Some("sub/proj")`): ekkor az előtagokból ez levágandó.
fn relative_base(root: &Path, dir: &Path) -> Option<String> {
    let rest = root.strip_prefix(dir).ok()?;
    Some(slashed(rest))
}

/// Egy út `/`-rel elválasztva (Windows-on is), a `.` szegmensek nélkül.
pub(crate) fn slashed(path: &Path) -> String {
    path.components()
        .filter_map(|c| match c {
            std::path::Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// `rel` a `prefix` mappában van-e (szegmens-határon).
fn under(rel: &str, prefix: &str) -> bool {
    prefix.is_empty()
        || rel == prefix
        || (rel.len() > prefix.len()
            && rel.starts_with(prefix)
            && rel.as_bytes()[prefix.len()] == b'/')
}

/// A fájl tartalma; `base`: a gyökér útja a fájl mappájától (`""` = a fájl a gyökérben).
pub(crate) fn parse(text: &str, base: Option<&str>) -> Overrides {
    let base = base.unwrap_or("");
    let mut out = Overrides::default();
    for (number, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((name, paths)) = line.split_once('=') else {
            out.warnings
                .push(format!("{LAYERS_FILE}:{}: {line}", number + 1));
            continue;
        };
        let name = name.trim().to_ascii_lowercase();
        let role = if name == "ignore-role" || name == "ignore" {
            None
        } else if let Some(role) = Role::parse(&name) {
            Some(role)
        } else {
            out.warnings
                .push(format!("{LAYERS_FILE}:{}: {name}", number + 1));
            continue;
        };
        for path in paths.split(',') {
            let path = path.trim().replace('\\', "/");
            let path = path.trim_start_matches("./").trim_matches('/');
            if path.is_empty() {
                continue;
            }
            // a gyökér feletti fájlnál: csak a gyökéren belüli előtagok, a gyökérhez képest
            let prefix = if base.is_empty() {
                Some(path.to_owned())
            } else if path == base {
                Some(String::new())
            } else {
                path.strip_prefix(&format!("{base}/")).map(str::to_owned)
            };
            if let Some(prefix) = prefix {
                out.mappings.push(Mapping { prefix, role });
            }
        }
    }
    out
}
