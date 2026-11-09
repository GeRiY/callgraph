//! Egy réteg = szerep + ökoszisztéma (PHP, JS / TS / Vue, Swift, Rust): a backend `controller`-e és egy AngularJS
//! `controllers` mappa külön réteg, és a hívásirány-lánc sem köti össze őket. A kiírt név csak akkor kap minősítőt
//! (`controller (php)` / `controller (js)`), ha ugyanaz a szerep több ökoszisztémában is előfordul — egy
//! ökoszisztémás projektben a kimenet változatlan.

use super::role::Role;
use super::rules::Eco;

/// Egy réteg: szerep és ökoszisztéma (a rendezés a szerep, azon belül az ökoszisztéma szerint).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct Layer {
    pub role: Role,
    pub eco: Eco,
}

impl Layer {
    pub(crate) fn new(role: Role, eco: Eco) -> Self {
        Self { role, eco }
    }

    /// A kiírt név: `controller`, vagy ha a szerep több ökoszisztémában is van: `controller (js)`.
    pub(crate) fn label(self, mixed: &Mixed) -> String {
        if mixed.contains(self.role) {
            format!("{} ({})", self.role.name(), self.eco.name())
        } else {
            self.role.name().to_owned()
        }
    }
}

/// A több ökoszisztémában is előforduló szerepek (csak ezek kapnak minősítőt).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Mixed(Vec<Role>);

impl Mixed {
    /// A rétegek közül azok a szerepek, amelyek legalább két ökoszisztémában szerepelnek.
    pub(crate) fn of(layers: impl IntoIterator<Item = Layer>) -> Self {
        let mut seen: Vec<Layer> = layers.into_iter().collect();
        seen.sort_unstable();
        seen.dedup();
        let mut roles: Vec<Role> = seen
            .windows(2)
            .filter(|pair| pair[0].role == pair[1].role)
            .map(|pair| pair[0].role)
            .collect();
        roles.dedup();
        Self(roles)
    }

    pub(crate) fn contains(&self, role: Role) -> bool {
        self.0.contains(&role)
    }
}

/// Egy `--layer` szűrő-elem: `controller` (minden ökoszisztéma), `controller:js` vagy `controller (js)` (csak az az
/// ökoszisztéma; `php`, `js` / `ts` / `vue`, `swift`, `rust`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LayerFilter {
    pub role: Role,
    pub eco: Option<Eco>,
}

impl LayerFilter {
    /// A felhasználói alak értelmezése; `None`, ha a szerep vagy az ökoszisztéma ismeretlen.
    pub(crate) fn parse(input: &str) -> Option<Self> {
        let input = input.trim();
        let (name, eco) = if let Some(rest) = input.strip_suffix(')') {
            let (name, eco) = rest.split_once('(')?;
            (name, Some(eco))
        } else if let Some((name, eco)) = input.split_once(':') {
            (name, Some(eco))
        } else {
            (input, None)
        };
        let role = Role::parse(name)?;
        let eco = match eco {
            Some(eco) => Some(Eco::parse(eco)?),
            None => None,
        };
        Some(Self { role, eco })
    }

    /// Átengedi-e a réteget.
    pub(crate) fn matches(self, layer: Layer) -> bool {
        self.role == layer.role && self.eco.is_none_or(|eco| eco == layer.eco)
    }
}
