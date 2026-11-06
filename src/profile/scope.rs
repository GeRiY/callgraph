//! A lekérdezések réteg-kapcsolói (`--layer`, `--layers`, `--by-layer`): a fájl szerepe az útjából, a
//! lekérdezéskor számolva (nincs tárolt adat, nincs formátumváltás).

use std::cmp::Reverse;
use std::path::{Path, PathBuf};

use super::classify::Classifier;
use super::layers_file::slashed;
use super::role::{Kind, Role};

/// A gyökér osztályozója és a kért réteg-szűrő / -sorrend.
#[derive(Debug, Clone)]
pub(crate) struct LayerScope {
    root: PathBuf,
    classifier: Classifier,
    filter: Vec<Role>,
    by_layer: bool,
}

/// A `--by-layer` rendezési kulcsa: előbb a hívásirányba számító rétegek (a hívotthoz legközelebbi, azaz a
/// nagyobb rangú elöl: egy repository hívói közül a service-ek, aztán a controllerek), utána a kiszolgáló fájlok
/// (teszt…), végül az ismeretlenek.
pub(crate) type OrderKey = (u8, Reverse<u8>);

impl LayerScope {
    /// `filter`: vesszős réteg-lista (`service,repository`); ismeretlen névnél a hibaüzenet az ismert nevekkel.
    pub(crate) fn new(root: &Path, filter: Option<&str>, by_layer: bool) -> Result<Self, String> {
        let mut roles = Vec::new();
        for name in filter
            .unwrap_or("")
            .split(',')
            .map(str::trim)
            .filter(|n| !n.is_empty())
        {
            let Some(role) = Role::parse(name) else {
                let known = Role::known_names();
                return Err(crate::tf!(
                    "Unknown layer: {name} (known: {known})",
                    "Ismeretlen réteg: {name} (ismertek: {known})"
                ));
            };
            roles.push(role);
        }
        Ok(Self {
            root: root.to_path_buf(),
            classifier: Classifier::for_root(root),
            filter: roles,
            by_layer,
        })
    }

    /// Kért-e rétegek szerinti sorrendet (`--by-layer`).
    pub(crate) fn by_layer(&self) -> bool {
        self.by_layer
    }

    /// Egy (abszolút vagy a gyökérhez relatív) út szerepe.
    pub(crate) fn role_of(&self, path: &Path) -> Option<Role> {
        let rel = path.strip_prefix(&self.root).unwrap_or(path);
        self.classifier.role_of(&slashed(rel))
    }

    /// Mint a `role_of`, a gráf-kulcs (az út szövegként) alapján.
    pub(crate) fn role_of_key(&self, key: &str) -> Option<Role> {
        self.role_of(Path::new(key))
    }

    /// A szűrő átengedi-e a fájlt (szűrő nélkül mindent).
    pub(crate) fn keeps(&self, path: &Path) -> bool {
        self.filter.is_empty() || self.role_of(path).is_some_and(|r| self.filter.contains(&r))
    }

    /// A `--by-layer` rendezési kulcsa a fájl szerepéből.
    pub(crate) fn order_key(&self, key: &str) -> OrderKey {
        match self.role_of_key(key) {
            Some(role) if role.in_flow() || role.kind() == Kind::Pattern => (0, Reverse(role.rank())),
            Some(role) => (1, Reverse(role.rank())),
            None => (2, Reverse(0)),
        }
    }

    /// A sor végére írt címke (`  [service]`); ismeretlen szerepnél üres.
    pub(crate) fn tag(&self, key: &str) -> String {
        self.role_of_key(key)
            .map(|role| format!("  [{role}]"))
            .unwrap_or_default()
    }
}
