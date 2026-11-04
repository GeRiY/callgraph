//! Laravel-stratégia: ismert modell-ős és globálisan alias-olt facade-ek.

use super::FrameworkStrategy;

/// Az `ast_graph.php` `FRAMEWORK_KNOWN_SHORT_NAMES['laravel']` listája.
const KNOWN_SHORT_NAMES: &[&str] = &[
    "DB",
    "Auth",
    "Cache",
    "Route",
    "Log",
    "Session",
    "Validator",
    "Storage",
    "Mail",
    "Queue",
    "Config",
    "View",
    "App",
    "Artisan",
    "Event",
    "Gate",
    "Hash",
    "Http",
    "Notification",
    "Password",
    "Redirect",
    "Redis",
    "Request",
    "Response",
    "Schema",
    "URL",
    "Cookie",
    "Blade",
    "File",
    "Lang",
    "RateLimiter",
    "Bus",
    "Broadcast",
    "Vite",
];

/// Laravel: az Eloquent `Model` az ismert modell-ős.
#[derive(Debug, Default, Clone, Copy)]
pub struct LaravelStrategy;

impl FrameworkStrategy for LaravelStrategy {
    fn name(&self) -> Option<&'static str> {
        Some("laravel")
    }

    fn model_bases(&self) -> &'static [&'static str] {
        &["Illuminate\\Database\\Eloquent\\Model"]
    }

    fn known_short_names(&self) -> &'static [&'static str] {
        KNOWN_SHORT_NAMES
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_facade_without_import_is_framework_use() {
        assert!(LaravelStrategy.is_framework_use("DB", false));
        assert!(LaravelStrategy.is_framework_use("Vite", false));
    }

    #[test]
    fn imported_or_unknown_name_is_not() {
        assert!(!LaravelStrategy.is_framework_use("DB", true));
        assert!(!LaravelStrategy.is_framework_use("Foo", false));
        assert!(!LaravelStrategy.is_framework_use("db", false));
    }

    #[test]
    fn list_has_34_names() {
        assert_eq!(LaravelStrategy.known_short_names().len(), 34);
    }
}
