//! Ismeretlen osztály példányosítása.
use crate::lang::php::categorize::{CallInfo, CategoryRule};
pub struct InstantiationRule;
impl CategoryRule for InstantiationRule {
    fn categorize(&self, call: &CallInfo<'_>) -> Option<&'static str> {
        (call.kind == "new").then_some("instantiation")
    }
}
