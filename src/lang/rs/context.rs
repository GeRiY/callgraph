//! Az elemzés közös állapota: a fájl névtáblája (a visitorok töltik és olvassák) és a figyelmeztetések.

use super::module_path::ModulePath;
use super::resolve::Resolver;

/// Egy fájl elemzésének állapota.
pub(crate) struct RsContext {
    pub resolver: Resolver,
    pub warnings: Vec<String>,
}

impl RsContext {
    pub(crate) fn new(path: &str) -> Self {
        Self {
            resolver: Resolver::new(ModulePath::from_file(path)),
            warnings: Vec::new(),
        }
    }
}
