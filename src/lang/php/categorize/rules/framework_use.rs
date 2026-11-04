//! Framework-névtér és import nélküli ismert framework-nevek.
use crate::lang::php::categorize::{CallInfo, CategoryRule};
const NAMESPACES: &[&str] = &[
    "illuminate\\",
    "symfony\\",
    "doctrine\\",
    "laravel\\",
    "inertia\\",
    "guzzlehttp\\",
    "monolog\\",
    "carbon\\",
    "psr\\",
];
pub struct FrameworkUseRule;
impl CategoryRule for FrameworkUseRule {
    fn categorize(&self, call: &CallInfo<'_>) -> Option<&'static str> {
        let namespace = call.resolved.is_some_and(|name| {
            let lower = name.to_ascii_lowercase();
            NAMESPACES.iter().any(|prefix| lower.starts_with(prefix))
        });
        let short = call
            .class_short_name
            .unwrap_or(call.target.split("::").next().unwrap_or(call.target));
        (namespace || call.framework.is_framework_use(short, call.has_use_import))
            .then_some("framework_use")
    }
}
