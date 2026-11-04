//! `helper(s)\\` névtérből érkező hívások.
use crate::lang::php::categorize::{CallInfo, CategoryRule};
pub struct HelperUseRule;
impl CategoryRule for HelperUseRule {
    fn categorize(&self, call: &CallInfo<'_>) -> Option<&'static str> {
        let name = call.resolved?.to_ascii_lowercase();
        (name.contains("helpers\\") || name.contains("helper\\")).then_some("helper_use")
    }
}
