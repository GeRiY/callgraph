//! Framework-tények a fájl elemzésekor (R4): a nyelvi frontend a forrásból kinyeri, a nézetek csak összesítenek,
//! forrásolvasás nélkül. A `FileGraph` mellé kerülő opcionális rekord — a gráf JSON-ja változatlan (nem szerializálódik),
//! a pack rekordjába viszont bekerül (`graph/codec.rs`). Ma csak az Inertia render-helyek és a megosztott props.

use serde::{Deserialize, Serialize};

/// Egy Inertia render-hely a PHP-fájlban (`Inertia::render('X', [...])`, `inertia('X')`, `Route::inertia(..)`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderFact {
    /// A page-név (sima string-literál; a dinamikus név nem tény, lásd `FrameworkFacts::dynamic_lines`).
    pub page: String,
    /// A props tömb-literál kulcsai; `None` = nem statikus (változó, kifejezés, spread).
    pub prop_keys: Option<Vec<String>>,
    pub line: u32,
    /// A hívó konténere és metódusa (a hívás helye a `rendered by:` sorhoz).
    pub container: Option<String>,
    pub caller: Option<String>,
}

/// A `HandleInertiaRequests::share` kulcsai: minden oldalnak átadottak.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedFact {
    pub keys: Vec<String>,
    /// `false`, ha a `share` visszatérése nem olvasható végig statikusan (a hiányzó-prop jelzés ilyenkor kimarad).
    pub complete: bool,
}

/// Egy fájl framework-tényei.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameworkFacts {
    pub render_sites: Vec<RenderFact>,
    /// A render-hívások sorai, amelyek page-neve nem literál (a „nem renderelt” lista ilyenkor nem biztos).
    pub dynamic_lines: Vec<u32>,
    pub shared: Option<SharedFact>,
}

impl FrameworkFacts {
    /// Nincs benne semmi (ilyenkor a `FileGraph::facts` `None`).
    pub fn is_empty(&self) -> bool {
        self.render_sites.is_empty() && self.dynamic_lines.is_empty() && self.shared.is_none()
    }
}
