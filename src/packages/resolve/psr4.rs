//! Láncszem: a lock csomagjainak psr-4 / psr-0 névterei közül a leghosszabb illeszkedő előtag dönt — a közvetlen
//! függőségeké és a csak a lockban szereplő (tranzitív) csomagoké is (`Illuminate\…` → `laravel/framework`,
//! `Symfony\Component\HttpFoundation\…` → `symfony/http-foundation`, a tranzitív nevével).

use std::cmp::Reverse;

use super::{Attribution, Resolution, SymbolRef, SymbolResolver, under};
use crate::packages::model::Ecosystem;
use crate::packages::project::EcosystemPackages;

/// A leghosszabb névtér-előtag csomagja.
pub(super) struct Psr4Prefix;

impl SymbolResolver for Psr4Prefix {
    fn resolve(&self, sym: &SymbolRef<'_>, part: &EcosystemPackages) -> Option<Resolution> {
        if sym.ecosystem != Ecosystem::Php {
            return None;
        }
        part.packages
            .iter()
            .chain(&part.installed_only)
            .flat_map(|package| {
                package
                    .namespaces
                    .iter()
                    .map(move |prefix| (package, prefix))
            })
            .filter(|(_, prefix)| under(sym.qualifier, prefix))
            // a leghosszabb; azonos hosszon az első (a közvetlen függőség a tranzitív előtt)
            .min_by_key(|(_, prefix)| Reverse(prefix.len()))
            .map(|(package, _)| Resolution::Package(Attribution::of(package)))
    }
}
