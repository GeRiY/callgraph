//! `components\\` névtérből érkező hívások.
use crate::lang::php::categorize::{CallInfo, CategoryRule};
pub struct ComponentUseRule;
impl CategoryRule for ComponentUseRule {
    fn categorize(&self, call: &CallInfo<'_>) -> Option<&'static str> {
        call.resolved?
            .to_ascii_lowercase()
            .contains("components\\")
            .then_some("component_use")
    }
}
