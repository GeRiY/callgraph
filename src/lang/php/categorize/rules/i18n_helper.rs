//! A `pw::` fordítási helper.
use crate::lang::php::categorize::{CallInfo, CategoryRule};
pub struct I18nHelperRule;
impl CategoryRule for I18nHelperRule {
    fn categorize(&self, call: &CallInfo<'_>) -> Option<&'static str> {
        (call.kind == "static_call" && call.target.split("::").next() == Some("pw"))
            .then_some("i18n_helper")
    }
}
