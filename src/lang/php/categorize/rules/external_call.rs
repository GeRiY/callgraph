//! A kategorizálatlan külső metódus/property/konstans elérés.
use crate::lang::php::categorize::{CallInfo, CategoryRule};
pub struct ExternalCallRule;
impl CategoryRule for ExternalCallRule {
    fn categorize(&self, call: &CallInfo<'_>) -> Option<&'static str> {
        matches!(
            call.kind,
            "method_call" | "nullsafe_method_call" | "static_property_fetch" | "const_fetch"
        )
        .then_some("external_call")
    }
}
