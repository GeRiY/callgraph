//! Láncszem: a projekt saját psr-4 / psr-0 névtere (`composer.json` `autoload`, `autoload-dev`) → saját kód.

use super::{Resolution, SymbolRef, SymbolResolver, under};
use crate::packages::model::Ecosystem;
use crate::packages::project::EcosystemPackages;

/// A saját névtér alatti PHP-név: `Own`.
pub(super) struct OwnNamespace;

impl SymbolResolver for OwnNamespace {
    fn resolve(&self, sym: &SymbolRef<'_>, part: &EcosystemPackages) -> Option<Resolution> {
        if sym.ecosystem != Ecosystem::Php {
            return None;
        }
        part.own_namespaces
            .iter()
            .any(|prefix| under(sym.qualifier, prefix))
            .then_some(Resolution::Own)
    }
}
