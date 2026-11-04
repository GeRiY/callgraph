//! Statikus hívás alapértelmezett kategóriája.
use crate::lang::php::categorize::{CallInfo, CategoryRule};
pub struct StaticCallRule;
impl CategoryRule for StaticCallRule {
    fn categorize(&self, call: &CallInfo<'_>) -> Option<&'static str> {
        (call.kind == "static_call").then_some("static_call")
    }
}
