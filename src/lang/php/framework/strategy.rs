//! A framework-specifikus tudás felülete.

/// Framework-specifikus ismert nevek (a kategorizálás ezeket használja).
pub trait FrameworkStrategy {
    /// A framework neve a kimenet `framework` mezőjéhez; `None` = általános PHP.
    fn name(&self) -> Option<&'static str>;

    /// Ismert modell-ős osztályok teljes nevei.
    fn model_bases(&self) -> &'static [&'static str];

    /// Ismert facade/helper rövidnevek, amik `use` import nélkül is framework-kódnak számítanak.
    fn known_short_names(&self) -> &'static [&'static str] {
        &[]
    }

    /// `categorizeNamespace` framework-ága: `framework_use`, ha a rövid névhez nincs
    /// explicit `use` import, és a név a framework ismert rövidnevei közt van.
    fn is_framework_use(&self, short_name: &str, has_import: bool) -> bool {
        self.name().is_some() && !has_import && self.known_short_names().contains(&short_name)
    }
}
