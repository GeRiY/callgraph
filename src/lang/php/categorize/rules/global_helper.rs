//! Nem natív, globális PHP-függvények.
use crate::lang::php::categorize::{CallInfo, CategoryRule};
pub struct GlobalHelperRule;
impl CategoryRule for GlobalHelperRule {
    fn categorize(&self, call: &CallInfo<'_>) -> Option<&'static str> {
        (call.kind == "func_call").then_some("global_helper")
    }
}
