//! A jelenlegi osztály belső segédhívásai.

use crate::lang::php::categorize::{CallInfo, CategoryRule};

/// Az osztályon belüli segéd-hívások felismerése.
#[derive(Debug, Default, Clone, Copy)]
pub struct InternalHelperRule;

impl CategoryRule for InternalHelperRule {
    fn categorize(&self, call: &CallInfo<'_>) -> Option<&'static str> {
        let self_access = matches!(call.kind, "const_fetch" | "static_property_fetch")
            && call.target.split("::").next().is_some_and(|name| {
                name.eq_ignore_ascii_case("self") || name.eq_ignore_ascii_case("static")
            });
        (call.is_this || self_access).then_some("internal_helper")
    }
}
