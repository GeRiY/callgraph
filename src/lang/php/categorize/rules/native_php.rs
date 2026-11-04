//! PHP beépített függvényei.

use crate::lang::php::categorize::data::is_native_php;
use crate::lang::php::categorize::{CallInfo, CategoryRule};

/// A PHP beépített függvényeinek felismerése.
#[derive(Debug, Default, Clone, Copy)]
pub struct NativePhpRule;

impl CategoryRule for NativePhpRule {
    fn categorize(&self, call: &CallInfo<'_>) -> Option<&'static str> {
        (call.kind == "func_call" && is_native_php(call.target)).then_some("native_php")
    }
}
