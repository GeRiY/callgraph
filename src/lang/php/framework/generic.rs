//! Általános PHP (nincs felismert framework).

use super::FrameworkStrategy;

/// Alapértelmezett stratégia: nincs framework-specifikus név.
#[derive(Debug, Default, Clone, Copy)]
pub struct GenericStrategy;

impl FrameworkStrategy for GenericStrategy {
    fn name(&self) -> Option<&'static str> {
        None
    }

    fn model_bases(&self) -> &'static [&'static str] {
        &[]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_framework_use() {
        assert_eq!(GenericStrategy.name(), None);
        assert!(GenericStrategy.known_short_names().is_empty());
        assert!(!GenericStrategy.is_framework_use("DB", false));
    }
}
