//! A statikusan fel nem oldható (futásidőben eldőlő) hívás jelölése.
use crate::lang::php::categorize::{CallInfo, CategoryRule};

/// A `dynamic` típusú hívás kategóriája `dynamic_call` (a lánc elején: más szabály nem írhatja felül).
pub struct DynamicCallRule;

impl CategoryRule for DynamicCallRule {
    fn categorize(&self, call: &CallInfo<'_>) -> Option<&'static str> {
        (call.kind == crate::model::DYNAMIC_KIND).then_some(crate::model::DYNAMIC_CALL)
    }
}
