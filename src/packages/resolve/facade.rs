//! Láncszem: a `use` nélkül, rövid névvel hívott Laravel-homlokzat (`Http::get`, `Cache::remember`, `Route::get`) →
//! `laravel/framework` — csak ha a fájlhoz legközelebbi PHP-rész függ tőle (közvetlenül vagy a lockban). A
//! homlokzat-nevek a Laravel-stratégia ismert rövidnevei (a kategorizáló ugyanezzel a listával adja a
//! `framework_use`-t).

use super::{Attribution, Resolution, SymbolRef, SymbolResolver};
use crate::lang::php::framework::{FrameworkStrategy, LaravelStrategy};
use crate::packages::model::Ecosystem;
use crate::packages::project::EcosystemPackages;

/// A homlokzatokat adó csomag.
const LARAVEL: &str = "laravel/framework";

/// Az import nélküli Laravel-homlokzat.
pub(super) struct FacadeTable;

impl SymbolResolver for FacadeTable {
    fn resolve(&self, sym: &SymbolRef<'_>, part: &EcosystemPackages) -> Option<Resolution> {
        if sym.ecosystem != Ecosystem::Php {
            return None;
        }
        let alias = sym.alias?;
        if !LaravelStrategy.known_short_names().contains(&alias) {
            return None;
        }
        let laravel = part.package(LARAVEL)?;
        Some(Resolution::Package(Attribution::of(laravel)))
    }
}
