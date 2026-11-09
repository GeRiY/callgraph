//! A lekérdezések réteg-kapcsolói (`--layer`, `--layers`, `--by-layer`): a fájl rétege (szerep + ökoszisztéma) az
//! útjából, a lekérdezéskor számolva (nincs tárolt adat, nincs formátumváltás). A kód-jelet kívánó szerepeket
//! (`event`, `listener`, `job`) itt a mappanév adja — gráf nélkül nincs mivel megerősíteni.

use std::cmp::Reverse;
use std::ops::Range;
use std::path::{Path, PathBuf};

use super::classify::Classifier;
use super::layer::{Layer, LayerFilter, Mixed};
use super::layers_file::slashed;
use super::role::{Kind, Role};

/// A gyökér osztályozója és a kért réteg-szűrő / -sorrend.
#[derive(Debug, Clone)]
pub(crate) struct LayerScope {
    root: PathBuf,
    classifier: Classifier,
    filter: Vec<LayerFilter>,
    by_layer: bool,
}

/// A `--by-layer` rendezési kulcsa: előbb a hívásirányba számító rétegek (a hívotthoz legközelebbi, azaz a
/// nagyobb rangú — mélyebb — elöl: egy repository hívói közül a service-ek, aztán a controllerek), utána a
/// kiszolgáló fájlok (teszt…), végül az ismeretlenek; azonos rangon szerep és ökoszisztéma szerint, hogy egy réteg
/// sorai egy csoportban legyenek.
pub(crate) type OrderKey = (u8, Reverse<u8>, Option<Layer>);

/// Egy réteg-csoport a már rendezett sorokban: a kiírt név és a sorok tartománya.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LayerRun {
    pub label: String,
    pub range: Range<usize>,
}

impl LayerScope {
    /// `filter`: vesszős réteg-lista (`service,repository`, egy ökoszisztéma: `controller:js`); ismeretlen névnél a
    /// hibaüzenet az ismert nevekkel.
    pub(crate) fn new(root: &Path, filter: Option<&str>, by_layer: bool) -> Result<Self, String> {
        let mut layers = Vec::new();
        for name in filter
            .unwrap_or("")
            .split(',')
            .map(str::trim)
            .filter(|n| !n.is_empty())
        {
            let Some(layer) = LayerFilter::parse(name) else {
                let known = Role::known_names();
                return Err(crate::tf!(
                    "Unknown layer: {name} (known: {known}; one ecosystem only: controller:php, controller:js, …:swift, …:rust)",
                    "Ismeretlen réteg: {name} (ismertek: {known}; csak egy ökoszisztéma: controller:php, controller:js, …:swift, …:rust)"
                ));
            };
            layers.push(layer);
        }
        Ok(Self {
            root: root.to_path_buf(),
            classifier: Classifier::for_root(root),
            filter: layers,
            by_layer,
        })
    }

    /// Kért-e rétegek szerinti sorrendet (`--by-layer`).
    pub(crate) fn by_layer(&self) -> bool {
        self.by_layer
    }

    fn rel(&self, path: &Path) -> String {
        slashed(path.strip_prefix(&self.root).unwrap_or(path))
    }

    /// Egy (abszolút vagy a gyökérhez relatív) út szerepe.
    pub(crate) fn role_of(&self, path: &Path) -> Option<Role> {
        self.classifier.role_of(&self.rel(path))
    }

    /// Egy út rétege (szerep + ökoszisztéma).
    pub(crate) fn layer_of(&self, path: &Path) -> Option<Layer> {
        self.classifier.layer_of(&self.rel(path))
    }

    /// Mint a `role_of`, a gráf-kulcs (az út szövegként) alapján.
    pub(crate) fn role_of_key(&self, key: &str) -> Option<Role> {
        self.role_of(Path::new(key))
    }

    /// Mint a `layer_of`, a gráf-kulcs alapján.
    pub(crate) fn layer_of_key(&self, key: &str) -> Option<Layer> {
        self.layer_of(Path::new(key))
    }

    /// A szűrő átengedi-e a fájlt (szűrő nélkül mindent).
    pub(crate) fn keeps(&self, path: &Path) -> bool {
        self.filter.is_empty()
            || self
                .layer_of(path)
                .is_some_and(|layer| self.filter.iter().any(|f| f.matches(layer)))
    }

    /// A `--by-layer` rendezési kulcsa a fájl rétegéből.
    pub(crate) fn order_key(&self, key: &str) -> OrderKey {
        match self.layer_of_key(key) {
            Some(layer) if layer.role.in_flow() || layer.role.kind() == Kind::Pattern => {
                (0, Reverse(layer.role.rank()), Some(layer))
            }
            Some(layer) => (1, Reverse(layer.role.rank()), Some(layer)),
            None => (2, Reverse(0), None),
        }
    }

    /// A sor végére írt címke (`  [service]`); ismeretlen szerepnél üres. `mixed`: a kimenetben több
    /// ökoszisztémában is előforduló szerepek (ezek minősítőt kapnak).
    pub(crate) fn tag(&self, key: &str, mixed: &Mixed) -> String {
        self.layer_of_key(key)
            .map(|layer| format!("  [{}]", layer.label(mixed)))
            .unwrap_or_default()
    }

    /// A rétegek szerint már rendezett kulcsok egymás utáni csoportjai (a `--by-layer` fejléceihez); a név
    /// minősítőt kap, ha a szerep a kulcsok között több ökoszisztémában is előfordul.
    pub(crate) fn runs<'a>(&self, keys: impl IntoIterator<Item = &'a str>) -> Vec<LayerRun> {
        let layers: Vec<Option<Layer>> = keys.into_iter().map(|k| self.layer_of_key(k)).collect();
        let mixed = Mixed::of(layers.iter().flatten().copied());
        let mut runs: Vec<(Option<Layer>, Range<usize>)> = Vec::new();
        for (at, layer) in layers.iter().enumerate() {
            if let Some((last, range)) = runs.last_mut()
                && *last == *layer
            {
                range.end = at + 1;
                continue;
            }
            runs.push((*layer, at..at + 1));
        }
        runs.into_iter()
            .map(|(layer, range)| LayerRun {
                label: layer.map_or_else(
                    || crate::t!("unknown", "ismeretlen").to_owned(),
                    |l| l.label(&mixed),
                ),
                range,
            })
            .collect()
    }
}
