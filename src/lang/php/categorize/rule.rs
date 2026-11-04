//! A kategorizálási szabályok közös adata és felülete.

use crate::core::Rule;
use crate::lang::php::framework::FrameworkStrategy;

/// Egy hívás kategorizáláshoz szükséges adatai.
#[derive(Clone, Copy)]
pub struct CallInfo<'a> {
    /// `func_call`, `method_call`, `static_call`, `new`, ...
    pub kind: &'a str,
    pub target: &'a str,
    pub on: Option<&'a str>,
    pub resolved: Option<&'a str>,
    pub class_short_name: Option<&'a str>,
    pub has_use_import: bool,
    pub is_this: bool,
    pub framework: &'a dyn FrameworkStrategy,
}

/// A lánc egy láncszeme: illeszkedés esetén a kategória nevét adja, különben `None`-t.
pub trait CategoryRule {
    fn categorize(&self, call: &CallInfo<'_>) -> Option<&'static str>;
}

/// Minden `CategoryRule` egyben `core::Rule<CallInfo>` is, így a generikus lánc futtatja.
impl<T: CategoryRule + ?Sized> Rule<CallInfo<'_>> for T {
    type Output = &'static str;

    fn apply(&self, call: &CallInfo<'_>) -> Option<&'static str> {
        self.categorize(call)
    }
}
