//! Lokális és property-típusok követése a hívás-célpontok feloldásához.

use std::collections::HashMap;

/// Egy kifejezés (pl. `$this->repo`) típusának kikövetkeztetése.
pub trait TypeTracker {
    /// A kifejezés teljes osztálynevét adja, ha ismert.
    fn type_of(&self, expr: &str) -> Option<String>;
}

/// Egy metódus lokális változóinak és az aktuális osztály property-jeinek típusai.
#[derive(Debug, Default, Clone)]
pub struct TypeMap {
    types: HashMap<String, String>,
}

impl TypeMap {
    pub fn set(&mut self, expr: impl Into<String>, ty: impl Into<String>) {
        self.types.insert(expr.into(), ty.into());
    }

    pub fn clear_locals(&mut self) {
        self.types.retain(|key, _| key.starts_with("this->"));
    }

    /// A property-típusok (`this->…`) törlése — a referencia minden osztályba (a névtelenbe is) belépéskor üríti őket.
    pub fn clear_properties(&mut self) {
        self.types.retain(|key, _| !key.starts_with("this->"));
    }
}

impl TypeTracker for TypeMap {
    fn type_of(&self, expr: &str) -> Option<String> {
        self.types.get(expr).cloned()
    }
}

/// Alapértelmezett, üres tracker kompatibilitási célból.
#[derive(Debug, Default, Clone, Copy)]
pub struct NullTypeTracker;

impl TypeTracker for NullTypeTracker {
    fn type_of(&self, _expr: &str) -> Option<String> {
        None
    }
}
