//! Yii-stratégia. Az `ast_graph.php` nem ismer Yii-listát; csak a globális `Yii` helper szerepel.

use super::FrameworkStrategy;

const KNOWN_SHORT_NAMES: &[&str] = &["Yii"];

/// Yii: az `ActiveRecord` az ismert modell-ős.
#[derive(Debug, Default, Clone, Copy)]
pub struct YiiStrategy;

impl FrameworkStrategy for YiiStrategy {
    fn name(&self) -> Option<&'static str> {
        Some("yii")
    }

    fn model_bases(&self) -> &'static [&'static str] {
        &["yii\\db\\ActiveRecord"]
    }

    fn known_short_names(&self) -> &'static [&'static str] {
        KNOWN_SHORT_NAMES
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yii_helper_without_import_is_framework_use() {
        assert!(YiiStrategy.is_framework_use("Yii", false));
        assert!(!YiiStrategy.is_framework_use("Yii", true));
    }

    #[test]
    fn laravel_facade_is_not_yii() {
        assert!(!YiiStrategy.is_framework_use("DB", false));
    }
}
