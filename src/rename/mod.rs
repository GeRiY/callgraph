//! Biztonságos átnevezés (`callgraph rename <fajta> <régi> <új>`, MCP `rename`): egy parancs, ami a teljes projekten
//! átnevez, fájlonként egy rövid sort ad vissza, mentéssel és egyetlen `undo`-val. A mérés szerint az átnevezés a
//! leggyengébb pont: az ágens `grep`-pel keres, majd soronként `Edit`-el ír (sok kimeneti token, sok hívás).
//!
//! Fajták:
//! - **route** (`route.rs`): Laravel route-név — `->name('x')` definíció (csoport-előtag is, `'a.*' 'b.*'`
//!   alakkal) és a PHP / Blade / JS / Vue `route('x')` hivatkozások;
//! - **class** (`class.rs`): PHP-osztály — deklaráció, PSR-4 fájl-áthelyezés, névtér, `use`, rövid és teljes nevek,
//!   `::class`, típusjelzések, docblock; a sztring-hivatkozás csak jelzés (`--strings`: átírja), és az Eloquent
//!   táblanév-változásról figyelmeztet (`--keep-table`: megtartja);
//! - **method** (`method.rs`): PHP-metódus — deklaráció (leszármazottak, implementálók), a projekt-elemző által
//!   feloldott hívások, callable tömbök és route-akciók; a feloldatlan fogadójú hívás csak jelzés.
//!
//! Tervezési minták:
//! - **Strategy** — a három tervező ugyanazt adja: `Plan` (szerkesztések, jelzések, fájl-áthelyezések);
//! - **Command / Memento** — a `sites::finish` a generátor `WriteGroup`-ját használja: mentés + napló, így egyetlen
//!   `callgraph undo` az összes fájlt (az áthelyezettet is) visszaállítja;
//! - **Fail-fast** — minden módosított PHP-fájlt mago, minden Vue / JS fájlt oxc ellenőriz írás előtt; bármely hiba
//!   → semmi nem íródik. A fájlrendszer a generátor DI-felületén (`FileSystem`) megy, így a tesztek memóriában futnak.
//!
//! A szerkesztés bájt-pontos: csak az azonosító / literál tartománya változik, minden más érintetlen.

mod class;
mod method;
mod php;
mod route;
mod sites;

#[cfg(test)]
mod tests;

use crate::generate::fs::Env;
use crate::generate::{GenError, GenResult, Output};

/// Mit nevezünk át.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Route,
    Class,
    Method,
}

impl Kind {
    /// A parancssori / MCP név alapján.
    pub fn parse(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "route" => Some(Self::Route),
            "class" => Some(Self::Class),
            "method" => Some(Self::Method),
            _ => None,
        }
    }
}

/// Egy átnevezési kérés.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenameRequest {
    pub kind: Kind,
    pub old: String,
    pub new: String,
    /// Csak jelentés: semmi nem íródik.
    pub dry: bool,
    /// Osztály: a sztring-hivatkozásokat (`'App\Models\Fee'`) is átírja.
    pub strings: bool,
    /// Osztály: az Eloquent táblanevet megtartja (`protected $table`).
    pub keep_table: bool,
}

/// A Facade: a CLI és az MCP ezt hívja.
pub struct Renamer<'a> {
    env: Env<'a>,
}

impl<'a> Renamer<'a> {
    pub fn new(env: Env<'a>) -> Self {
        Self { env }
    }

    /// A kérés végrehajtása: terv → ellenőrzés → egy tranzakcióban írás (vagy `--dry`: csak jelentés).
    pub fn run(&self, request: &RenameRequest) -> GenResult<Output> {
        if request.old.trim().is_empty() || request.new.trim().is_empty() {
            return Err(GenError::new(
                crate::t!(
                    "both the old and the new name are required",
                    "a régi és az új név is kell"
                )
                .to_owned(),
            ));
        }
        let (what, plan) = match request.kind {
            Kind::Route => route::plan(self.env, request)?,
            Kind::Class => class::plan(self.env, request)?,
            Kind::Method => method::plan(self.env, request)?,
        };
        sites::finish(self.env, &what, plan, request.dry)
    }
}
