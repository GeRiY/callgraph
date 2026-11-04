//! A kategória-lánc: a `core::Chain` PHP-példánya — az első illeszkedő szabály dönt.

use super::rule::{CallInfo, CategoryRule};
use super::rules;
use crate::core::Chain;

/// Rendezett szabálylista (a sorrend a prioritás); tartalék: `external_call`.
pub struct CategoryChain {
    chain: Chain<dyn CategoryRule, &'static str>,
}

impl CategoryChain {
    pub fn new(rules: Vec<Box<dyn CategoryRule>>) -> Self {
        Self {
            chain: Chain::new(rules, "external_call"),
        }
    }

    /// Az alapértelmezett lánc.
    pub fn with_defaults() -> Self {
        Self::new(rules::default_rules())
    }

    /// Az első illeszkedő szabály kategóriája; ha egy sem illik: `external_call`.
    pub fn categorize(&self, call: &CallInfo<'_>) -> &'static str {
        self.chain.resolve(call)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Always(&'static str);
    impl CategoryRule for Always {
        fn categorize(&self, _: &CallInfo<'_>) -> Option<&'static str> {
            Some(self.0)
        }
    }

    fn call() -> CallInfo<'static> {
        CallInfo {
            kind: "func_call",
            target: "x",
            on: None,
            resolved: None,
            class_short_name: None,
            has_use_import: false,
            is_this: false,
            framework: &crate::lang::php::framework::GenericStrategy,
        }
    }

    #[test]
    fn first_matching_rule_wins() {
        let c = CategoryChain::new(vec![Box::new(Always("a")), Box::new(Always("b"))]);
        assert_eq!(c.categorize(&call()), "a");
    }

    #[test]
    fn falls_back_to_external_call() {
        assert_eq!(
            CategoryChain::new(vec![]).categorize(&call()),
            "external_call"
        );
    }
}
